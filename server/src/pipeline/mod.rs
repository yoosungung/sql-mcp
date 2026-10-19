//! Semantic SQL → physical SQL (spec 6-step pipeline entry).
//! Free physical SELECT is not supported — input must reference cube(s).
//! Same MCP `target` cubes may JOIN / UNION (·EXCEPT/INTERSECT).

mod clauses;
mod join;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sqlparser::ast::{
    Query, Select, SetExpr, SetOperator, SetQuantifier,
    Statement, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::compile::{
    CubeQuerySpec, check_role_access, compile_cube_query,
};
use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::expr_guard::{
    contains_aggregate_sql, extract_idents,
};
use crate::select_guard::assert_select_only;

use clauses::{
    classify_projection_for_cube, ensure_projection_mode, extract_group_by_for_cube,
    extract_having_for_cube, extract_limit, extract_offset, extract_order_by,
    extract_where_for_cube, materialize_adhoc_metrics,
};
use join::compile_cube_join;

/// Compile semantic SQL to physical SELECT. `role` applies table permission_filters.
/// Defaults to Postgres dialect emit (ANSI quotes).
pub fn compile_semantic_sql(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
) -> Result<String, AppError> {
    compile_semantic_sql_with_binds(docs, sql, role, None, DialectProfile::Postgres)
}

/// All FROM cube names in a semantic SELECT (including JOIN/UNION arms).
pub fn cube_names_from_semantic_sql(sql: &str) -> Result<Vec<String>, AppError> {
    let query = parse_query(sql)?;
    let mut names = Vec::new();
    collect_cubes_from_set_expr(query.body.as_ref(), &mut names)?;
    Ok(names)
}

/// First FROM cube name (compat helper).
pub fn cube_name_from_semantic_sql(sql: &str) -> Result<String, AppError> {
    cube_names_from_semantic_sql(sql)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Validation("semantic SQL has no FROM cube".into()))
}

pub fn compile_semantic_sql_with_binds(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    dialect: DialectProfile,
) -> Result<String, AppError> {
    let query = parse_query(sql)?;
    let mut cubes = Vec::new();
    collect_cubes_from_set_expr(query.body.as_ref(), &mut cubes)?;
    if cubes.is_empty() {
        return Err(AppError::Validation(
            "semantic SQL must reference at least one cube".into(),
        ));
    }
    assert_same_cube_target(docs, &cubes)?;
    for c in &cubes {
        let cube = find_doc(docs, "cube", c)?;
        check_role_access(cube.get("role"), role)?;
    }
    // Steps 1–7: engine-neutral canonical SQL (ANSI "…" · date_trunc).
    let physical = compile_query(docs, &query, role, binds)?;
    assert_select_only(&physical)?;
    // Step 8 (D21): date_trunc rewrite + quote adapt.
    dialect.emit_physical_sql(&physical)
}

pub(crate) fn parse_query(sql: &str) -> Result<Query, AppError> {
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
    Ok(*query)
}

pub(crate) fn compile_query(
    docs: &[Value],
    query: &Query,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    match query.body.as_ref() {
        SetExpr::Select(select) => compile_select(docs, select, Some(query), role, binds),
        SetExpr::Query(inner) => compile_query(docs, inner, role, binds),
        other => {
            let mut sql = compile_set_expr(docs, other, role, binds)?;
            let order_by = extract_order_by(Some(query), None, &[])?;
            if !order_by.is_empty() {
                sql.push_str(" ORDER BY ");
                sql.push_str(&order_by.join(", "));
            }
            if let Some(lim) = extract_limit(Some(query), binds)? {
                sql.push_str(" LIMIT ");
                sql.push_str(&lim);
            }
            if let Some(off) = extract_offset(Some(query), binds)? {
                sql.push_str(" OFFSET ");
                sql.push_str(&off);
            }
            Ok(sql)
        }
    }
}

pub(crate) fn compile_set_expr(
    docs: &[Value],
    expr: &SetExpr,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    match expr {
        SetExpr::Select(select) => compile_select(docs, select, None, role, binds),
        SetExpr::Query(q) => compile_query(docs, q, role, binds),
        SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
        } => {
            match op {
                SetOperator::Union | SetOperator::Except | SetOperator::Intersect => {}
                SetOperator::Minus => {}
            }
            let l = compile_set_expr(docs, left, role, binds)?;
            let r = compile_set_expr(docs, right, role, binds)?;
            let quant = match set_quantifier {
                SetQuantifier::All => " ALL",
                SetQuantifier::Distinct => " DISTINCT",
                SetQuantifier::None => "",
                other => {
                    return Err(AppError::Validation(format!(
                        "unsupported set quantifier: {other}"
                    )));
                }
            };
            Ok(format!("({l}) {op}{quant} ({r})"))
        }
        _ => Err(AppError::Validation(
            "semantic SQL must be SELECT or set operation of SELECTs".into(),
        )),
    }
}

pub(crate) fn compile_select(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SELECT must have exactly one FROM item (joins go on that item)".into(),
        ));
    }
    let tw = &select.from[0];
    if tw.joins.is_empty() {
        compile_single_cube(docs, select, query, role, binds)
    } else {
        compile_cube_join(docs, select, query, role, binds)
    }
}

pub(crate) fn compile_single_cube(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let distinct = select.distinct.is_some();
    let (cube_alias, cube_name) = relation_alias_cube(&select.from[0].relation)?;
    let classified =
        classify_projection_for_cube(select, docs, &cube_name, Some(&cube_alias), true, None)?;
    ensure_projection_mode(&classified)?;

    let (segments, where_exprs) =
        extract_where_for_cube(select, docs, &cube_name, Some(&cube_alias), true, role)?;
    let (explicit_group_by, aggregation) = extract_group_by_for_cube(
        select,
        &cube_name,
        &cube_alias,
        &classified.projections,
        &classified.dims,
    )?;
    let having = extract_having_for_cube(select, &cube_name, &cube_alias)?;
    let order_by = extract_order_by(query, Some((&cube_alias, &cube_name)), &[])?;
    let limit = extract_limit(query, binds)?;
    let offset = extract_offset(query, binds)?;

    let has_named = !classified.named_metrics.is_empty();
    let has_adhoc = !classified.adhoc_metrics.is_empty();
    let has_explicit_gb = explicit_group_by.is_some() || aggregation.is_some();
    let has_agg_order = order_by_has_aggregate_or_metric(&order_by, docs, &cube_name);

    // D23 2-mode grain determination:
    // Enter Grouped mode whenever SELECT projects metrics/aggregates, or GROUP BY / rollup is present,
    // or ORDER BY aggregates/references a metric.
    let ungrouped = !has_named && !has_adhoc && !has_explicit_gb && !has_agg_order;

    if ungrouped {
        if having.is_some() {
            return Err(AppError::Validation(
                "HAVING is not allowed in dim/fact-only ungrouped SELECT (D23)".into(),
            ));
        }
    }

    let dims = classified.dims.clone();
    let projections = classified.projections.clone();
    let (docs_owned, metrics) =
        materialize_adhoc_metrics(docs, &cube_name, &classified, &dims, &explicit_group_by)?;
    let docs_ref: &[Value] = docs_owned.as_deref().unwrap_or(docs);

    let explicit_group_by = if !ungrouped {
        match explicit_group_by {
            Some(gb) => Some(gb),
            None if !dims.is_empty() => Some(dims.clone()),
            None if has_adhoc && !has_named => Some(Vec::new()), // global aggregate
            None => None,
        }
    } else {
        explicit_group_by
    };

    let spec = CubeQuerySpec {
        projections,
        metrics,
        dimensions: dims,
        segments,
        where_exprs,
        explicit_group_by,
        aggregation,
        having,
        order_by,
        limit,
        offset,
        ungrouped,
        distinct,
    };

    compile_cube_query(docs_ref, &cube_name, &spec, role, binds)
}

pub(crate) fn order_by_has_aggregate_or_metric(order_by: &[String], docs: &[Value], cube_name: &str) -> bool {
    let metric_names: HashSet<&str> = docs
        .iter()
        .filter(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .filter_map(|d| d.get("name").and_then(|v| v.as_str()))
        .collect();
    for ob in order_by {
        if contains_aggregate_sql(ob).unwrap_or(false) {
            return true;
        }
        if let Ok(ids) = extract_idents(ob) {
            if ids.iter().any(|id| metric_names.contains(id.as_str())) {
                return true;
            }
        }
    }
    false
}


pub(crate) fn collect_cubes_from_set_expr(expr: &SetExpr, out: &mut Vec<String>) -> Result<(), AppError> {
    match expr {
        SetExpr::Select(select) => collect_cubes_from_select(select, out),
        SetExpr::Query(q) => collect_cubes_from_set_expr(q.body.as_ref(), out),
        SetExpr::SetOperation { left, right, .. } => {
            collect_cubes_from_set_expr(left, out)?;
            collect_cubes_from_set_expr(right, out)
        }
        _ => Err(AppError::Validation(
            "semantic SQL must be SELECT or set operation of SELECTs".into(),
        )),
    }
}

pub(crate) fn collect_cubes_from_select(select: &Select, out: &mut Vec<String>) -> Result<(), AppError> {
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SELECT must have exactly one FROM item".into(),
        ));
    }
    push_cube_unique(out, &relation_cube_name(&select.from[0].relation)?);
    for j in &select.from[0].joins {
        push_cube_unique(out, &relation_cube_name(&j.relation)?);
    }
    Ok(())
}

pub(crate) fn push_cube_unique(out: &mut Vec<String>, name: &str) {
    if !out.iter().any(|x| x == name) {
        out.push(name.to_string());
    }
}

pub(crate) fn relation_cube_name(rel: &TableFactor) -> Result<String, AppError> {
    Ok(relation_alias_cube(rel)?.1)
}

pub(crate) fn relation_alias_cube(rel: &TableFactor) -> Result<(String, String), AppError> {
    match rel {
        TableFactor::Table { name, alias, .. } => {
            // Flexible FROM: `cube`, `cube.physical`, or `db.schema.tbl` → first ident is the
            // cube name (LLM physical-SQL habit). Trailing parts are ignored for routing.
            let cube = name
                .0
                .first()
                .and_then(|p| p.as_ident())
                .map(|id| id.value.clone())
                .ok_or_else(|| {
                    AppError::Validation("semantic FROM must be a cube identifier".into())
                })?;
            let al = alias
                .as_ref()
                .map(|a| a.name.value.clone())
                .unwrap_or_else(|| cube.clone());
            Ok((al, cube))
        }
        _ => Err(AppError::Validation(
            "semantic FROM must be a cube identifier".into(),
        )),
    }
}

pub(crate) fn assert_same_cube_target(docs: &[Value], cubes: &[String]) -> Result<(), AppError> {
    let mut target: Option<&str> = None;
    for name in cubes {
        let cube = find_doc(docs, "cube", name)?;
        let t = cube
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("cube {name}: target missing")))?;
        match target {
            None => target = Some(t),
            Some(prev) if prev == t => {}
            Some(prev) => {
                return Err(AppError::Validation(format!(
                    "cube JOIN/UNION requires same target; {name} has {t}, others have {prev}"
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn find_doc<'a>(docs: &'a [Value], kind: &str, name: &str) -> Result<&'a Value, AppError> {
    docs.iter()
        .find(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some(kind)
                && d.get("name").and_then(|v| v.as_str()) == Some(name)
        })
        .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))
}

