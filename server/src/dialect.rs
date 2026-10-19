//! Target SQL dialect emit (D21).
//!
//! Semantic compile emits **canonical** physical SQL: ANSI `"…"` identifiers and
//! Postgres-shaped helpers such as `date_trunc`. At the execution boundary,
//! `DialectProfile` rewrites date functions, adapts quotes, and wraps row limits.
//! Compile Steps 1–7 stay engine-neutral. Ad-hoc OLAP only — no execution bind rewrite.

use std::ops::ControlFlow;

use sqlparser::ast::{
    BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments, Ident,
    VisitMut, VisitorMut, Value as AstValue,
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

    /// Rewrite canonical `date_trunc('unit', expr)`, safe `/`, and dialect helpers.
    /// Postgres skips the walk only when there is no `DIVIDE(` and no `/` (no-op path).
    /// Unsupported `date_trunc` units → `validation`.
    pub fn rewrite_date_trunc(self, sql: &str) -> Result<String, AppError> {
        let lower = sql.to_ascii_lowercase();
        if self == Self::Postgres && !lower.contains("divide(") && !lower.contains('/') {
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

    fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        if self.err.is_some() {
            return ControlFlow::Break(());
        }
        if self.profile != DialectProfile::Sqlite {
            return ControlFlow::Continue(());
        }
        match expr {
            Expr::BinaryOp { left, op, right }
                if matches!(
                    op,
                    BinaryOperator::Eq
                        | BinaryOperator::NotEq
                        | BinaryOperator::Lt
                        | BinaryOperator::LtEq
                        | BinaryOperator::Gt
                        | BinaryOperator::GtEq
                ) =>
            {
                if let Expr::Function(func) = left.as_ref() {
                    if is_date_trunc(func) {
                        match rewrite_sqlite_date_trunc_for_literal(func, right) {
                            Ok(Some(new_left)) => **left = new_left,
                            Ok(None) => {}
                            Err(e) => {
                                self.err = Some(e);
                                return ControlFlow::Break(());
                            }
                        }
                    }
                }
                if let Expr::Function(func) = right.as_ref() {
                    if is_date_trunc(func) {
                        match rewrite_sqlite_date_trunc_for_literal(func, left) {
                            Ok(Some(new_right)) => **right = new_right,
                            Ok(None) => {}
                            Err(e) => {
                                self.err = Some(e);
                                return ControlFlow::Break(());
                            }
                        }
                    }
                }
            }
            Expr::Between {
                expr: target, low, ..
            } => {
                if let Expr::Function(func) = target.as_ref() {
                    if is_date_trunc(func) {
                        match rewrite_sqlite_date_trunc_for_literal(func, low) {
                            Ok(Some(new_target)) => **target = new_target,
                            Ok(None) => {}
                            Err(e) => {
                                self.err = Some(e);
                                return ControlFlow::Break(());
                            }
                        }
                    }
                }
            }
            Expr::InList {
                expr: target, list, ..
            } => {
                if let (Expr::Function(func), Some(first)) = (target.as_ref(), list.first()) {
                    if is_date_trunc(func) {
                        match rewrite_sqlite_date_trunc_for_literal(func, first) {
                            Ok(Some(new_target)) => **target = new_target,
                            Ok(None) => {}
                            Err(e) => {
                                self.err = Some(e);
                                return ControlFlow::Break(());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        if self.err.is_some() {
            return ControlFlow::Break(());
        }
        match expr {
            Expr::Function(func) => {
                let fname = func.name.to_string().to_ascii_lowercase();
                if fname == "date_trunc" {
                    match rewrite_date_trunc_fn(self.profile, func) {
                        Ok(new_expr) => *expr = new_expr,
                        Err(e) => {
                            self.err = Some(e);
                            return ControlFlow::Break(());
                        }
                    }
                } else if fname == "divide" {
                    match rewrite_divide_fn(self.profile, func) {
                        Ok(new_expr) => *expr = new_expr,
                        Err(e) => {
                            self.err = Some(e);
                            return ControlFlow::Break(());
                        }
                    }
                } else if self.profile == DialectProfile::Sqlite
                    && matches!(fname.as_str(), "year" | "month" | "day")
                {
                    match rewrite_sqlite_ymd_fn(&fname, func) {
                        Ok(new_expr) => *expr = new_expr,
                        Err(e) => {
                            self.err = Some(e);
                            return ControlFlow::Break(());
                        }
                    }
                }
            }
            Expr::BinaryOp {
                left,
                op: BinaryOperator::Divide,
                right,
            } => {
                *expr = rewrite_safe_divide(self.profile, left, right);
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

fn is_already_real_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Cast { data_type, .. } => {
            let dt = data_type.to_string().to_ascii_uppercase();
            dt.contains("REAL") || dt.contains("FLOAT") || dt.contains("DOUBLE")
        }
        Expr::Value(v) => match &v.value {
            AstValue::Number(n, _) => n.contains('.'),
            _ => false,
        },
        Expr::BinaryOp {
            op: BinaryOperator::Multiply,
            left,
            right,
        } => {
            // `(x * 1.0)` / `(1.0 * x)` already promotes to float.
            is_float_one(left) || is_float_one(right)
        }
        Expr::Nested(inner) => is_already_real_expr(inner),
        _ => false,
    }
}

fn is_float_one(expr: &Expr) -> bool {
    match expr {
        Expr::Value(v) => match &v.value {
            AstValue::Number(n, _) => n == "1.0" || n == "1.00",
            _ => false,
        },
        Expr::Nested(inner) => is_float_one(inner),
        _ => false,
    }
}

/// True when `expr` is `NULLIF(…, 0)` (optionally nested), so we do not double-wrap.
fn is_nullif_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Nested(inner) => is_nullif_zero(inner),
        Expr::Function(func) if func.name.to_string().eq_ignore_ascii_case("nullif") => {
            let FunctionArguments::List(list) = &func.args else {
                return false;
            };
            if list.args.len() != 2 {
                return false;
            }
            match unnamed_arg_expr(&list.args[1]) {
                Ok(arg) => matches!(
                    arg,
                    Expr::Value(v) if matches!(&v.value, AstValue::Number(n, _) if n == "0")
                ),
                Err(_) => false,
            }
        }
        _ => false,
    }
}

/// Float-promote numerator and wrap denominator with `NULLIF(…, 0)` (idempotent).
fn rewrite_safe_divide(profile: DialectProfile, num: &Expr, den: &Expr) -> Expr {
    let num_sql = num.to_string();
    let den_sql = if is_nullif_zero(den) {
        den.to_string()
    } else {
        format!("NULLIF({}, 0)", den)
    };
    let replacement = match profile {
        DialectProfile::Sqlite => {
            if is_already_real_expr(num) {
                format!("{num_sql} / {den_sql}")
            } else {
                format!("CAST({num_sql} AS REAL) / {den_sql}")
            }
        }
        _ => {
            if is_already_real_expr(num) {
                format!("{num_sql} / {den_sql}")
            } else {
                format!("({num_sql} * 1.0) / {den_sql}")
            }
        }
    };
    parse_expr_fragment(&replacement)
}

fn is_date_trunc(func: &Function) -> bool {
    func.name.to_string().eq_ignore_ascii_case("date_trunc")
}

fn rewrite_divide_fn(profile: DialectProfile, func: &Function) -> Result<Expr, AppError> {
    let FunctionArguments::List(list) = &func.args else {
        return Err(AppError::Validation(
            "DIVIDE requires (numerator, denominator) arguments".into(),
        ));
    };
    if list.args.len() != 2 {
        return Err(AppError::Validation(
            "DIVIDE requires exactly two arguments".into(),
        ));
    }
    let num = unnamed_arg_expr(&list.args[0])?;
    let den = unnamed_arg_expr(&list.args[1])?;
    Ok(rewrite_safe_divide(profile, &num, &den))
}

fn rewrite_sqlite_ymd_fn(fname: &str, func: &Function) -> Result<Expr, AppError> {
    let FunctionArguments::List(list) = &func.args else {
        return Err(AppError::Validation(format!(
            "{fname} requires 1 argument"
        )));
    };
    if list.args.len() != 1 {
        return Err(AppError::Validation(format!(
            "{fname} requires exactly 1 argument"
        )));
    }
    let arg_sql = unnamed_arg_expr(&list.args[0])?.to_string();
    let replacement = match fname {
        "year" => format!("CAST(SUBSTR(CAST({arg_sql} AS TEXT), 1, 4) AS INTEGER)"),
        "month" => format!(
            "CAST(COALESCE(strftime('%m', {arg_sql}), SUBSTR(REPLACE(CAST({arg_sql} AS TEXT), '-', ''), 5, 2)) AS INTEGER)"
        ),
        "day" => format!(
            "CAST(COALESCE(strftime('%d', {arg_sql}), SUBSTR(REPLACE(CAST({arg_sql} AS TEXT), '-', ''), 7, 2)) AS INTEGER)"
        ),
        _ => unreachable!(),
    };
    Ok(parse_expr_fragment(&replacement))
}

fn rewrite_sqlite_date_trunc_for_literal(
    func: &Function,
    lit_expr: &Expr,
) -> Result<Option<Expr>, AppError> {
    let (unit, arg) = date_trunc_args(func)?;
    let arg_sql = arg.to_string();
    if let Some(s) = string_literal(lit_expr) {
        let trimmed = s.trim();
        let replacement = match unit.as_str() {
            "year" => {
                if trimmed.len() == 4 && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    Some(format!("SUBSTR(CAST({arg_sql} AS TEXT), 1, 4)"))
                } else if trimmed.len() == 10
                    && trimmed.as_bytes()[4] == b'-'
                    && trimmed.as_bytes()[7] == b'-'
                {
                    Some(format!("date({arg_sql}, 'start of year')"))
                } else {
                    None
                }
            }
            "month" => {
                if trimmed.len() == 7 && trimmed.as_bytes()[4] == b'-' {
                    Some(format!(
                        "COALESCE(strftime('%Y-%m', {arg_sql}), SUBSTR(CAST({arg_sql} AS TEXT), 1, 4) || '-' || SUBSTR(CAST({arg_sql} AS TEXT), 5, 2))"
                    ))
                } else if trimmed.len() == 6 && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    Some(format!(
                        "SUBSTR(REPLACE(CAST({arg_sql} AS TEXT), '-', ''), 1, 6)"
                    ))
                } else if trimmed.len() == 10
                    && trimmed.as_bytes()[4] == b'-'
                    && trimmed.as_bytes()[7] == b'-'
                {
                    Some(format!("date({arg_sql}, 'start of month')"))
                } else {
                    None
                }
            }
            "day" => {
                if trimmed.len() == 10
                    && trimmed.as_bytes()[4] == b'-'
                    && trimmed.as_bytes()[7] == b'-'
                {
                    Some(format!("date({arg_sql}, 'start of day')"))
                } else if trimmed.len() == 8 && trimmed.chars().all(|c| c.is_ascii_digit()) {
                    Some(format!(
                        "SUBSTR(REPLACE(CAST({arg_sql} AS TEXT), '-', ''), 1, 8)"
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        return Ok(replacement.map(|r| parse_expr_fragment(&r)));
    }
    if let Some(n) = int_literal(lit_expr) {
        let replacement = match unit.as_str() {
            "year" if n.len() == 4 => Some(format!(
                "CAST(SUBSTR(CAST({arg_sql} AS TEXT), 1, 4) AS INTEGER)"
            )),
            "month" if n.len() == 6 => Some(format!(
                "CAST(SUBSTR(REPLACE(CAST({arg_sql} AS TEXT), '-', ''), 1, 6) AS INTEGER)"
            )),
            "day" if n.len() == 8 => Some(format!(
                "CAST(SUBSTR(REPLACE(CAST({arg_sql} AS TEXT), '-', ''), 1, 8) AS INTEGER)"
            )),
            _ => None,
        };
        return Ok(replacement.map(|r| parse_expr_fragment(&r)));
    }
    Ok(None)
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
    };
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
            "function arguments must be unnamed exprs".into(),
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

fn int_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Value(v) => match &v.value {
            AstValue::Number(n, _) if n.chars().all(|c| c.is_ascii_digit()) => Some(n.clone()),
            _ => None,
        },
        Expr::Nested(inner) => int_literal(inner),
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

/// Physical / quoted SQL identifier (D22). Allows spaces and symbols used in
/// real DW catalogs (e.g. BIRD); rejects quote delimiters, controls, and `;`.
fn validate_simple_ident(name: &str) -> Result<(), AppError> {
    if name.is_empty()
        || name.len() > 512
        || name.contains('"')
        || name.contains('`')
        || name.contains(';')
        || name.chars().any(|c| c.is_control())
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
    fn quote_ident_allows_bird_physical_names() {
        assert_eq!(
            DialectProfile::Postgres
                .quote_ident("Percent (%) Eligible Free (K-12)")
                .unwrap(),
            "\"Percent (%) Eligible Free (K-12)\""
        );
        assert_eq!(
            DialectProfile::Sqlite
                .quote_ident("Academic Year")
                .unwrap(),
            "\"Academic Year\""
        );
        assert!(DialectProfile::Postgres.quote_ident("we\"ird").is_err());
        assert!(DialectProfile::Postgres.quote_ident("a\nb").is_err());
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

    #[test]
    fn sqlite_date_trunc_comparisons_adapt_to_literal_format() {
        let y_str = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT "a" FROM "t" WHERE date_trunc('year', "d") = '1980'"#)
            .unwrap();
        assert!(
            y_str.contains(r#"SUBSTR(CAST("d" AS TEXT), 1, 4) = '1980'"#),
            "expected 4-char year string comparison, got: {y_str}"
        );

        let y_int = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT "a" FROM "t" WHERE date_trunc('year', "d") = 1980"#)
            .unwrap();
        assert!(
            y_int.contains(r#"CAST(SUBSTR(CAST("d" AS TEXT), 1, 4) AS INTEGER) = 1980"#),
            "expected integer year comparison, got: {y_int}"
        );

        let y_between = DialectProfile::Sqlite
            .emit_physical_sql(
                r#"SELECT "a" FROM "t" WHERE date_trunc('year', "d") BETWEEN 1980 AND 1985"#,
            )
            .unwrap();
        assert!(
            y_between.contains(
                r#"CAST(SUBSTR(CAST("d" AS TEXT), 1, 4) AS INTEGER) BETWEEN 1980 AND 1985"#
            ),
            "expected integer year BETWEEN, got: {y_between}"
        );

        let m_ym = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT "a" FROM "t" WHERE date_trunc('month', "d") = '2012-08'"#)
            .unwrap();
        assert!(
            m_ym.contains("strftime('%Y-%m'") && m_ym.contains("= '2012-08'"),
            "expected YYYY-MM comparison, got: {m_ym}"
        );

        let m_yyyymm = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT "a" FROM "t" WHERE date_trunc('month', "d") = '201208'"#)
            .unwrap();
        assert!(
            m_yyyymm.contains(r#"SUBSTR(REPLACE(CAST("d" AS TEXT), '-', ''), 1, 6) = '201208'"#),
            "expected YYYYMM comparison, got: {m_yyyymm}"
        );

        let d_iso = DialectProfile::Sqlite
            .emit_physical_sql(
                r#"SELECT "a" FROM "t" WHERE date_trunc('day', "d") = '2012-08-25'"#,
            )
            .unwrap();
        assert!(
            d_iso.contains(r#"date("d", 'start of day') = '2012-08-25'"#),
            "expected date() comparison without 00:00:00, got: {d_iso}"
        );
    }

    #[test]
    fn sqlite_promotes_division_to_real_and_rewrites_helpers() {
        let div_sql = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT COUNT("a") * 100 / COUNT("b") AS "pct" FROM "t""#)
            .unwrap();
        assert!(
            div_sql.contains("AS REAL)"),
            "expected REAL cast on SQLite division numerator, got: {div_sql}"
        );
        assert!(
            div_sql.contains("NULLIF(COUNT(\"b\"), 0)"),
            "expected NULLIF on raw `/` denominator, got: {div_sql}"
        );

        let helper_sql = DialectProfile::Sqlite
            .emit_physical_sql(
                r#"SELECT DIVIDE(SUM("a"), COUNT("b")) AS "r", YEAR("d") AS "y" FROM "t""#,
            )
            .unwrap();
        assert!(
            helper_sql.contains("NULLIF(COUNT(\"b\"), 0)"),
            "expected NULLIF in DIVIDE rewrite, got: {helper_sql}"
        );
        assert!(
            helper_sql.contains(r#"CAST(SUBSTR(CAST("d" AS TEXT), 1, 4) AS INTEGER)"#),
            "expected YEAR(d) rewrite for SQLite, got: {helper_sql}"
        );

        let raw = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT SUM("a") / COUNT("b") AS "r" FROM "t""#)
            .unwrap();
        let via_divide = DialectProfile::Sqlite
            .emit_physical_sql(r#"SELECT DIVIDE(SUM("a"), COUNT("b")) AS "r" FROM "t""#)
            .unwrap();
        assert_eq!(raw, via_divide, "raw `/` and DIVIDE must emit identically");
    }

    #[test]
    fn postgres_raw_division_gets_nullif_and_float_promote() {
        let sql = DialectProfile::Postgres
            .emit_physical_sql(r#"SELECT COUNT("a") / COUNT("b") AS "r" FROM "t""#)
            .unwrap();
        assert!(
            sql.contains("* 1.0") && sql.contains("NULLIF(COUNT(\"b\"), 0)"),
            "expected float promote + NULLIF on Postgres `/`, got: {sql}"
        );
    }

    #[test]
    fn safe_divide_is_idempotent_when_nullif_already_present() {
        let sql = DialectProfile::Postgres
            .emit_physical_sql(r#"SELECT "a" / NULLIF("b", 0) AS "r" FROM "t""#)
            .unwrap();
        assert!(
            !sql.to_ascii_uppercase().contains("NULLIF(NULLIF("),
            "must not double-wrap NULLIF, got: {sql}"
        );
        assert!(
            sql.contains("NULLIF(\"b\", 0)"),
            "expected original NULLIF preserved, got: {sql}"
        );
        assert!(
            sql.contains("* 1.0"),
            "expected float promote on numerator, got: {sql}"
        );
    }
}
