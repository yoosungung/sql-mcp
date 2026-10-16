use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Column, PgPool, Row, TypeInfo};

use crate::dialect::DialectProfile;
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
        let limited = DialectProfile::Postgres.wrap_row_limit(sql, max_rows);
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
        let d = DialectProfile::Postgres;
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

fn pg_value_to_json(row: &sqlx::postgres::PgRow, idx: usize) -> Result<Value, AppError> {
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
    if let Ok(raw) = row.try_get_raw(idx) {
        if let Ok(s) = raw.as_str() {
            return Ok(parse_numeric_or_string(s, type_name));
        }
        if let Ok(bytes) = raw.as_bytes() {
            if type_name.eq_ignore_ascii_case("NUMERIC") {
                if let Some(num_str) = decode_pg_numeric_binary(bytes) {
                    return Ok(parse_numeric_or_string(&num_str, "NUMERIC"));
                }
            }
            if type_name.eq_ignore_ascii_case("DATE") && bytes.len() == 4 {
                let days = i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                return Ok(Value::String(pg_days_to_ymd(days)));
            }
            if let Ok(s) = std::str::from_utf8(bytes) {
                return Ok(parse_numeric_or_string(s, type_name));
            }
        }
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "unsupported postgres type {type_name}"
    )))
}

fn parse_numeric_or_string(s: &str, type_name: &str) -> Value {
    if matches!(
        type_name.to_ascii_uppercase().as_str(),
        "NUMERIC" | "DECIMAL" | "INT2" | "INT4" | "INT8" | "FLOAT4" | "FLOAT8"
    ) {
        if let Ok(i) = s.parse::<i64>() {
            return json!(i);
        }
        if let Ok(f) = s.parse::<f64>() {
            if let Some(n) = serde_json::Number::from_f64(f) {
                return Value::Number(n);
            }
        }
    }
    Value::String(s.to_string())
}

/// Decode PostgreSQL binary NUMERIC format (base-10000 digits) into a decimal string.
pub(crate) fn decode_pg_numeric_binary(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 8 {
        return None;
    }
    let ndigits = i16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    let weight = i16::from_be_bytes([bytes[2], bytes[3]]) as i32;
    let sign = u16::from_be_bytes([bytes[4], bytes[5]]);
    let dscale = i16::from_be_bytes([bytes[6], bytes[7]]).max(0) as usize;
    if bytes.len() < 8 + ndigits * 2 {
        return None;
    }
    if sign == 0xC000 {
        return Some("NaN".to_string());
    }
    let mut digits = Vec::with_capacity(ndigits);
    for i in 0..ndigits {
        let off = 8 + i * 2;
        digits.push(i16::from_be_bytes([bytes[off], bytes[off + 1]]) as u32);
    }
    let mut out = String::new();
    if sign == 0x4000 {
        out.push('-');
    }
    // Integer part: groups 0..=weight
    if weight < 0 {
        out.push('0');
    } else {
        let mut started = false;
        for idx in 0..=weight {
            let d = digits.get(idx as usize).copied().unwrap_or(0);
            if !started {
                out.push_str(&d.to_string());
                started = true;
            } else {
                out.push_str(&format!("{d:04}"));
            }
        }
        if !started {
            out.push('0');
        }
    }
    // Fractional part
    if dscale > 0 {
        out.push('.');
        let mut frac = String::new();
        let start_idx = weight + 1;
        let groups_needed = (dscale + 3) / 4;
        for g in 0..(groups_needed as i32) {
            let idx = start_idx + g;
            let d = if idx < 0 {
                0
            } else {
                digits.get(idx as usize).copied().unwrap_or(0)
            };
            frac.push_str(&format!("{d:04}"));
        }
        frac.truncate(dscale);
        out.push_str(&frac);
    }
    Some(out)
}

/// Convert Postgres DATE (days since 2000-01-01) to `YYYY-MM-DD`.
pub(crate) fn pg_days_to_ymd(days_since_2000: i32) -> String {
    // Civil date from days since 1970-01-01 (2000-01-01 is 10957 days after 1970-01-01).
    let z = days_since_2000 as i64 + 10_957 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    format!("{year:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_pg_binary_numeric_and_date() {
        // 12345.67 -> ndigits=3, weight=1, sign=0x0000, dscale=2, digits=[1, 2345, 6700]
        let bytes: [u8; 14] = [
            0, 3, // ndigits = 3
            0, 1, // weight = 1
            0, 0, // sign = positive
            0, 2, // dscale = 2
            0, 1, // 1
            0x09, 0x29, // 2345
            0x1a, 0x2c, // 6700
        ];
        assert_eq!(decode_pg_numeric_binary(&bytes).as_deref(), Some("12345.67"));
        assert_eq!(pg_days_to_ymd(0), "2000-01-01");
        assert_eq!(pg_days_to_ymd(9131), "2024-12-31");
        assert_eq!(pg_days_to_ymd(9132), "2025-01-01");
    }
}
