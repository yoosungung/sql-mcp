//! Semantic SQL → physical SQL (spec 6-step pipeline entry).
//! Free physical SELECT is not supported — input must reference a cube.

use std::collections::HashMap;

use serde_json::Value;
use sqlparser::ast::{
    Expr, GroupByExpr, Query, SelectItem, SetExpr, Statement, TableFactor, TableWithJoins,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::compile::compile_metric_select_with_binds;
use crate::error::AppError;
use crate::select_guard::assert_select_only;

/// Compile semantic SQL to physical SELECT. `role` applies table permission_filters.
pub fn compile_semantic_sql(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
) -> Result<String, AppError> {
    compile_semantic_sql_with_binds(docs, sql, role, None)
}

pub fn compile_semantic_sql_with_binds(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    assert_select_only(sql)?;
    let dialect = GenericDialect {};
    let stmts =
        Parser::parse_sql(&dialect, sql).map_err(|e| AppError::SqlParse(e.to_string()))?;
    let Statement::Query(query) = stmts
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotSelect)?
    else {
        return Err(AppError::NotSelect);
    };

    let cube_name = extract_cube_name(&query)?;
    let cube = find_doc(docs, "cube", &cube_name)?;
    check_role_access(cube.get("role"), role)?;

    let (metrics, dims) = classify_projection(&query, docs, &cube_name)?;
    let segments = extract_segments(&query, docs, &cube_name)?;

    if metrics.is_empty() {
        return Err(AppError::Validation(
            "semantic SELECT must project at least one metric".into(),
        ));
    }
    if metrics.len() > 1 {
        return Err(AppError::Validation(
            "multiple metrics in one SELECT not yet supported".into(),
        ));
    }

    let metric_name = &metrics[0];
    // Step 2: semi-additive flag (reserved for summary-view routing).
    let _is_semi_additive = is_semi_additive_metric(docs, metric_name)?;

    let seg_refs: Vec<&str> = segments.iter().map(String::as_str).collect();
    let physical =
        compile_metric_select_with_binds(docs, metric_name, &seg_refs, role, binds)?;

    // If caller listed dimensions, ensure they appear (compile_metric_select uses metric group_by).
    if !dims.is_empty() {
        let metric = find_doc(docs, "metric", metric_name)?;
        let gb: Vec<String> = metric
            .get("group_by")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        for d in &dims {
            if !gb.iter().any(|g| g == d) {
                return Err(AppError::Validation(format!(
                    "dimension {d} not in metric {metric_name} group_by"
                )));
            }
        }
    }

    assert_select_only(&physical)?;
    Ok(physical)
}

fn extract_cube_name(query: &Query) -> Result<String, AppError> {
    let SetExpr::Select(select) = query.body.as_ref() else {
        return Err(AppError::Validation(
            "semantic SQL must be a plain SELECT".into(),
        ));
    };
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SQL must have exactly one FROM cube".into(),
        ));
    }
    table_name(&select.from[0])
}

fn table_name(tw: &TableWithJoins) -> Result<String, AppError> {
    if !tw.joins.is_empty() {
        return Err(AppError::Validation(
            "semantic FROM must be a single cube (no joins)".into(),
        ));
    }
    match &tw.relation {
        TableFactor::Table { name, .. } => {
            if name.0.len() != 1 {
                return Err(AppError::Validation(
                    "physical schema.table FROM is not allowed; use cube name only".into(),
                ));
            }
            Ok(name.0[0].to_string())
        }
        _ => Err(AppError::Validation(
            "semantic FROM must be a cube identifier".into(),
        )),
    }
}

fn classify_projection(
    query: &Query,
    docs: &[Value],
    cube_name: &str,
) -> Result<(Vec<String>, Vec<String>), AppError> {
    let SetExpr::Select(select) = query.body.as_ref() else {
        return Err(AppError::Validation("expected SELECT".into()));
    };
    let metric_names: Vec<String> = docs
        .iter()
        .filter(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .filter_map(|d| d.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let cube = find_doc(docs, "cube", cube_name)?;
    let mut dim_names = Vec::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    dim_names.push(n.to_string());
                }
            }
        }
    }

    let mut metrics = Vec::new();
    let mut dims = Vec::new();
    for item in &select.projection {
        let ident = match item {
            SelectItem::UnnamedExpr(Expr::Identifier(id)) => id.value.clone(),
            SelectItem::ExprWithAlias {
                expr: Expr::Identifier(id),
                ..
            } => id.value.clone(),
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                return Err(AppError::Validation(
                    "SELECT * not allowed in semantic SQL".into(),
                ));
            }
            _ => {
                return Err(AppError::Validation(
                    "semantic projection must be metric/dimension identifiers".into(),
                ));
            }
        };
        if metric_names.iter().any(|m| m == &ident) {
            metrics.push(ident);
        } else if dim_names.iter().any(|d| d == &ident) {
            dims.push(ident);
        } else {
            return Err(AppError::Validation(format!(
                "unknown semantic member: {ident}"
            )));
        }
    }
    Ok((metrics, dims))
}

fn extract_segments(
    query: &Query,
    docs: &[Value],
    cube_name: &str,
) -> Result<Vec<String>, AppError> {
    let SetExpr::Select(select) = query.body.as_ref() else {
        return Ok(vec![]);
    };
    let Some(selection) = &select.selection else {
        return Ok(vec![]);
    };
    let seg_names: Vec<String> = docs
        .iter()
        .filter(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("segment")
                && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .filter_map(|d| d.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let mut found = Vec::new();
    collect_segment_idents(selection, &seg_names, &mut found);
    Ok(found)
}

fn collect_segment_idents(expr: &Expr, known: &[String], out: &mut Vec<String>) {
    match expr {
        Expr::Identifier(id) => {
            if known.iter().any(|k| k == &id.value) && !out.iter().any(|x| x == &id.value) {
                out.push(id.value.clone());
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_segment_idents(left, known, out);
            collect_segment_idents(right, known, out);
        }
        Expr::UnaryOp { expr, .. } => collect_segment_idents(expr, known, out),
        Expr::Nested(e) => collect_segment_idents(e, known, out),
        _ => {}
    }
}

fn is_semi_additive_metric(docs: &[Value], metric_name: &str) -> Result<bool, AppError> {
    let metric = find_doc(docs, "metric", metric_name)?;
    let expr = metric
        .get("expression")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(expr.contains("count(distinct")
        || expr.contains("avg(")
        || expr.contains("median("))
}

fn check_role_access(role_val: Option<&Value>, caller: Option<&str>) -> Result<(), AppError> {
    let Some(role_val) = role_val else {
        return Ok(());
    };
    if role_val.as_str() == Some("all") || role_val.is_null() {
        return Ok(());
    }
    let Some(caller) = caller else {
        return Err(AppError::Forbidden(
            "caller_role required for restricted cube".into(),
        ));
    };
    match role_val {
        Value::String(s) if s == "all" || s == caller => Ok(()),
        Value::Array(arr) if arr.iter().any(|v| v.as_str() == Some(caller)) => Ok(()),
        Value::String(s) if s == caller => Ok(()),
        _ => Err(AppError::Forbidden(format!(
            "role {caller} cannot access this cube"
        ))),
    }
}

fn find_doc<'a>(docs: &'a [Value], kind: &str, name: &str) -> Result<&'a Value, AppError> {
    docs.iter()
        .find(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some(kind)
                && d.get("name").and_then(|v| v.as_str()) == Some(name)
        })
        .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))
}

#[allow(dead_code)]
fn _group_by_unused(g: &GroupByExpr) {
    let _ = g;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn example_docs() -> Vec<Value> {
        [
            include_str!("../../semantic/examples/tables/customer.json"),
            include_str!("../../semantic/examples/tables/orders.json"),
            include_str!("../../semantic/examples/tables/orders_month.json"),
            include_str!("../../semantic/examples/views/orders_customer.json"),
            include_str!("../../semantic/examples/cubes/sales.json"),
            include_str!("../../semantic/examples/metrics/revenue.json"),
            include_str!("../../semantic/examples/segments/paid.json"),
        ]
        .into_iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
    }

    #[test]
    fn compiles_revenue_with_segment() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales WHERE paid",
            Some("branch"),
        )
        .unwrap();
        assert!(sql.to_ascii_lowercase().contains("sum("));
        assert!(sql.contains("PAID") || sql.contains("paid") || sql.contains("'PAID'"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_physical_from() {
        let docs = example_docs();
        let err = compile_semantic_sql(
            &docs,
            "SELECT order_id FROM sales.orders",
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            AppError::Validation(_) | AppError::NotFound(_)
        ));
    }

    #[test]
    fn rejects_bare_physical_select() {
        let docs = example_docs();
        assert!(compile_semantic_sql(&docs, "SELECT 1", None).is_err());
    }
}
