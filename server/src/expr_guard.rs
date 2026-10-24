//! D16/D18: trusted SQL fragment AST limits + identifier checks at compile time.

use std::collections::HashSet;

use sqlparser::ast::{
    BinaryOperator, DateTimeField, Expr, FunctionArg, FunctionArgExpr, FunctionArguments, SetExpr,
    Statement, UnaryOperator, WindowType,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::{forbidden_function_message, unknown_member_message, AppError};

const METRIC_FUNCS: &[&str] = &[
    "sum",
    "count",
    "avg",
    "min",
    "max",
    "coalesce",
    "nullif",
    "greatest",
    "least",
    "abs",
    "round",
    "date_trunc",
    "lower",
    "upper",
    "trim",
    "substr",
    "substring",
    "length",
    "replace",
    "instr",
    "ifnull",
    "iif",
    "date",
    "datetime",
    "strftime",
    "year",
    "month",
    "day",
    "divide",
    "current_date",
    "current_timestamp",
    "now",
    "datediff",
];

const WINDOW_ONLY_FUNCS: &[&str] = &[
    "row_number",
    "rank",
    "dense_rank",
    "ntile",
    "percent_rank",
    "cume_dist",
    "lag",
    "lead",
    "first_value",
    "last_value",
    "nth_value",
];

const FILTER_FUNCS: &[&str] = &[
    "coalesce",
    "nullif",
    "lower",
    "upper",
    "trim",
    "abs",
    "round",
    "date_trunc",
    "substr",
    "substring",
    "length",
    "replace",
    "instr",
    "ifnull",
    "iif",
    "date",
    "datetime",
    "strftime",
    "year",
    "month",
    "day",
    "divide",
    "current_date",
    "current_timestamp",
    "now",
    "datediff",
];

const EXTRACT_FIELDS: &[DateTimeField] = &[
    DateTimeField::Year,
    DateTimeField::Month,
    DateTimeField::Day,
    DateTimeField::Hour,
];

/// Ad-hoc metric/aggregate expression: scalar/aggregate/window + read-only subqueries (D16/D26).
pub fn assert_metric_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_select_expr(&normalized)?;
    walk_value_expr_opts(&e, true, true)?;
    Ok(())
}

/// Registered metadata `metric.expression`: allows read-only SELECT subqueries in addition to aggregates/windows.
pub fn assert_metadata_metric_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_select_expr(&normalized)?;
    walk_value_expr_opts(&e, true, true)?;
    Ok(())
}

/// D23/D26 scalar/window/subquery expression over dimensions/facts (no group-collapsing aggregates).
pub fn assert_scalar_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_select_expr(&normalized)?;
    walk_value_expr_opts(&e, false, true)?;
    Ok(())
}

/// Return true if `expr` contains a group-collapsing aggregate function (`SUM/COUNT/AVG/MIN/MAX` without `OVER`).
pub fn contains_aggregate_sql(expr: &str) -> Result<bool, AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_select_expr(&normalized)?;
    Ok(contains_aggregate(&e))
}

/// Return true if `expr` contains a window function (`... OVER (...)`).
pub fn contains_window_sql(expr: &str) -> Result<bool, AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_select_expr(&normalized)?;
    Ok(contains_window(&e))
}

/// D23 ad-hoc projection: same AST as metric expression, but must include a group aggregate.
pub fn assert_adhoc_agg_expr(expr: &str) -> Result<(), AppError> {
    assert_metric_expr(expr)?;
    if !contains_aggregate_sql(expr)? {
        return Err(AppError::Validation(
            "ad-hoc projection must be an aggregate expression".into(),
        ));
    }
    Ok(())
}

fn contains_aggregate(expr: &Expr) -> bool {
    match expr {
        Expr::Function(func) => {
            let lower = func.name.to_string().to_ascii_lowercase();
            if func.over.is_none()
                && matches!(lower.as_str(), "sum" | "count" | "avg" | "min" | "max")
            {
                return true;
            }
            let args_have_agg = match &func.args {
                FunctionArguments::List(list) => list.args.iter().any(|a| match a {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(e),
                        ..
                    } => contains_aggregate(e),
                    _ => false,
                }),
                _ => false,
            };
            let over_has_agg = match &func.over {
                Some(WindowType::WindowSpec(spec)) => {
                    spec.partition_by.iter().any(contains_aggregate)
                        || spec.order_by.iter().any(|ob| contains_aggregate(&ob.expr))
                }
                _ => false,
            };
            args_have_agg || over_has_agg
        }
        Expr::UnaryOp { expr, .. } | Expr::Cast { expr, .. } | Expr::Nested(expr) => {
            contains_aggregate(expr)
        }
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            contains_aggregate(expr)
                || substring_from
                    .as_ref()
                    .is_some_and(|e| contains_aggregate(e))
                || substring_for
                    .as_ref()
                    .is_some_and(|e| contains_aggregate(e))
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            contains_aggregate(expr)
                || trim_what.as_ref().is_some_and(|e| contains_aggregate(e))
                || trim_characters
                    .as_ref()
                    .is_some_and(|v| v.iter().any(contains_aggregate))
        }
        Expr::BinaryOp { left, right, .. } => contains_aggregate(left) || contains_aggregate(right),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|o| contains_aggregate(o))
                || conditions
                    .iter()
                    .any(|cw| contains_aggregate(&cw.condition) || contains_aggregate(&cw.result))
                || else_result.as_ref().is_some_and(|e| contains_aggregate(e))
        }
        _ => false,
    }
}

fn contains_window(expr: &Expr) -> bool {
    match expr {
        Expr::Function(func) => {
            if func.over.is_some() {
                return true;
            }
            match &func.args {
                FunctionArguments::List(list) => list.args.iter().any(|a| match a {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(e),
                        ..
                    } => contains_window(e),
                    _ => false,
                }),
                _ => false,
            }
        }
        Expr::UnaryOp { expr, .. } | Expr::Cast { expr, .. } | Expr::Nested(expr) => {
            contains_window(expr)
        }
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            contains_window(expr)
                || substring_from.as_ref().is_some_and(|e| contains_window(e))
                || substring_for.as_ref().is_some_and(|e| contains_window(e))
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            contains_window(expr)
                || trim_what.as_ref().is_some_and(|e| contains_window(e))
                || trim_characters
                    .as_ref()
                    .is_some_and(|v| v.iter().any(contains_window))
        }
        Expr::BinaryOp { left, right, .. } => contains_window(left) || contains_window(right),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            operand.as_ref().is_some_and(|o| contains_window(o))
                || conditions
                    .iter()
                    .any(|cw| contains_window(&cw.condition) || contains_window(&cw.result))
                || else_result.as_ref().is_some_and(|e| contains_window(e))
        }
        _ => false,
    }
}

/// Ad-hoc WHERE / table filter predicate: single boolean expr (D16, no aggregates, no subqueries).
pub fn assert_filter_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_where_expr(&normalized)?;
    walk_bool_expr_full(&e, false, false)?;
    Ok(())
}

/// Registered metadata `segment.filter.expression` / `metric.filter.expression`: allows read-only SELECT subqueries.
pub fn assert_segment_filter_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_where_expr(&normalized)?;
    walk_bool_expr_full(&e, false, true)?;
    Ok(())
}

/// HAVING predicate: boolean with aggregates and read-only subqueries (D23/D26).
pub fn assert_having_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_where_expr(&normalized)?;
    walk_bool_expr_full(&e, true, true)?;
    Ok(())
}

/// Identifiers in `expr` must be subset of `known` (D18). `:bind` placeholders ignored.
pub fn assert_idents_known(expr: &str, known: &HashSet<String>) -> Result<(), AppError> {
    let found = extract_idents(expr)?;
    for id in &found {
        if !known.contains(id) {
            return Err(AppError::Validation(unknown_member_message(&format!(
                "unknown identifier in expression: {id}"
            ))));
        }
    }
    Ok(())
}

/// Extract all bare/compound column identifiers from `expr` via AST (ignoring `:bind` placeholders).
pub fn extract_idents(expr: &str) -> Result<HashSet<String>, AppError> {
    let (normalized, _binds) = replace_binds(expr);
    let e = parse_expr_loose(&normalized)?;
    let mut found = HashSet::new();
    collect_idents(&e, &mut found);
    Ok(found)
}

/// Replace `:name` bind params with NULL so sqlparser accepts them.
fn replace_binds(expr: &str) -> (String, HashSet<String>) {
    let mut out = String::new();
    let mut binds = HashSet::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            out.push('\'');
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if chars[i] == ':'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '_')
        {
            if i > 0 && chars[i - 1] == ':' {
                out.push(':');
                i += 1;
                continue;
            }
            i += 1;
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            binds.insert(chars[start..i].iter().collect());
            out.push_str("NULL");
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    (out, binds)
}

fn parse_as_select_expr(expr: &str) -> Result<Expr, AppError> {
    let sql = format!("SELECT ({expr})");
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, &sql)
        .map_err(|e| AppError::Validation(format!("expr: {e}")))?;
    if stmts.len() != 1 {
        return Err(AppError::Validation(
            "expression must be a single expr".into(),
        ));
    }
    match stmts.into_iter().next() {
        Some(Statement::Query(q)) => match q.body.as_ref() {
            SetExpr::Select(sel) => {
                if sel.projection.len() != 1 {
                    return Err(AppError::Validation(
                        "expression must project a single value".into(),
                    ));
                }
                match &sel.projection[0] {
                    sqlparser::ast::SelectItem::UnnamedExpr(e)
                    | sqlparser::ast::SelectItem::ExprWithAlias { expr: e, .. } => Ok(e.clone()),
                    _ => Err(AppError::Validation("unsupported select item".into())),
                }
            }
            _ => Err(AppError::Validation(
                "expression must be a value expr".into(),
            )),
        },
        _ => Err(AppError::Validation(
            "expression must be a value expr".into(),
        )),
    }
}

fn parse_as_where_expr(expr: &str) -> Result<Expr, AppError> {
    let sql = format!("SELECT 1 WHERE ({expr})");
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, &sql)
        .map_err(|e| AppError::Validation(format!("filter: {e}")))?;
    if stmts.len() != 1 {
        return Err(AppError::Validation("filter must be a single expr".into()));
    }
    match stmts.into_iter().next() {
        Some(Statement::Query(q)) => match q.body.as_ref() {
            SetExpr::Select(sel) => sel
                .selection
                .clone()
                .ok_or_else(|| AppError::Validation("filter required".into())),
            _ => Err(AppError::Validation("filter must be a boolean expr".into())),
        },
        _ => Err(AppError::Validation("filter must be a boolean expr".into())),
    }
}

fn parse_expr_loose(expr: &str) -> Result<Expr, AppError> {
    parse_as_select_expr(expr).or_else(|_| parse_as_where_expr(expr))
}

fn walk_value_expr_opts(
    expr: &Expr,
    allow_agg: bool,
    allow_subquery: bool,
) -> Result<(), AppError> {
    if !allow_subquery {
        reject_subquery(expr)?;
    }
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) | Expr::Value(_) => Ok(()),
        Expr::Subquery(q) if allow_subquery => {
            crate::select_guard::assert_select_only(&q.to_string())
        }
        Expr::UnaryOp { op, expr } => {
            if !matches!(op, UnaryOperator::Plus | UnaryOperator::Minus) {
                return Err(AppError::Validation(format!(
                    "unary op not allowed in metric expr: {op}"
                )));
            }
            walk_value_expr_opts(expr, allow_agg, allow_subquery)
        }
        Expr::BinaryOp { left, op, right } => {
            if !matches!(
                op,
                BinaryOperator::Plus
                    | BinaryOperator::Minus
                    | BinaryOperator::Multiply
                    | BinaryOperator::Divide
                    | BinaryOperator::Modulo
                    | BinaryOperator::StringConcat
            ) {
                return Err(AppError::Validation(format!(
                    "binary op not allowed in metric expr: {op}"
                )));
            }
            walk_value_expr_opts(left, allow_agg, allow_subquery)?;
            walk_value_expr_opts(right, allow_agg, allow_subquery)
        }
        Expr::Nested(inner) => walk_value_expr_opts(inner, allow_agg, allow_subquery),
        Expr::Cast { expr, .. } => walk_value_expr_opts(expr, allow_agg, allow_subquery),
        Expr::Extract { field, expr, .. } => {
            if !EXTRACT_FIELDS.contains(field) {
                return Err(AppError::Validation(format!(
                    "EXTRACT field not allowed: {field}"
                )));
            }
            walk_value_expr_opts(expr, allow_agg, allow_subquery)
        }
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            walk_value_expr_opts(expr, allow_agg, allow_subquery)?;
            if let Some(f) = substring_from {
                walk_value_expr_opts(f, allow_agg, allow_subquery)?;
            }
            if let Some(l) = substring_for {
                walk_value_expr_opts(l, allow_agg, allow_subquery)?;
            }
            Ok(())
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            walk_value_expr_opts(expr, allow_agg, allow_subquery)?;
            if let Some(w) = trim_what {
                walk_value_expr_opts(w, allow_agg, allow_subquery)?;
            }
            if let Some(chars) = trim_characters {
                for c in chars {
                    walk_value_expr_opts(c, allow_agg, allow_subquery)?;
                }
            }
            Ok(())
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                walk_value_expr_opts(op, allow_agg, allow_subquery)?;
            }
            for cw in conditions {
                if operand.is_some() {
                    walk_value_expr_opts(&cw.condition, allow_agg, allow_subquery)?;
                } else {
                    walk_bool_expr_full(&cw.condition, false, allow_subquery)?;
                }
                walk_value_expr_opts(&cw.result, allow_agg, allow_subquery)?;
            }
            if let Some(el) = else_result {
                walk_value_expr_opts(el, allow_agg, allow_subquery)?;
            }
            Ok(())
        }
        Expr::Function(func) => {
            let name = func.name.to_string();
            let lower = name.to_ascii_lowercase();
            let is_agg = matches!(lower.as_str(), "sum" | "count" | "avg" | "min" | "max");
            let is_win_only = WINDOW_ONLY_FUNCS.contains(&lower.as_str());
            if let Some(over) = &func.over {
                if !is_agg && !is_win_only {
                    return Err(AppError::Validation(format!(
                        "function does not support OVER clause: {name}"
                    )));
                }
                walk_function_args_opts(&func.args, allow_agg, allow_subquery)?;
                match over {
                    WindowType::WindowSpec(spec) => {
                        for p in &spec.partition_by {
                            walk_value_expr_opts(p, allow_agg, allow_subquery)?;
                        }
                        for ob in &spec.order_by {
                            walk_value_expr_opts(&ob.expr, allow_agg, allow_subquery)?;
                        }
                        Ok(())
                    }
                    WindowType::NamedWindow(_) => Err(AppError::Validation(
                        "named window is not supported; use inline OVER (...)".into(),
                    )),
                }
            } else {
                if is_win_only {
                    return Err(AppError::Validation(format!(
                        "window function {name} requires OVER (...) clause"
                    )));
                }
                if is_agg && !allow_agg {
                    return Err(AppError::Validation(format!(
                        "aggregate not allowed here: {name}"
                    )));
                }
                if !METRIC_FUNCS.contains(&lower.as_str()) {
                    return Err(AppError::Validation(forbidden_function_message(
                        "metric expr",
                        &name,
                    )));
                }
                if lower == "iif" {
                    walk_iif_args(&func.args, allow_agg, allow_subquery)
                } else {
                    walk_function_args_opts(&func.args, allow_agg && !is_agg, allow_subquery)
                }
            }
        }
        other => Err(AppError::Validation(format!(
            "unsupported metric expression form: {other}"
        ))),
    }
}

fn walk_bool_expr_full(expr: &Expr, allow_agg: bool, allow_subquery: bool) -> Result<(), AppError> {
    if !allow_subquery {
        reject_subquery(expr)?;
    }
    match expr {
        Expr::Nested(inner) => walk_bool_expr_full(inner, allow_agg, allow_subquery),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => walk_bool_expr_full(expr, allow_agg, allow_subquery),
        Expr::Subquery(q) | Expr::Exists { subquery: q, .. } if allow_subquery => {
            crate::select_guard::assert_select_only(&q.to_string())
        }
        Expr::InSubquery { expr, subquery, .. } if allow_subquery => {
            walk_value_expr_opts(expr, allow_agg, allow_subquery)?;
            crate::select_guard::assert_select_only(&subquery.to_string())
        }
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor => {
                walk_bool_expr_full(left, allow_agg, allow_subquery)?;
                walk_bool_expr_full(right, allow_agg, allow_subquery)
            }
            BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::LtEq
            | BinaryOperator::Gt
            | BinaryOperator::GtEq => {
                walk_value_expr_opts(left, allow_agg, allow_subquery)?;
                walk_value_expr_opts(right, allow_agg, allow_subquery)
            }
            _ => Err(AppError::Validation(format!(
                "binary op not allowed in filter: {op}"
            ))),
        },
        Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
            walk_value_expr_opts(expr, allow_agg, allow_subquery)?;
            walk_value_expr_opts(pattern, allow_agg, allow_subquery)
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => {
            walk_value_expr_opts(inner, allow_agg, allow_subquery)
        }
        Expr::InList { expr, list, .. } => {
            walk_value_expr_opts(expr, allow_agg, allow_subquery)?;
            for item in list {
                walk_value_expr_opts(item, allow_agg, allow_subquery)?;
            }
            Ok(())
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            walk_value_expr_opts(expr, allow_agg, allow_subquery)?;
            walk_value_expr_opts(low, allow_agg, allow_subquery)?;
            walk_value_expr_opts(high, allow_agg, allow_subquery)
        }
        Expr::Function(func) => {
            let name = func.name.to_string();
            let lower = name.to_ascii_lowercase();
            if !FILTER_FUNCS.contains(&lower.as_str()) {
                return Err(AppError::Validation(forbidden_function_message(
                    "filter", &name,
                )));
            }
            if lower == "iif" {
                walk_iif_args(&func.args, allow_agg, allow_subquery)
            } else {
                walk_function_args_opts(&func.args, allow_agg, allow_subquery)
            }
        }
        other => Err(AppError::Validation(format!(
            "unsupported filter expression form: {other}"
        ))),
    }
}

fn extract_fn_arg_expr(arg: &FunctionArg) -> Result<&Expr, AppError> {
    match arg {
        FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
        | FunctionArg::Named {
            arg: FunctionArgExpr::Expr(e),
            ..
        } => Ok(e),
        _ => Err(AppError::Validation("unsupported IIF argument".into())),
    }
}

fn walk_iif_args(
    args: &FunctionArguments,
    allow_agg: bool,
    allow_subquery: bool,
) -> Result<(), AppError> {
    let FunctionArguments::List(list) = args else {
        return Err(AppError::Validation(
            "IIF requires (cond, then_val, else_val)".into(),
        ));
    };
    if list.args.len() != 3 {
        return Err(AppError::Validation(
            "IIF requires exactly 3 arguments".into(),
        ));
    }
    let cond = extract_fn_arg_expr(&list.args[0])?;
    let then_val = extract_fn_arg_expr(&list.args[1])?;
    let else_val = extract_fn_arg_expr(&list.args[2])?;
    walk_bool_expr_full(cond, allow_agg, allow_subquery)
        .or_else(|_| walk_value_expr_opts(cond, allow_agg, allow_subquery))?;
    walk_value_expr_opts(then_val, allow_agg, allow_subquery)?;
    walk_value_expr_opts(else_val, allow_agg, allow_subquery)
}

fn walk_function_args_opts(
    args: &FunctionArguments,
    allow_agg: bool,
    allow_subquery: bool,
) -> Result<(), AppError> {
    match args {
        FunctionArguments::None => Ok(()),
        FunctionArguments::Subquery(q) => {
            if allow_subquery {
                crate::select_guard::assert_select_only(&q.to_string())
            } else {
                Err(AppError::Validation(
                    "subquery not allowed in expression".into(),
                ))
            }
        }
        FunctionArguments::List(list) => {
            for arg in &list.args {
                match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(e),
                        ..
                    } => walk_value_expr_opts(e, allow_agg, allow_subquery)?,
                    FunctionArg::Unnamed(FunctionArgExpr::Wildcard)
                    | FunctionArg::Unnamed(FunctionArgExpr::QualifiedWildcard(_)) => {}
                    _ => {
                        return Err(AppError::Validation("unsupported function argument".into()));
                    }
                }
            }
            Ok(())
        }
    }
}

pub fn reject_subquery(expr: &Expr) -> Result<(), AppError> {
    match expr {
        Expr::Subquery(_) | Expr::InSubquery { .. } | Expr::Exists { .. } => Err(
            AppError::Validation("subquery not allowed in expression".into()),
        ),
        _ => Ok(()),
    }
}

fn collect_idents(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Identifier(id) => {
            out.insert(id.value.clone());
        }
        Expr::CompoundIdentifier(parts) => {
            if let Some(last) = parts.last() {
                out.insert(last.value.clone());
            }
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::Cast { expr, .. }
        | Expr::Extract { expr, .. } => {
            collect_idents(expr, out);
        }
        Expr::Substring {
            expr,
            substring_from,
            substring_for,
            ..
        } => {
            collect_idents(expr, out);
            if let Some(f) = substring_from {
                collect_idents(f, out);
            }
            if let Some(l) = substring_for {
                collect_idents(l, out);
            }
        }
        Expr::Trim {
            expr,
            trim_what,
            trim_characters,
            ..
        } => {
            collect_idents(expr, out);
            if let Some(w) = trim_what {
                collect_idents(w, out);
            }
            if let Some(chars) = trim_characters {
                for c in chars {
                    collect_idents(c, out);
                }
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_idents(left, out);
            collect_idents(right, out);
        }
        Expr::Function(func) => {
            if let FunctionArguments::List(list) = &func.args {
                for arg in &list.args {
                    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(e),
                        ..
                    } = arg
                    {
                        collect_idents(e, out);
                    }
                }
            }
            if let Some(WindowType::WindowSpec(spec)) = &func.over {
                for p in &spec.partition_by {
                    collect_idents(p, out);
                }
                for ob in &spec.order_by {
                    collect_idents(&ob.expr, out);
                }
            }
        }
        Expr::IsNull(e) | Expr::IsNotNull(e) => collect_idents(e, out),
        Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
            collect_idents(expr, out);
            collect_idents(pattern, out);
        }
        Expr::InList { expr, list, .. } => {
            collect_idents(expr, out);
            for item in list {
                collect_idents(item, out);
            }
        }
        Expr::InSubquery { expr, .. } => {
            collect_idents(expr, out);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_idents(expr, out);
            collect_idents(low, out);
            collect_idents(high, out);
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_idents(op, out);
            }
            for cw in conditions {
                collect_idents(&cw.condition, out);
                collect_idents(&cw.result, out);
            }
            if let Some(el) = else_result {
                collect_idents(el, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_metric_sum() {
        assert_metric_expr("sum(sale_amount)").unwrap();
        assert_metric_expr("sale_amount * fx_rate").unwrap();
    }

    #[test]
    fn adhoc_requires_aggregate() {
        assert_adhoc_agg_expr("sum(sale_amount)").unwrap();
        assert!(assert_adhoc_agg_expr("sale_amount * fx_rate").is_err());
    }

    #[test]
    fn scalar_expr_rejects_aggregate_but_accepts_math_and_date_trunc() {
        assert_scalar_expr("sale_amount * fx_rate").unwrap();
        assert_scalar_expr("date_trunc('month', order_date)").unwrap();
        assert!(assert_scalar_expr("sum(sale_amount)").is_err());
    }

    #[test]
    fn having_expr_accepts_aggregates_and_scalars() {
        assert_having_expr("COUNT(*) > 20").unwrap();
        assert_having_expr("sum(sale_amount) >= 100 AND region_code = 'KR'").unwrap();
        assert_having_expr("(a - b) > avg(a - b)").unwrap();
        assert!(assert_filter_expr("COUNT(*) > 20").is_err());
    }

    #[test]
    fn adhoc_metric_allows_subquery() {
        assert_metric_expr("sum(sale_amount) / (select sum(sale_amount) from sales)").unwrap();
        assert_scalar_expr("(select max(sale_amount) from sales)").unwrap();
    }

    #[test]
    fn accepts_filter_predicate() {
        assert_filter_expr("status_code = 'PAID'").unwrap();
        assert_filter_expr("region_code = :caller_region").unwrap();
    }

    #[test]
    fn rejects_filter_subquery() {
        assert!(assert_filter_expr("id in (select id from t)").is_err());
    }

    #[test]
    fn metadata_metric_and_segment_filter_allow_subqueries() {
        assert_metadata_metric_expr(
            "sum(sale_amount) * 100.0 / (SELECT sum(sale_amount) FROM sales)",
        )
        .unwrap();
        assert_segment_filter_expr("sale_amount > (SELECT avg(sale_amount) FROM sales)").unwrap();
        assert_segment_filter_expr(
            "customer_id IN (SELECT customer_id FROM sales WHERE status_code = 'PAID')",
        )
        .unwrap();
        assert_segment_filter_expr("EXISTS (SELECT 1 FROM sales WHERE sale_amount > 1000)")
            .unwrap();

        let known: HashSet<_> = ["sale_amount", "customer_id"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_idents_known("sale_amount > (SELECT avg(other_col) FROM t)", &known).unwrap();
        assert_idents_known("customer_id IN (SELECT id FROM t)", &known).unwrap();
    }

    #[test]
    fn accepts_window_functions_and_collects_over_idents() {
        assert_scalar_expr(
            "ROW_NUMBER() OVER (PARTITION BY region_code ORDER BY sale_amount DESC)",
        )
        .unwrap();
        assert_scalar_expr("SUM(sale_amount) OVER (PARTITION BY region_code)").unwrap();
        assert_scalar_expr("LAG(sale_amount, 1, 0) OVER (ORDER BY order_date)").unwrap();
        assert_adhoc_agg_expr("RANK() OVER (ORDER BY SUM(sale_amount) DESC)").unwrap();
        assert_adhoc_agg_expr("SUM(sale_amount) * 100.0 / SUM(SUM(sale_amount)) OVER ()").unwrap();
        assert!(contains_window_sql("ROW_NUMBER() OVER ()").unwrap());
        assert!(!contains_aggregate_sql("ROW_NUMBER() OVER (ORDER BY sale_amount DESC)").unwrap());
        assert!(contains_aggregate_sql("RANK() OVER (ORDER BY SUM(sale_amount) DESC)").unwrap());

        // Window function without OVER must be rejected
        assert!(assert_scalar_expr("ROW_NUMBER()").is_err());

        let known: HashSet<_> = ["region_code", "sale_amount"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_idents_known(
            "ROW_NUMBER() OVER (PARTITION BY region_code ORDER BY sale_amount DESC)",
            &known,
        )
        .unwrap();
        assert!(assert_idents_known("ROW_NUMBER() OVER (ORDER BY unknown_col)", &known).is_err());
    }

    #[test]
    fn idents_must_be_known() {
        let known: HashSet<_> = ["sale_amount", "fx_rate"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_idents_known("sum(sale_amount)", &known).unwrap();
        assert!(assert_idents_known("sum(unknown_col)", &known).is_err());
    }

    #[test]
    fn accepts_sqlite_and_olap_helper_functions() {
        assert_scalar_expr("substr(order_date, 1, 4)").unwrap();
        assert_scalar_expr("substring(order_date, 1, 4)").unwrap();
        assert_scalar_expr("length(region_code)").unwrap();
        assert_scalar_expr("replace(order_date, '-', '')").unwrap();
        assert_scalar_expr("instr(region_code, 'K')").unwrap();
        assert_scalar_expr("ifnull(sale_amount, 0)").unwrap();
        assert_scalar_expr("iif(status_code = 'PAID', sale_amount, 0)").unwrap();
        assert_scalar_expr("strftime('%Y', order_date)").unwrap();
        assert_scalar_expr("year(order_date)").unwrap();
        assert_scalar_expr("month(order_date)").unwrap();
        assert_scalar_expr("day(order_date)").unwrap();
        assert_scalar_expr("divide(sale_amount, fx_rate)").unwrap();
        assert_adhoc_agg_expr("sum(iif(status_code = 'PAID', 1, 0))").unwrap();
        assert_adhoc_agg_expr("divide(sum(sale_amount), count(*))").unwrap();
        assert_filter_expr("substr(order_date, 1, 4) = '2024'").unwrap();
        assert_filter_expr("year(order_date) = 2024").unwrap();

        let known: HashSet<_> = ["status_code", "sale_amount", "order_date"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_idents_known("sum(iif(status_code = 'PAID', sale_amount, 0))", &known).unwrap();
        assert_idents_known("substring(order_date, 1, 4)", &known).unwrap();
    }

    #[test]
    fn accepts_d25_temporal_helpers() {
        assert_scalar_expr("CURRENT_DATE").unwrap();
        assert_scalar_expr("CURRENT_TIMESTAMP").unwrap();
        assert_scalar_expr("NOW()").unwrap();
        assert_scalar_expr("DATEDIFF(CURRENT_DATE, order_date)").unwrap();
        assert_scalar_expr("EXTRACT(YEAR FROM order_date)").unwrap();
        assert_scalar_expr("EXTRACT(MONTH FROM order_date)").unwrap();
        assert_scalar_expr("EXTRACT(DAY FROM order_date)").unwrap();
        assert_scalar_expr("EXTRACT(HOUR FROM order_date)").unwrap();
        assert_scalar_expr("YEAR(CURRENT_DATE) - YEAR(order_date)").unwrap();
        assert_metric_expr("AVG(DATEDIFF(CURRENT_DATE, order_date))").unwrap();
        assert_filter_expr("DATEDIFF(CURRENT_DATE, order_date) > 30").unwrap();
        assert_filter_expr("EXTRACT(YEAR FROM order_date) = 2024").unwrap();
        assert!(assert_scalar_expr("EXTRACT(WEEK FROM order_date)").is_err());

        let known: HashSet<_> = ["order_date"].into_iter().map(str::to_string).collect();
        assert_idents_known("EXTRACT(YEAR FROM order_date)", &known).unwrap();
        assert_idents_known("DATEDIFF(CURRENT_DATE, order_date)", &known).unwrap();
    }
}
