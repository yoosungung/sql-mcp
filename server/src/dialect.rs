//! Target SQL dialect emit (D21).
//!
//! Semantic compile emits **canonical** physical SQL: ANSI `"…"` identifiers and
//! Postgres-shaped helpers such as `date_trunc`. At the execution boundary,
//! `DialectProfile` rewrites date functions, adapts quotes, and wraps row limits.
//! Compile Steps 1–7 stay engine-neutral. Ad-hoc OLAP only — no execution bind rewrite.

use std::ops::ControlFlow;

use sqlparser::ast::{
    Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, Ident, VisitMut, VisitorMut,
    Value as AstValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

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

    /// Quote a simple identifier.
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

    /// Full emit: `date_trunc` rewrite → identifier quote adapt.
    pub fn emit_physical_sql(self, canonical: &str) -> Result<String, AppError> {
        let rewritten = self.rewrite_date_trunc(canonical)?;
        Ok(self.adapt_compiled_sql(&rewritten))
    }

    /// Rewrite canonical `date_trunc('unit', expr)` to the target dialect.
    /// Postgres: identity. Unsupported units → `validation`.
    pub fn rewrite_date_trunc(self, sql: &str) -> Result<String, AppError> {
        if self == Self::Postgres {
            return Ok(sql.to_string());
        }
        let dialect = GenericDialect {};
        let mut stmts = Parser::parse_sql(&dialect, sql)
            .map_err(|e| AppError::Validation(format!("dialect rewrite parse: {e}")))?;
        let mut visitor = DateTruncRewriter {
            profile: self,
            err: None,
        };
        for stmt in &mut stmts {
            let _ = stmt.visit(&mut visitor);
            if let Some(e) = visitor.err.take() {
                return Err(e);
            }
        }
        Ok(stmts
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join("; "))
    }

    /// Adapt canonical (ANSI `"…"`) compiled SQL to this dialect's identifier quotes.
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

struct DateTruncRewriter {
    profile: DialectProfile,
    err: Option<AppError>,
}

impl VisitorMut for DateTruncRewriter {
    type Break = ();

    fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        if self.err.is_some() {
            return ControlFlow::Break(());
        }
        let Expr::Function(func) = expr else {
            return ControlFlow::Continue(());
        };
        if !is_date_trunc(func) {
            return ControlFlow::Continue(());
        }
        match rewrite_date_trunc_fn(self.profile, func) {
            Ok(new_expr) => *expr = new_expr,
            Err(e) => {
                self.err = Some(e);
                return ControlFlow::Break(());
            }
        }
        ControlFlow::Continue(())
    }
}

fn is_date_trunc(func: &Function) -> bool {
    func.name.to_string().eq_ignore_ascii_case("date_trunc")
}

fn rewrite_date_trunc_fn(profile: DialectProfile, func: &Function) -> Result<Expr, AppError> {
    let (unit, arg) = date_trunc_args(func)?;
    let arg_sql = arg.to_string();
    let replacement = match (profile, unit.as_str()) {
        (DialectProfile::Mysql, "year") => {
            format!("DATE_FORMAT({arg_sql}, '%Y-01-01')")
        }
        (DialectProfile::Mysql, "month") => {
            format!("DATE_FORMAT({arg_sql}, '%Y-%m-01')")
        }
        (DialectProfile::Mysql, "day") => format!("DATE({arg_sql})"),
        (DialectProfile::Mysql, "hour") => {
            format!("DATE_FORMAT({arg_sql}, '%Y-%m-%d %H:00:00')")
        }
        (DialectProfile::Oracle, "year") => format!("TRUNC({arg_sql}, 'YYYY')"),
        (DialectProfile::Oracle, "month") => format!("TRUNC({arg_sql}, 'MM')"),
        (DialectProfile::Oracle, "day") => format!("TRUNC({arg_sql})"),
        (DialectProfile::Oracle, "hour") => format!("TRUNC({arg_sql}, 'HH24')"),
        (DialectProfile::Sqlite, "year") => {
            format!("datetime({arg_sql}, 'start of year')")
        }
        (DialectProfile::Sqlite, "month") => {
            format!("datetime({arg_sql}, 'start of month')")
        }
        (DialectProfile::Sqlite, "day") => {
            format!("datetime({arg_sql}, 'start of day')")
        }
        (DialectProfile::Sqlite, "hour") => {
            format!("datetime(strftime('%Y-%m-%d %H:00:00', {arg_sql}))")
        }
        (DialectProfile::Postgres, _) => {
            return Ok(Expr::Function(func.clone()));
        }
        (_, other) => {
            return Err(AppError::Validation(format!(
                "date_trunc unit '{other}' not supported for {}",
                profile.engine_name()
            )));
        }
    };
    Ok(parse_expr_fragment(&replacement))
}

fn date_trunc_args(func: &Function) -> Result<(String, Expr), AppError> {
    let FunctionArguments::List(list) = &func.args else {
        return Err(AppError::Validation(
            "date_trunc requires (unit, expr) arguments".into(),
        ));
    };
    if list.args.len() != 2 {
        return Err(AppError::Validation(
            "date_trunc requires exactly two arguments".into(),
        ));
    }
    let unit_expr = unnamed_arg_expr(&list.args[0])?;
    let value_expr = unnamed_arg_expr(&list.args[1])?;
    let unit = string_literal(&unit_expr).ok_or_else(|| {
        AppError::Validation("date_trunc unit must be a string literal".into())
    })?;
    Ok((unit.to_ascii_lowercase(), value_expr))
}

fn unnamed_arg_expr(arg: &FunctionArg) -> Result<Expr, AppError> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Ok(e.clone()),
        _ => Err(AppError::Validation(
            "date_trunc arguments must be unnamed exprs".into(),
        )),
    }
}

fn string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(v) => match &v.value {
            AstValue::SingleQuotedString(s) | AstValue::DoubleQuotedString(s) => Some(s.clone()),
            _ => None,
        },
        Expr::Nested(inner) => string_literal(inner),
        _ => None,
    }
}

fn parse_expr_fragment(sql: &str) -> Expr {
    let wrapped = format!("SELECT ({sql})");
    let dialect = GenericDialect {};
    if let Ok(stmts) = Parser::parse_sql(&dialect, &wrapped) {
        if let Some(sqlparser::ast::Statement::Query(q)) = stmts.into_iter().next() {
            if let sqlparser::ast::SetExpr::Select(select) = *q.body {
                if let Some(sqlparser::ast::SelectItem::UnnamedExpr(e)) =
                    select.projection.into_iter().next()
                {
                    return match e {
                        Expr::Nested(inner) => *inner,
                        other => other,
                    };
                }
            }
        }
    }
    Expr::Identifier(Ident::new(sql))
}

fn validate_simple_ident(name: &str) -> Result<(), AppError> {
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
        assert!(DialectProfile::for_engine("mssql").is_err());
    }

    #[test]
    fn quote_ident_golden() {
        assert_eq!(
            DialectProfile::Postgres.quote_ident("orders").unwrap(),
            "\"orders\""
        );
        assert_eq!(
            DialectProfile::Mysql.quote_ident("orders").unwrap(),
            "`orders`"
        );
        assert!(DialectProfile::Mysql.quote_ident("a;drop").is_err());
    }

    #[test]
    fn wrap_row_limit_golden() {
        let ora = DialectProfile::Oracle.wrap_row_limit("SELECT 1", 7);
        assert!(ora.contains("FETCH FIRST 7 ROWS ONLY"));
        assert!(!ora.contains("LIMIT"));
    }

    #[test]
    fn adapt_compiled_sql_mysql_quotes_not_strings() {
        let canonical =
            r#"SELECT "amt" AS "revenue" FROM "sales"."orders" WHERE "status" = 'PAID'"#;
        assert_eq!(
            DialectProfile::Mysql.adapt_compiled_sql(canonical),
            "SELECT `amt` AS `revenue` FROM `sales`.`orders` WHERE `status` = 'PAID'"
        );
    }

    #[test]
    fn rewrite_date_trunc_postgres_identity() {
        let sql = r#"SELECT date_trunc('year', "order_date") FROM "t""#;
        let out = DialectProfile::Postgres.rewrite_date_trunc(sql).unwrap();
        assert_eq!(out, sql);
    }

    #[test]
    fn rewrite_date_trunc_mysql_year_month_day() {
        let sql = r#"SELECT date_trunc('year', "order_date") AS y FROM "t""#;
        let out = DialectProfile::Mysql.rewrite_date_trunc(sql).unwrap();
        let lower = out.to_ascii_lowercase();
        assert!(lower.contains("date_format"), "{out}");
        assert!(out.contains("%Y-01-01"), "{out}");
        assert!(!lower.contains("date_trunc"), "{out}");

        let month = DialectProfile::Mysql
            .rewrite_date_trunc(r#"SELECT date_trunc('month', "d") FROM "t""#)
            .unwrap();
        assert!(month.contains("%Y-%m-01"), "{month}");

        let day = DialectProfile::Mysql
            .rewrite_date_trunc(r#"SELECT date_trunc('day', "d") FROM "t""#)
            .unwrap();
        assert!(day.to_ascii_lowercase().contains("date("), "{day}");
    }

    #[test]
    fn rewrite_date_trunc_oracle_and_sqlite() {
        let ora = DialectProfile::Oracle
            .rewrite_date_trunc(r#"SELECT date_trunc('month', "d") FROM "t""#)
            .unwrap();
        assert!(ora.to_ascii_uppercase().contains("TRUNC"), "{ora}");
        assert!(ora.contains("'MM'") || ora.contains("\"MM\""), "{ora}");

        let lite = DialectProfile::Sqlite
            .rewrite_date_trunc(r#"SELECT date_trunc('year', "d") FROM "t""#)
            .unwrap();
        assert!(lite.to_ascii_lowercase().contains("datetime"), "{lite}");
        assert!(lite.contains("start of year"), "{lite}");
    }

    #[test]
    fn rewrite_date_trunc_rejects_unknown_unit() {
        let err = DialectProfile::Mysql
            .rewrite_date_trunc(r#"SELECT date_trunc('millennium', "d") FROM "t""#)
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn emit_physical_sql_mysql_rewrites_and_quotes() {
        let canonical = r#"SELECT date_trunc('year', "order_date") AS "y" FROM "sales"."orders""#;
        let out = DialectProfile::Mysql.emit_physical_sql(canonical).unwrap();
        assert!(out.contains('`'), "{out}");
        assert!(!out.contains('"'), "{out}");
        assert!(out.to_ascii_lowercase().contains("date_format"), "{out}");
        assert!(!out.to_ascii_lowercase().contains("date_trunc"), "{out}");
        crate::select_guard::assert_select_only(&out).unwrap();
    }
}
