//! D16/D18: trusted SQL fragment AST limits + identifier checks at compile time.

use std::collections::HashSet;

use sqlparser::ast::{
    BinaryOperator, Expr, FunctionArg, FunctionArgExpr, FunctionArguments, SetExpr, Statement,
    UnaryOperator,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::AppError;

const METRIC_FUNCS: &[&str] = &[
    "sum", "count", "avg", "min", "max", "coalesce", "nullif", "greatest", "least", "abs", "round",
    "date_trunc", "lower", "upper", "trim",
];

const FILTER_FUNCS: &[&str] = &[
    "coalesce", "nullif", "lower", "upper", "trim", "abs", "round", "date_trunc",
];

/// Metric `expression`: single scalar/aggregate expr (D16).
pub fn assert_metric_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_select_expr(&normalized)?;
    walk_value_expr(&e, true)?;
    Ok(())
}

/// Filter / segment predicate: single boolean expr (D16).
pub fn assert_filter_expr(expr: &str) -> Result<(), AppError> {
    let (normalized, _) = replace_binds(expr);
    let e = parse_as_where_expr(&normalized)?;
    walk_bool_expr(&e)?;
    Ok(())
}

/// Identifiers in `expr` must be subset of `known` (D18). `:bind` placeholders ignored.
pub fn assert_idents_known(expr: &str, known: &HashSet<String>) -> Result<(), AppError> {
    let found = extract_idents(expr)?;
    for id in &found {
        if !known.contains(id) {
            return Err(AppError::Validation(format!(
                "unknown identifier in expression: {id}"
            )));
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
    let stmts =
        Parser::parse_sql(&dialect, &sql).map_err(|e| AppError::Validation(format!("expr: {e}")))?;
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
            _ => Err(AppError::Validation(
                "filter must be a boolean expr".into(),
            )),
        },
        _ => Err(AppError::Validation(
            "filter must be a boolean expr".into(),
        )),
    }
}

fn parse_expr_loose(expr: &str) -> Result<Expr, AppError> {
    parse_as_select_expr(expr).or_else(|_| parse_as_where_expr(expr))
}

fn walk_value_expr(expr: &Expr, allow_agg: bool) -> Result<(), AppError> {
    reject_subquery(expr)?;
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) | Expr::Value(_) => Ok(()),
        Expr::UnaryOp { op, expr } => {
            if !matches!(op, UnaryOperator::Plus | UnaryOperator::Minus) {
                return Err(AppError::Validation(format!(
                    "unary op not allowed in metric expr: {op}"
                )));
            }
            walk_value_expr(expr, allow_agg)
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
            walk_value_expr(left, allow_agg)?;
            walk_value_expr(right, allow_agg)
        }
        Expr::Nested(inner) => walk_value_expr(inner, allow_agg),
        Expr::Cast { expr, .. } => walk_value_expr(expr, allow_agg),
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                walk_value_expr(op, allow_agg)?;
            }
            for cw in conditions {
                if operand.is_some() {
                    walk_value_expr(&cw.condition, allow_agg)?;
                } else {
                    walk_bool_expr(&cw.condition)?;
                }
                walk_value_expr(&cw.result, allow_agg)?;
            }
            if let Some(el) = else_result {
                walk_value_expr(el, allow_agg)?;
            }
            Ok(())
        }
        Expr::Function(func) => {
            let name = func.name.to_string();
            let lower = name.to_ascii_lowercase();
            let is_agg = matches!(lower.as_str(), "sum" | "count" | "avg" | "min" | "max");
            if is_agg && !allow_agg {
                return Err(AppError::Validation(format!(
                    "aggregate not allowed here: {name}"
                )));
            }
            if !METRIC_FUNCS.contains(&lower.as_str()) {
                return Err(AppError::Validation(format!(
                    "function not allowed in metric expr: {name}"
                )));
            }
            walk_function_args(&func.args, allow_agg && !is_agg)
        }
        other => Err(AppError::Validation(format!(
            "unsupported metric expression form: {other}"
        ))),
    }
}

fn walk_bool_expr(expr: &Expr) -> Result<(), AppError> {
    reject_subquery(expr)?;
    match expr {
        Expr::Nested(inner) => walk_bool_expr(inner),
        Expr::UnaryOp {
            op: UnaryOperator::Not,
            expr,
        } => walk_bool_expr(expr),
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor => {
                walk_bool_expr(left)?;
                walk_bool_expr(right)
            }
            BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::LtEq
            | BinaryOperator::Gt
            | BinaryOperator::GtEq => {
                walk_value_expr(left, false)?;
                walk_value_expr(right, false)
            }
            _ => Err(AppError::Validation(format!(
                "binary op not allowed in filter: {op}"
            ))),
        },
        Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
            walk_value_expr(expr, false)?;
            walk_value_expr(pattern, false)
        }
        Expr::IsNull(inner) | Expr::IsNotNull(inner) => walk_value_expr(inner, false),
        Expr::InList { expr, list, .. } => {
            walk_value_expr(expr, false)?;
            for item in list {
                walk_value_expr(item, false)?;
            }
            Ok(())
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            walk_value_expr(expr, false)?;
            walk_value_expr(low, false)?;
            walk_value_expr(high, false)
        }
        Expr::Function(func) => {
            let name = func.name.to_string();
            let lower = name.to_ascii_lowercase();
            if !FILTER_FUNCS.contains(&lower.as_str()) {
                return Err(AppError::Validation(format!(
                    "function not allowed in filter: {name}"
                )));
            }
            walk_function_args(&func.args, false)
        }
        other => Err(AppError::Validation(format!(
            "unsupported filter expression form: {other}"
        ))),
    }
}

fn walk_function_args(args: &FunctionArguments, allow_agg: bool) -> Result<(), AppError> {
    match args {
        FunctionArguments::None => Ok(()),
        FunctionArguments::Subquery(_) => Err(AppError::Validation(
            "subquery not allowed in expression".into(),
        )),
        FunctionArguments::List(list) => {
            for arg in &list.args {
                match arg {
                    FunctionArg::Unnamed(FunctionArgExpr::Expr(e))
                    | FunctionArg::Named {
                        arg: FunctionArgExpr::Expr(e),
                        ..
                    } => walk_value_expr(e, allow_agg)?,
                    FunctionArg::Unnamed(FunctionArgExpr::Wildcard)
                    | FunctionArg::Unnamed(FunctionArgExpr::QualifiedWildcard(_)) => {}
                    _ => {
                        return Err(AppError::Validation(
                            "unsupported function argument".into(),
                        ));
                    }
                }
            }
            Ok(())
        }
    }
}

fn reject_subquery(expr: &Expr) -> Result<(), AppError> {
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
        Expr::UnaryOp { expr, .. } | Expr::Nested(expr) | Expr::Cast { expr, .. } => {
            collect_idents(expr, out);
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
    fn rejects_metric_subquery() {
        assert!(assert_metric_expr("(select 1)").is_err());
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
    fn idents_must_be_known() {
        let known: HashSet<_> = ["sale_amount", "fx_rate"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_idents_known("sum(sale_amount)", &known).unwrap();
        assert!(assert_idents_known("sum(unknown_col)", &known).is_err());
    }
}
