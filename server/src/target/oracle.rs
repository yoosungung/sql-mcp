use std::sync::Arc;

use async_trait::async_trait;
use oracledb::{Pool, PoolConfig};
use serde_json::{Value, json};
use tokio::task;

use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::select_guard::assert_select_only;

use super::registry::TargetInfo;
use super::{ColumnInfo, QueryResult, TableInfo, TargetDb};

fn ora_err(e: oracledb::Error) -> AppError {
    AppError::Connection(anyhow::anyhow!("{e}"))
}

pub struct OracleTarget {
    info: TargetInfo,
    pool: Arc<Pool>,
}

impl OracleTarget {
    pub async fn connect(info: TargetInfo, url: &str) -> Result<Self, AppError> {
        let parsed = parse_oracle_dsn(url)?;
        let pool = task::spawn_blocking(move || {
            let cfg = PoolConfig::default()
                .set_credentials(&parsed.user, &parsed.password)
                .set_connect_string(&parsed.connect_string)
                .map_err(|e| ora_err(e))?
                .set_min_connections(0)
                .set_max_connections(5);
            oracledb::create_pool(cfg).map_err(|e| ora_err(e))
        })
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("oracle connect join: {e}")))??;
        Ok(Self {
            info,
            pool: Arc::new(pool),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OracleDsn {
    user: String,
    password: String,
    connect_string: String,
}

/// Accepts:
/// - `oracle://user:password@host:1521/service`
/// - `user/password@host:1521/service`
/// - `user/password@//host:1521/service`
fn parse_oracle_dsn(url: &str) -> Result<OracleDsn, AppError> {
    let rest = url
        .strip_prefix("oracle://")
        .or_else(|| url.strip_prefix("oracle:"))
        .unwrap_or(url);

    let (creds, hostpart) = rest.split_once('@').ok_or_else(|| {
        AppError::Validation(
            "oracle DSN must be user/password@host:port/service or oracle://user:pass@host:port/service"
                .into(),
        )
    })?;

    let (user, password) = if let Some((u, p)) = creds.split_once('/') {
        (u, p)
    } else if let Some((u, p)) = creds.split_once(':') {
        (u, p)
    } else {
        return Err(AppError::Validation(
            "oracle DSN needs user/password or user:password before @".into(),
        ));
    };

    if user.is_empty() || hostpart.is_empty() {
        return Err(AppError::Validation(
            "oracle DSN missing user or connect string".into(),
        ));
    }

    Ok(OracleDsn {
        user: percent_decode(user),
        password: percent_decode(password),
        connect_string: hostpart.to_string(),
    })
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(char::from((h * 16 + l) as u8));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[async_trait]
impl TargetDb for OracleTarget {
    fn info(&self) -> TargetInfo {
        self.info.clone()
    }

    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, AppError> {
        let pool = Arc::clone(&self.pool);
        let schema = schema.map(|s| s.to_string());
        task::spawn_blocking(move || {
            let conn = pool
                .acquire()
                .map_err(|e| ora_err(e))?;
            let sql = if schema.is_some() {
                r#"
                SELECT owner AS table_schema, table_name
                FROM all_tables
                WHERE owner = :1
                ORDER BY owner, table_name
                "#
            } else {
                r#"
                SELECT owner AS table_schema, table_name
                FROM all_tables
                WHERE owner NOT IN ('SYS', 'SYSTEM', 'XDB', 'CTXSYS', 'MDSYS', 'OLAPSYS')
                ORDER BY owner, table_name
                "#
            };
            let cursor = if let Some(ref s) = schema {
                conn.query(sql, &[&s.as_str()])
            } else {
                conn.query(sql, &[])
            }
            .map_err(|e| ora_err(e))?;
            let mut out = Vec::new();
            for row in cursor {
                let row = row.map_err(|e| ora_err(e))?;
                out.push(TableInfo {
                    schema: row
                        .get::<String>(0)
                        .map_err(|e| ora_err(e))?,
                    name: row
                        .get::<String>(1)
                        .map_err(|e| ora_err(e))?,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("oracle list_tables join: {e}")))?
    }

    async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Vec<ColumnInfo>, AppError> {
        let pool = Arc::clone(&self.pool);
        let schema = schema.unwrap_or("").to_string();
        let table = table.to_string();
        task::spawn_blocking(move || {
            let conn = pool
                .acquire()
                .map_err(|e| ora_err(e))?;
            let owner = if schema.is_empty() {
                conn.query_row("SELECT USER FROM dual", &[])
                    .map_err(|e| ora_err(e))?
                    .get::<String>(0)
                    .map_err(|e| ora_err(e))?
            } else {
                schema
            };
            let cursor = conn
                .query(
                    r#"
                    SELECT column_name, data_type, nullable
                    FROM all_tab_columns
                    WHERE owner = :1 AND table_name = :2
                    ORDER BY column_id
                    "#,
                    &[&owner.as_str(), &table.as_str()],
                )
                .map_err(|e| ora_err(e))?;
            let mut out = Vec::new();
            for row in cursor {
                let row = row.map_err(|e| ora_err(e))?;
                let nullable: String = row
                    .get(2)
                    .map_err(|e| ora_err(e))?;
                out.push(ColumnInfo {
                    name: row
                        .get::<String>(0)
                        .map_err(|e| ora_err(e))?,
                    data_type: row
                        .get::<String>(1)
                        .map_err(|e| ora_err(e))?,
                    nullable: nullable == "Y",
                });
            }
            if out.is_empty() {
                return Err(AppError::NotFound(format!("{owner}.{table}")));
            }
            Ok(out)
        })
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("oracle describe_table join: {e}")))?
    }

    async fn execute_select(&self, sql: &str, max_rows: usize) -> Result<QueryResult, AppError> {
        assert_select_only(sql)?;
        let limited = DialectProfile::Oracle.wrap_row_limit(sql, max_rows);
        let pool = Arc::clone(&self.pool);
        task::spawn_blocking(move || run_select(&pool, &limited))
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("oracle execute_select join: {e}")))?
    }

    async fn sample_column(
        &self,
        schema: Option<&str>,
        table: &str,
        column: &str,
        limit: usize,
    ) -> Result<Vec<Value>, AppError> {
        let schema = schema.unwrap_or("");
        for ident in [schema, table, column] {
            if !ident.is_empty()
                && !ident
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(AppError::Validation("invalid identifier".into()));
            }
        }
        let d = DialectProfile::Oracle;
        let col = d.quote_ident(column)?;
        let sql = if schema.is_empty() {
            let tbl = d.quote_ident(table)?;
            format!("SELECT {col} AS v FROM {tbl} WHERE {col} IS NOT NULL")
        } else {
            let from = d.qualify_table(schema, table)?;
            format!("SELECT {col} AS v FROM {from} WHERE {col} IS NOT NULL")
        };
        let result = self.execute_select(&sql, limit).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next())
            .collect())
    }
}

fn run_select(pool: &Pool, sql: &str) -> Result<QueryResult, AppError> {
    let conn = pool
        .acquire()
        .map_err(|e| ora_err(e))?;
    let cursor = conn
        .query(sql, &[])
        .map_err(|e| ora_err(e))?;
    let mut columns: Vec<String> = Vec::new();
    let mut out_rows: Vec<Vec<Value>> = Vec::new();
    for row in cursor {
        let row = row.map_err(|e| ora_err(e))?;
        if columns.is_empty() {
            columns = row.columns().iter().map(|m| m.name().to_string()).collect();
        }
        let mut vals = Vec::with_capacity(columns.len());
        for i in 0..columns.len() {
            vals.push(oracle_value_to_json(&row, i)?);
        }
        out_rows.push(vals);
    }
    Ok(QueryResult {
        columns,
        rows: out_rows,
    })
}

fn oracle_value_to_json(row: &oracledb::Row, idx: usize) -> Result<Value, AppError> {
    if let Ok(v) = row.get::<Option<bool>>(idx) {
        return Ok(match v {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.get::<Option<i64>>(idx) {
        return Ok(match v {
            Some(n) => json!(n),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.get::<Option<f64>>(idx) {
        return Ok(match v {
            Some(n) => json!(n),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.get::<Option<String>>(idx) {
        return Ok(match v {
            Some(s) => Value::String(s),
            None => Value::Null,
        });
    }
    if let Ok(v) = row.get::<Option<oracledb::OracleNumber>>(idx) {
        return Ok(match v {
            Some(n) => Value::String(n.to_string()),
            None => Value::Null,
        });
    }
    Err(AppError::Internal(anyhow::anyhow!(
        "unsupported oracle column type at index {idx}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_slash_at_form() {
        let d = parse_oracle_dsn("scott/tiger@dbhost:1521/ORCL").unwrap();
        assert_eq!(d.user, "scott");
        assert_eq!(d.password, "tiger");
        assert_eq!(d.connect_string, "dbhost:1521/ORCL");
    }

    #[test]
    fn parses_oracle_url() {
        let d = parse_oracle_dsn("oracle://scott:tiger@dbhost:1521/ORCL").unwrap();
        assert_eq!(d.user, "scott");
        assert_eq!(d.password, "tiger");
        assert_eq!(d.connect_string, "dbhost:1521/ORCL");
    }

    #[test]
    fn parses_double_slash_connect() {
        let d = parse_oracle_dsn("scott/tiger@//dbhost:1521/ORCL").unwrap();
        assert_eq!(d.connect_string, "//dbhost:1521/ORCL");
    }

    #[test]
    fn percent_decodes_password() {
        let d = parse_oracle_dsn("oracle://u:p%40ss@h:1521/s").unwrap();
        assert_eq!(d.password, "p@ss");
    }
}
