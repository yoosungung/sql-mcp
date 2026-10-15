use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Column, Row, SqlitePool, TypeInfo};

use crate::error::AppError;
use crate::select_guard::assert_select_only;

use super::registry::TargetInfo;
use super::{ColumnInfo, QueryResult, TableInfo, TargetDb};

pub struct SqliteTarget {
    info: TargetInfo,
    pool: SqlitePool,
}

impl SqliteTarget {
    pub async fn connect(info: TargetInfo, url: &str) -> Result<Self, AppError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        Ok(Self { info, pool })
    }
}

#[async_trait]
impl TargetDb for SqliteTarget {
    fn info(&self) -> TargetInfo {
        self.info.clone()
    }

    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, AppError> {
        if let Some(schema) = schema {
            if schema != "main" {
                return Ok(vec![]);
            }
        }
        let rows = sqlx::query(
            r#"
            SELECT name AS table_name
            FROM sqlite_master
            WHERE type = 'table'
              AND name NOT LIKE 'sqlite_%'
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Connection(e.into()))?;

        Ok(rows
            .iter()
            .map(|r| TableInfo {
                schema: "main".into(),
                name: r.get::<String, _>("table_name"),
            })
            .collect())
    }

    async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Vec<ColumnInfo>, AppError> {
        if let Some(schema) = schema {
            if schema != "main" {
                return Err(AppError::NotFound(format!("{schema}.{table}")));
            }
        }
        validate_ident(table)?;
        // PRAGMA table_info: cid, name, type, notnull, dflt_value, pk
        let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""));
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        if rows.is_empty() {
            return Err(AppError::NotFound(format!("main.{table}")));
        }
        Ok(rows
            .iter()
            .map(|r| {
                let notnull: i64 = r.get("notnull");
                ColumnInfo {
                    name: r.get::<String, _>("name"),
                    data_type: {
                        let t: String = r.get("type");
                        if t.is_empty() {
                            "NULL".into()
                        } else {
                            t
                        }
                    },
                    nullable: notnull == 0,
                }
            })
            .collect())
    }

    async fn execute_select(&self, sql: &str, max_rows: usize) -> Result<QueryResult, AppError> {
        assert_select_only(sql)?;
        // Audited: outer wrapper only; inner SQL already passed SELECT-only AST guard.
        let limited = format!("SELECT * FROM ({sql}) AS _sql_mcp_q LIMIT {max_rows}");
        let rows = sqlx::query(sqlx::AssertSqlSafe(limited))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        if rows.is_empty() {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
            });
        }
        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();
        let mut out_rows = Vec::with_capacity(rows.len());
        for row in &rows {
            let mut vals = Vec::with_capacity(columns.len());
            for (i, _) in columns.iter().enumerate() {
                vals.push(sqlite_value_to_json(row, i)?);
            }
            out_rows.push(vals);
        }
        Ok(QueryResult {
            columns,
            rows: out_rows,
        })
    }

    async fn sample_column(
        &self,
        schema: Option<&str>,
        table: &str,
        column: &str,
        limit: usize,
    ) -> Result<Vec<Value>, AppError> {
        if let Some(schema) = schema {
            if schema != "main" {
                return Err(AppError::NotFound(format!("{schema}.{table}")));
            }
        }
        let table_q = quote_ident(table)?;
        let column_q = quote_ident(column)?;
        let sql = format!(
            r#"SELECT {column_q} AS v FROM {table_q} WHERE {column_q} IS NOT NULL LIMIT {limit}"#
        );
        let result = self.execute_select(&sql, limit).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next())
            .collect())
    }
}

fn validate_ident(ident: &str) -> Result<(), AppError> {
    if ident.is_empty()
        || !ident
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '-'))
    {
        return Err(AppError::Validation("invalid identifier".into()));
    }
    Ok(())
}

fn quote_ident(ident: &str) -> Result<String, AppError> {
    validate_ident(ident)?;
    Ok(format!("\"{}\"", ident.replace('"', "\"\"")))
}

fn sqlite_value_to_json(row: &sqlx::sqlite::SqliteRow, idx: usize) -> Result<Value, AppError> {
    let col = &row.columns()[idx];
    let type_name = col.type_info().name();
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return Ok(match v {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return Ok(match v {
            Some(n) => json!(n),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return Ok(match v {
            Some(n) => json!(n),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return Ok(match v {
            Some(s) => Value::String(s),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return Ok(match v {
            Some(b) => {
                let mut s = String::with_capacity(2 + b.len() * 2);
                s.push_str("\\x");
                for byte in b {
                    s.push_str(&format!("{byte:02x}"));
                }
                Value::String(s)
            }
            None => Value::Null,
        });
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "unsupported sqlite type {type_name}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn sample_db() -> (tempfile::TempDir, SqliteTarget) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("birdish.sqlite");
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = SqlitePoolOptions::new()
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE hero (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL,
              "power level" INTEGER
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO hero (id, name, "power level") VALUES (1, 'A', 10), (2, 'B', 20)"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        drop(pool);
        let info = TargetInfo {
            id: "t".into(),
            name: "test".into(),
            engine: "sqlite".into(),
        };
        let target = SqliteTarget::connect(info, &format!("sqlite:{}?mode=ro", path.display()))
            .await
            .unwrap();
        (dir, target)
    }

    #[tokio::test]
    async fn list_tables_and_describe() {
        let (_dir, t) = sample_db().await;
        let tables = t.list_tables(None).await.unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "hero");
        assert_eq!(tables[0].schema, "main");
        let cols = t.describe_table(Some("main"), "hero").await.unwrap();
        assert!(cols.iter().any(|c| c.name == "power level"));
    }

    #[tokio::test]
    async fn execute_select_and_sample() {
        let (_dir, t) = sample_db().await;
        let q = t
            .execute_select("SELECT name FROM hero ORDER BY id", 10)
            .await
            .unwrap();
        assert_eq!(q.rows.len(), 2);
        assert_eq!(q.rows[0][0], json!("A"));
        let samples = t
            .sample_column(None, "hero", "power level", 5)
            .await
            .unwrap();
        assert_eq!(samples.len(), 2);
    }

    #[tokio::test]
    async fn rejects_non_select() {
        let (_dir, t) = sample_db().await;
        let err = t
            .execute_select("DELETE FROM hero", 10)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotSelect), "{err}");
    }
}

