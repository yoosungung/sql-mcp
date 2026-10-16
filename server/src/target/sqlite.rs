use std::sync::Arc;

use async_trait::async_trait;
use libsql::{Builder, Connection};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::error::AppError;
use crate::select_guard::assert_select_only;

use super::limit::wrap_select;
use super::registry::TargetInfo;
use super::{ColumnInfo, QueryResult, TableInfo, TargetDb};

pub struct SqliteTarget {
    info: TargetInfo,
    conn: Arc<Mutex<Connection>>,
}

impl SqliteTarget {
    pub async fn connect(info: TargetInfo, url: &str) -> Result<Self, AppError> {
        let (path, readonly) = parse_sqlite_url(url)?;
        let db = Builder::new_local(&path)
            .build()
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        let conn = db
            .connect()
            .map_err(|e| AppError::Connection(e.into()))?;
        if readonly {
            conn.execute("PRAGMA query_only=ON;", ())
                .await
                .map_err(|e| AppError::Connection(e.into()))?;
        }
        Ok(Self {
            info,
            conn: Arc::new(Mutex::new(conn)),
        })
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
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                r#"
                SELECT name AS table_name
                FROM sqlite_master
                WHERE type = 'table'
                  AND name NOT LIKE 'sqlite_%'
                ORDER BY name
                "#,
                (),
            )
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| AppError::Connection(e.into()))?
        {
            let name: String = row.get(0).map_err(|e| AppError::Connection(e.into()))?;
            out.push(TableInfo {
                schema: "main".into(),
                name,
            });
        }
        Ok(out)
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
        let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', "\"\""));
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(&sql, ())
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        let mut cols = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| AppError::Connection(e.into()))?
        {
            // cid, name, type, notnull, dflt_value, pk
            let name: String = row.get(1).map_err(|e| AppError::Connection(e.into()))?;
            let type_name: String = row.get::<Option<String>>(2).ok().flatten().unwrap_or_default();
            let notnull: i64 = row.get(3).map_err(|e| AppError::Connection(e.into()))?;
            cols.push(ColumnInfo {
                name,
                data_type: if type_name.is_empty() {
                    "NULL".into()
                } else {
                    type_name
                },
                nullable: notnull == 0,
            });
        }
        if cols.is_empty() {
            return Err(AppError::NotFound(format!("main.{table}")));
        }
        Ok(cols)
    }

    async fn execute_select(&self, sql: &str, max_rows: usize) -> Result<QueryResult, AppError> {
        assert_select_only(sql)?;
        // Audited: outer wrapper only; inner SQL already passed SELECT-only AST guard.
        let limited = wrap_select("sqlite", sql, max_rows);
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(&limited, ())
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        let col_count = rows.column_count();
        if col_count <= 0 {
            return Ok(QueryResult {
                columns: vec![],
                rows: vec![],
            });
        }
        let columns: Vec<String> = (0..col_count)
            .map(|i| {
                rows.column_name(i)
                    .unwrap_or("?")
                    .to_string()
            })
            .collect();
        let mut out_rows = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| AppError::Connection(e.into()))?
        {
            let mut vals = Vec::with_capacity(columns.len());
            for i in 0..col_count {
                vals.push(libsql_value_to_json(
                    row.get_value(i)
                        .map_err(|e| AppError::Connection(e.into()))?,
                ));
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
            r#"SELECT {column_q} AS v FROM {table_q} WHERE {column_q} IS NOT NULL"#
        );
        let result = self.execute_select(&sql, limit).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next())
            .collect())
    }
}

/// `sqlite:<path>?mode=ro` (sqlx 호환 DSN). `mode=ro`면 `PRAGMA query_only=ON`.
fn parse_sqlite_url(url: &str) -> Result<(String, bool), AppError> {
    let rest = url.strip_prefix("sqlite:").ok_or_else(|| {
        AppError::Validation("sqlite DSN must start with sqlite:".into())
    })?;
    let (path, query) = match rest.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (rest, None),
    };
    let path = path.strip_prefix("//").unwrap_or(path);
    if path.is_empty() {
        return Err(AppError::Validation("sqlite DSN missing path".into()));
    }
    let readonly = query
        .map(|q| {
            q.split('&').any(|p| {
                let mut parts = p.splitn(2, '=');
                parts.next() == Some("mode") && parts.next() == Some("ro")
            })
        })
        .unwrap_or(false);
    Ok((path.to_string(), readonly))
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

fn libsql_value_to_json(v: libsql::Value) -> Value {
    match v {
        libsql::Value::Null => Value::Null,
        libsql::Value::Integer(n) => json!(n),
        libsql::Value::Real(n) => json!(n),
        libsql::Value::Text(s) => Value::String(s),
        libsql::Value::Blob(b) => {
            let mut s = String::with_capacity(2 + b.len() * 2);
            s.push_str("\\x");
            for byte in b {
                s.push_str(&format!("{byte:02x}"));
            }
            Value::String(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn sample_db() -> (tempfile::TempDir, SqliteTarget) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("birdish.sqlite");
        let path_str = path.to_string_lossy().into_owned();
        {
            let db = Builder::new_local(&path_str).build().await.unwrap();
            let conn = db.connect().unwrap();
            conn.execute(
                r#"
                CREATE TABLE hero (
                  id INTEGER PRIMARY KEY,
                  name TEXT NOT NULL,
                  "power level" INTEGER
                )
                "#,
                (),
            )
            .await
            .unwrap();
            conn.execute(
                r#"INSERT INTO hero (id, name, "power level") VALUES (1, 'A', 10), (2, 'B', 20)"#,
                (),
            )
            .await
            .unwrap();
        }
        let info = TargetInfo {
            id: "t".into(),
            name: "test".into(),
            engine: "sqlite".into(),
        };
        let target = SqliteTarget::connect(info, &format!("sqlite:{path_str}?mode=ro"))
            .await
            .unwrap();
        (dir, target)
    }

    #[test]
    fn parse_dsn_path_and_readonly() {
        let (path, ro) = parse_sqlite_url("sqlite:/tmp/a.sqlite?mode=ro").unwrap();
        assert_eq!(path, "/tmp/a.sqlite");
        assert!(ro);
        let (path, ro) = parse_sqlite_url("sqlite:./rel.db").unwrap();
        assert_eq!(path, "./rel.db");
        assert!(!ro);
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
