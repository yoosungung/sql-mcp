use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{Column, MySqlPool, Row, TypeInfo};

use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::select_guard::assert_select_only;

use super::registry::TargetInfo;
use super::{ColumnInfo, QueryResult, TableInfo, TargetDb};

pub struct MysqlTarget {
    info: TargetInfo,
    pool: MySqlPool,
}

impl MysqlTarget {
    pub async fn connect(info: TargetInfo, url: &str) -> Result<Self, AppError> {
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        Ok(Self { info, pool })
    }
}

#[async_trait]
impl TargetDb for MysqlTarget {
    fn info(&self) -> TargetInfo {
        self.info.clone()
    }

    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, AppError> {
        let rows = if let Some(schema) = schema {
            sqlx::query(
                r#"
                SELECT table_schema, table_name
                FROM information_schema.tables
                WHERE table_type = 'BASE TABLE' AND table_schema = ?
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
                  AND table_schema NOT IN ('mysql', 'information_schema', 'performance_schema', 'sys')
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
            WHERE table_schema = ? AND table_name = ?
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
        let limited = DialectProfile::Mysql.wrap_row_limit(sql, max_rows);
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
                vals.push(mysql_value_to_json(row, i)?);
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
        let schema = schema.unwrap_or("public");
        let d = DialectProfile::Mysql;
        let col = d.quote_ident(column)?;
        let sch = d.quote_ident(schema)?;
        let tbl = d.quote_ident(table)?;
        let sql = format!("SELECT {col} AS v FROM {sch}.{tbl} WHERE {col} IS NOT NULL");
        let result = self.execute_select(&sql, limit).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next())
            .collect())
    }
}

fn mysql_value_to_json(row: &sqlx::mysql::MySqlRow, idx: usize) -> Result<Value, AppError> {
    use sqlx::ValueRef;

    if let Ok(raw) = row.try_get_raw(idx) {
        if raw.is_null() {
            return Ok(Value::Null);
        }
    }
    let col = &row.columns()[idx];
    let type_name = col.type_info().name();
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) {
        return Ok(v.map(Value::Bool).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) {
        return Ok(v.map(|n| json!(n)).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<u64>, _>(idx) {
        return Ok(v.map(|n| json!(n)).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) {
        return Ok(v.map(|n| json!(n)).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<i16>, _>(idx) {
        return Ok(v.map(|n| json!(n)).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) {
        return Ok(v.map(|n| json!(n)).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<f32>, _>(idx) {
        return Ok(v.map(|n| json!(n)).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) {
        return Ok(v.map(Value::String).unwrap_or(Value::Null));
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(idx) {
        return Ok(v.unwrap_or(Value::Null));
    }
    if let Ok(Some(bytes)) = row.try_get_unchecked::<Option<Vec<u8>>, _>(idx) {
        if let Ok(s) = std::str::from_utf8(&bytes) {
            if matches!(
                type_name.to_ascii_uppercase().as_str(),
                "DECIMAL" | "NEWDECIMAL"
            ) {
                if let Ok(i) = s.parse::<i64>() {
                    return Ok(json!(i));
                }
                if let Ok(f) = s.parse::<f64>() {
                    if let Some(n) = serde_json::Number::from_f64(f) {
                        return Ok(Value::Number(n));
                    }
                }
            }
            return Ok(Value::String(s.to_owned()));
        }
    }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) {
        return Ok(match v {
            Some(b) => Value::String(String::from_utf8_lossy(&b).into_owned()),
            None => Value::Null,
        });
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "unsupported mysql type {type_name}"
    )))
}
