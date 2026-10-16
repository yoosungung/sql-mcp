//! Target SQL dialect emit (D21).
//!
//! Semantic compile emits a **canonical** physical SQL using ANSI double-quoted
//! identifiers (`compile::quote_ident`). At the execution boundary, a
//! `DialectProfile` adapts quotes (e.g. MySQL backticks) and wraps row limits.
//! Compile Steps 1–7 stay engine-neutral.

use crate::error::AppError;

/// Engine-specific SQL emit rules. Selected from `TargetConfig.engine`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialectProfile {
    Postgres,
    Mysql,
    Sqlite,
    Oracle,
}

impl DialectProfile {
    /// Map config `engine` string → profile.
    pub fn for_engine(engine: &str) -> Result<Self, AppError> {
        match engine {
            "postgres" => Ok(Self::Postgres),
            "mysql" => Ok(Self::Mysql),
            "sqlite" => Ok(Self::Sqlite),
            "oracle" => Ok(Self::Oracle),
            other => Err(AppError::Validation(format!(
                "unsupported target engine for dialect: {other}"
            ))),
        }
    }

    pub fn engine_name(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
            Self::Oracle => "oracle",
        }
    }

    /// Quote a simple identifier (alphanumeric + `_` only).
    pub fn quote_ident(self, name: &str) -> Result<String, AppError> {
        validate_simple_ident(name)?;
        Ok(match self {
            Self::Mysql => format!("`{name}`"),
            Self::Postgres | Self::Sqlite | Self::Oracle => format!("\"{name}\""),
        })
    }

    /// `schema.table` with dialect quoting.
    pub fn qualify_table(self, schema: &str, table: &str) -> Result<String, AppError> {
        Ok(format!(
            "{}.{}",
            self.quote_ident(schema)?,
            self.quote_ident(table)?
        ))
    }

    /// Adapt canonical (ANSI `"…"`) compiled SQL to this dialect's identifier quotes.
    /// Leaves `'…'` string literals untouched. Postgres/SQLite/Oracle: identity.
    pub fn adapt_compiled_sql(self, sql: &str) -> String {
        match self {
            Self::Mysql => ansi_double_quotes_to_mysql_backticks(sql),
            Self::Postgres | Self::Sqlite | Self::Oracle => sql.to_string(),
        }
    }

    /// Wrap a SELECT so the engine returns at most `max_rows`.
    pub fn wrap_row_limit(self, sql: &str, max_rows: usize) -> String {
        match self {
            Self::Oracle => format!(
                "SELECT * FROM ({sql}) _sql_mcp_q FETCH FIRST {max_rows} ROWS ONLY"
            ),
            Self::Postgres | Self::Mysql | Self::Sqlite => {
                format!("SELECT * FROM ({sql}) AS _sql_mcp_q LIMIT {max_rows}")
            }
        }
    }
}

fn validate_simple_ident(name: &str) -> Result<(), AppError> {
    // Quoted idents may include space/hyphen (physical SQLite columns); reject quote chars / `;`.
    if name.is_empty()
        || name.contains('"')
        || name.contains('`')
        || name.contains(';')
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '-'))
    {
        return Err(AppError::Validation(format!("invalid identifier: {name}")));
    }
    Ok(())
}

/// Convert ANSI `"ident"` to MySQL `` `ident` `` without touching `'…'` strings.
fn ansi_double_quotes_to_mysql_backticks(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                out.push(ch);
                i += 1;
                if ch == '\'' {
                    // SQL escape: '' inside string
                    if i < chars.len() && chars[i] == '\'' {
                        out.push('\'');
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            continue;
        }
        if c == '"' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            out.push('`');
            out.push_str(&ident.replace('`', "``"));
            out.push('`');
            if i < chars.len() && chars[i] == '"' {
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_engine_maps_known() {
        assert_eq!(
            DialectProfile::for_engine("postgres").unwrap(),
            DialectProfile::Postgres
        );
        assert_eq!(
            DialectProfile::for_engine("mysql").unwrap(),
            DialectProfile::Mysql
        );
        assert_eq!(
            DialectProfile::for_engine("sqlite").unwrap(),
            DialectProfile::Sqlite
        );
        assert_eq!(
            DialectProfile::for_engine("oracle").unwrap(),
            DialectProfile::Oracle
        );
        assert!(DialectProfile::for_engine("mssql").is_err());
    }

    #[test]
    fn quote_ident_golden() {
        assert_eq!(
            DialectProfile::Postgres.quote_ident("orders").unwrap(),
            "\"orders\""
        );
        assert_eq!(
            DialectProfile::Sqlite.quote_ident("orders").unwrap(),
            "\"orders\""
        );
        assert_eq!(
            DialectProfile::Oracle.quote_ident("ORDERS").unwrap(),
            "\"ORDERS\""
        );
        assert_eq!(
            DialectProfile::Mysql.quote_ident("orders").unwrap(),
            "`orders`"
        );
        assert!(DialectProfile::Mysql.quote_ident("a;drop").is_err());
        assert!(DialectProfile::Postgres.quote_ident("").is_err());
    }

    #[test]
    fn qualify_table_golden() {
        assert_eq!(
            DialectProfile::Postgres
                .qualify_table("sales", "orders")
                .unwrap(),
            "\"sales\".\"orders\""
        );
        assert_eq!(
            DialectProfile::Mysql
                .qualify_table("sales", "orders")
                .unwrap(),
            "`sales`.`orders`"
        );
    }

    #[test]
    fn wrap_row_limit_golden() {
        let inner = "SELECT 1 AS n";
        let pg = DialectProfile::Postgres.wrap_row_limit(inner, 10);
        assert!(pg.contains("LIMIT 10"));
        assert!(pg.contains("AS _sql_mcp_q"));
        assert!(!pg.contains("FETCH FIRST"));

        let my = DialectProfile::Mysql.wrap_row_limit(inner, 5);
        assert!(my.contains("LIMIT 5"));

        let lite = DialectProfile::Sqlite.wrap_row_limit(inner, 3);
        assert!(lite.contains("LIMIT 3"));

        let ora = DialectProfile::Oracle.wrap_row_limit(inner, 7);
        assert!(ora.contains("FETCH FIRST 7 ROWS ONLY"));
        assert!(!ora.contains("LIMIT"));
        assert!(!ora.contains(" AS _sql_mcp_q"));
    }

    #[test]
    fn adapt_compiled_sql_mysql_quotes_not_strings() {
        let canonical =
            r#"SELECT "amt" AS "revenue" FROM "sales"."orders" WHERE "status" = 'PAID'"#;
        let mysql = DialectProfile::Mysql.adapt_compiled_sql(canonical);
        assert_eq!(
            mysql,
            "SELECT `amt` AS `revenue` FROM `sales`.`orders` WHERE `status` = 'PAID'"
        );
        // string with doubles must stay
        let with_str = r#"SELECT "c" FROM "t" WHERE "x" = 'say "hi"'"#;
        let adapted = DialectProfile::Mysql.adapt_compiled_sql(with_str);
        assert_eq!(adapted, "SELECT `c` FROM `t` WHERE `x` = 'say \"hi\"'");

        assert_eq!(
            DialectProfile::Postgres.adapt_compiled_sql(canonical),
            canonical
        );
    }

    #[test]
    fn end_to_end_emit_shape() {
        let canonical = r#"SELECT "a" FROM "s"."t""#;
        let d = DialectProfile::Mysql;
        let adapted = d.adapt_compiled_sql(canonical);
        let limited = d.wrap_row_limit(&adapted, 100);
        assert!(limited.starts_with("SELECT * FROM (SELECT `a` FROM `s`.`t`)"));
        assert!(limited.contains("LIMIT 100"));
    }

    #[test]
    fn adapted_mysql_sql_passes_select_guard() {
        let canonical = r#"SELECT "amt" AS "revenue" FROM "sales"."orders" WHERE "status" = 'PAID'"#;
        let adapted = DialectProfile::Mysql.adapt_compiled_sql(canonical);
        crate::select_guard::assert_select_only(&adapted).unwrap();
    }
}
