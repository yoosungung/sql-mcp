use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Column, PgPool, Row, TypeInfo};

use crate::error::AppError;
use crate::select_guard::assert_select_only;

use super::registry::TargetInfo;
use super::{ColumnInfo, QueryResult, TableInfo, TargetDb};

pub struct PostgresTarget {
    info: TargetInfo,
    pool: PgPool,
}

impl PostgresTarget {
    pub async fn connect(info: TargetInfo, url: &str) -> Result<Self, AppError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        Ok(Self { info, pool })
    }
}

#[async_trait]
impl TargetDb for PostgresTarget {
    fn info(&self) -> TargetInfo {
        self.info.clone()
    }

    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, AppError> {
        let rows = if let Some(schema) = schema {
            sqlx::query(
                r#"
                SELECT table_schema, table_name
                FROM information_schema.tables
                WHERE table_type = 'BASE TABLE' AND table_schema = $1
                ORDER BY table_schema, table_name
                "#,
            )
            .bind(schema)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query(
                r#"
                SELECT table_schema, table_name
                FROM information_schema.tables
                WHERE table_type = 'BASE TABLE'
                  AND table_schema NOT IN ('pg_catalog', 'information_schema')
                ORDER BY table_schema, table_name
                "#,
            )
            .fetch_all(&self.pool)
            .await
        }
        .map_err(|e| AppError::Connection(e.into()))?;

        Ok(rows
            .iter()
            .map(|r| TableInfo {
                schema: r.get::<String, _>("table_schema"),
                name: r.get::<String, _>("table_name"),
            })
            .collect())
    }

    async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Vec<ColumnInfo>, AppError> {
        let schema = schema.unwrap_or("public");
        let rows = sqlx::query(
            r#"
            SELECT column_name, data_type, is_nullable
            FROM information_schema.columns
            WHERE table_schema = $1 AND table_name = $2
            ORDER BY ordinal_position
            "#,
        )
        .bind(schema)
        .bind(table)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Connection(e.into()))?;
        if rows.is_empty() {
            return Err(AppError::NotFound(format!("{schema}.{table}")));
        }
        Ok(rows
            .iter()
            .map(|r| ColumnInfo {
                name: r.get::<String, _>("column_name"),
                data_type: r.get::<String, _>("data_type"),
                nullable: r.get::<String, _>("is_nullable") == "YES",
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
                vals.push(pg_value_to_json(row, i)?);
            }
            out_rows.push(vals);
        }
        if out_rows.len() >= max_rows {
            // Soft signal: caller may treat exact max as possible truncation.
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
        let schema = schema.unwrap_or("public");
        // Identifiers: only allow simple names.
        for ident in [schema, table, column] {
            if !ident
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(AppError::Validation("invalid identifier".into()));
            }
        }
        let sql = format!(
            r#"SELECT "{column}" AS v FROM "{schema}"."{table}" WHERE "{column}" IS NOT NULL LIMIT {limit}"#
        );
        let result = self.execute_select(&sql, limit).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next())
            .collect())
    }
}

fn pg_value_to_json(row: &sqlx::postgres::PgRow, idx: usize) -> Result<Value, AppError> {
    let col = &row.columns()[idx];
    let type_name = col.type_info().name();
    // Try common types; fall back to string display via try_get Option<String>.
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
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
        return Ok(v.unwrap_or(Value::Null));
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "unsupported postgres type {type_name}"
    )))
}
