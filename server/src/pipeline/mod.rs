//! Semantic SQL → physical SQL (spec 6-step pipeline entry).
//! Free physical SELECT is not supported — input must reference cube(s).
//! Same MCP `target` cubes may JOIN / UNION (·EXCEPT/INTERSECT).
//! D24: read-only ANSI SELECT (subquery / CTE / derived FROM / window / set ops).

mod clauses;
mod join;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sqlparser::ast::{
    Query, Select, SelectItem, SetExpr, SetOperator, SetQuantifier, Statement, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::compile::{
    check_role_access, compile_cube_query, expand_semantic_subqueries, CubeQuerySpec,
};
use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::expr_guard::contains_aggregate_sql;
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

/// All cube names referenced in a semantic SELECT (WITH / subquery / JOIN / UNION).
pub fn cube_names_from_semantic_sql(sql: &str) -> Result<Vec<String>, AppError> {
    let query = parse_query(sql)?;
    let mut names = Vec::new();
    collect_cubes_from_query(&query, &mut names, &HashSet::new())?;
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
    collect_cubes_from_query(&query, &mut cubes, &HashSet::new())?;
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
    compile_query_with_ctes(docs, query, role, binds, &HashSet::new())
}

fn compile_query_with_ctes(
    docs: &[Value],
    query: &Query,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    outer_ctes: &HashSet<String>,
) -> Result<String, AppError> {
    let mut cte_names = outer_ctes.clone();
    let mut with_prefix = String::new();
    if let Some(with) = &query.with {
        if with.recursive {
            return Err(AppError::Validation(
                "WITH RECURSIVE is not supported in semantic SQL".into(),
            ));
        }
        let mut parts = Vec::new();
        for cte in &with.cte_tables {
            let body = compile_query_with_ctes(docs, &cte.query, role, binds, &cte_names)?;
            let name = cte.alias.name.value.clone();
            cte_names.insert(name.clone());
            parts.push(format!("{name} AS ({body})"));
        }
        with_prefix = format!("WITH {} ", parts.join(", "));
    }

    let body = match query.body.as_ref() {
        SetExpr::Select(select) => {
            compile_select_ctx(docs, select, Some(query), &cte_names, role, binds)?
        }
        SetExpr::Query(inner) => compile_query_with_ctes(docs, inner, role, binds, &cte_names)?,
        other => {
            let mut sql = compile_set_expr_ctx(docs, other, &cte_names, role, binds)?;
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
            sql
        }
    };
    Ok(format!("{with_prefix}{body}"))
}

pub(crate) fn compile_set_expr(
    docs: &[Value],
    expr: &SetExpr,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    compile_set_expr_ctx(docs, expr, &HashSet::new(), role, binds)
}

fn compile_set_expr_ctx(
    docs: &[Value],
    expr: &SetExpr,
    cte_names: &HashSet<String>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    match expr {
        SetExpr::Select(select) => compile_select_ctx(docs, select, None, cte_names, role, binds),
        SetExpr::Query(q) => compile_query_with_ctes(docs, q, role, binds, cte_names),
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
            let l = compile_set_expr_ctx(docs, left, cte_names, role, binds)?;
            let r = compile_set_expr_ctx(docs, right, cte_names, role, binds)?;
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
    compile_select_ctx(docs, select, query, &HashSet::new(), role, binds)
}

fn compile_select_ctx(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    cte_names: &HashSet<String>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SELECT must have exactly one FROM item (joins go on that item)".into(),
        ));
    }
    let tw = &select.from[0];
    let primary = classify_from_relation(&tw.relation, cte_names)?;
    for j in &tw.joins {
        let side = classify_from_relation(&j.relation, cte_names)?;
        if matches!(primary, FromKind::Cube) != matches!(side, FromKind::Cube)
            || matches!(primary, FromKind::Cte | FromKind::Derived)
                && matches!(side, FromKind::Cte | FromKind::Derived)
        {
            // Allow cube↔cube joins only; reject mixing cubes with CTE/derived.
            if !(matches!(primary, FromKind::Cube) && matches!(side, FromKind::Cube)) {
                return Err(AppError::Validation(
                    "JOIN between cube and CTE/derived table is not supported".into(),
                ));
            }
        }
    }

    match primary {
        FromKind::Cube => {
            if tw.joins.is_empty() {
                compile_single_cube(docs, select, query, role, binds)
            } else {
                compile_cube_join(docs, select, query, role, binds)
            }
        }
        FromKind::Cte | FromKind::Derived => {
            if !tw.joins.is_empty() {
                return Err(AppError::Validation(
                    "JOIN on CTE/derived FROM is not supported".into(),
                ));
            }
            compile_passthrough_select(docs, select, query, cte_names, role, binds)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FromKind {
    Cube,
    Cte,
    Derived,
}

fn classify_from_relation(
    rel: &TableFactor,
    cte_names: &HashSet<String>,
) -> Result<FromKind, AppError> {
    match rel {
        TableFactor::Table { name, .. } => {
            if name.0.len() != 1 {
                return Err(AppError::Validation(
                    "physical schema.table FROM is not allowed; use cube name only".into(),
                ));
            }
            let n = name.0[0].to_string();
            if cte_names.contains(&n) {
                Ok(FromKind::Cte)
            } else {
                Ok(FromKind::Cube)
            }
        }
        TableFactor::Derived { .. } => Ok(FromKind::Derived),
        _ => Err(AppError::Validation(
            "semantic FROM must be a cube, CTE, or derived SELECT".into(),
        )),
    }
}

/// Outer SELECT whose FROM is a CTE or derived table: compile nested cube SELECTs, leave outer idents.
fn compile_passthrough_select(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    cte_names: &HashSet<String>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let from_sql = match &select.from[0].relation {
        TableFactor::Derived {
            subquery, alias, ..
        } => {
            let inner = compile_query_with_ctes(docs, subquery, role, binds, cte_names)?;
            let a = alias
                .as_ref()
                .map(|a| format!(" AS {}", a.name.value))
                .unwrap_or_default();
            format!("({inner}){a}")
        }
        TableFactor::Table { name, alias, .. } => {
            let n = name.0[0].to_string();
            let a = alias
                .as_ref()
                .map(|a| format!(" AS {}", a.name.value))
                .unwrap_or_default();
            format!("{n}{a}")
        }
        _ => {
            return Err(AppError::Validation(
                "passthrough FROM must be CTE or derived SELECT".into(),
            ));
        }
    };

    let distinct = if select.distinct.is_some() {
        "DISTINCT "
    } else {
        ""
    };
    let mut projs = Vec::new();
    for item in &select.projection {
        if matches!(
            item,
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _)
        ) {
            return Err(AppError::Validation(
                "SELECT * not allowed in semantic SQL".into(),
            ));
        }
        projs.push(maybe_expand_expr(&item.to_string(), docs, role, binds)?);
    }

    let mut sql = format!("SELECT {distinct}{} FROM {from_sql}", projs.join(", "));

    if let Some(sel) = &select.selection {
        let w = maybe_expand_expr(&sel.to_string(), docs, role, binds)?;
        sql.push_str(" WHERE ");
        sql.push_str(&w);
    }
    if let sqlparser::ast::GroupByExpr::Expressions(cols, _) = &select.group_by {
        if !cols.is_empty() {
            let mut parts = Vec::new();
            for c in cols {
                parts.push(maybe_expand_expr(&c.to_string(), docs, role, binds)?);
            }
            sql.push_str(" GROUP BY ");
            sql.push_str(&parts.join(", "));
        }
    }
    if let Some(having) = &select.having {
        let h = maybe_expand_expr(&having.to_string(), docs, role, binds)?;
        sql.push_str(" HAVING ");
        sql.push_str(&h);
    }

    let order_by = extract_order_by(query, None, &[])?;
    if !order_by.is_empty() {
        let mut obs = Vec::new();
        for ob in order_by {
            obs.push(maybe_expand_expr(&ob, docs, role, binds)?);
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(&obs.join(", "));
    }
    if let Some(lim) = extract_limit(query, binds)? {
        sql.push_str(" LIMIT ");
        sql.push_str(&lim);
    }
    if let Some(off) = extract_offset(query, binds)? {
        sql.push_str(" OFFSET ");
        sql.push_str(&off);
    }
    Ok(sql)
}

fn maybe_expand_expr(
    expr: &str,
    docs: &[Value],
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if expr.to_ascii_lowercase().contains("select") {
        expand_semantic_subqueries(expr, docs, role, binds)
    } else {
        Ok(expr.to_string())
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
    let has_agg_order = order_by_has_aggregate(&order_by);

    // D23 2-mode grain determination (D24: no named metrics).
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

pub(crate) fn order_by_has_aggregate(order_by: &[String]) -> bool {
    order_by
        .iter()
        .any(|ob| contains_aggregate_sql(ob).unwrap_or(false))
}

/// Collect cube table names, skipping CTE aliases and recursing into WITH / derived / expr subqueries.
fn collect_cubes_from_query(
    query: &Query,
    out: &mut Vec<String>,
    outer_ctes: &HashSet<String>,
) -> Result<(), AppError> {
    let mut cte_names = outer_ctes.clone();
    if let Some(with) = &query.with {
        for cte in &with.cte_tables {
            collect_cubes_from_query(&cte.query, out, &cte_names)?;
            cte_names.insert(cte.alias.name.value.clone());
        }
    }
    collect_cubes_from_set_expr_ctx(query.body.as_ref(), out, &cte_names)
}

pub(crate) fn collect_cubes_from_set_expr(
    expr: &SetExpr,
    out: &mut Vec<String>,
) -> Result<(), AppError> {
    collect_cubes_from_set_expr_ctx(expr, out, &HashSet::new())
}

fn collect_cubes_from_set_expr_ctx(
    expr: &SetExpr,
    out: &mut Vec<String>,
    cte_names: &HashSet<String>,
) -> Result<(), AppError> {
    match expr {
        SetExpr::Select(select) => collect_cubes_from_select_ctx(select, out, cte_names),
        SetExpr::Query(q) => collect_cubes_from_query(q, out, cte_names),
        SetExpr::SetOperation { left, right, .. } => {
            collect_cubes_from_set_expr_ctx(left, out, cte_names)?;
            collect_cubes_from_set_expr_ctx(right, out, cte_names)
        }
        _ => Err(AppError::Validation(
            "semantic SQL must be SELECT or set operation of SELECTs".into(),
        )),
    }
}

fn collect_cubes_from_select_ctx(
    select: &Select,
    out: &mut Vec<String>,
    cte_names: &HashSet<String>,
) -> Result<(), AppError> {
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SELECT must have exactly one FROM item".into(),
        ));
    }
    collect_cubes_from_relation(&select.from[0].relation, out, cte_names)?;
    for j in &select.from[0].joins {
        collect_cubes_from_relation(&j.relation, out, cte_names)?;
    }
    Ok(())
}

fn collect_cubes_from_relation(
    rel: &TableFactor,
    out: &mut Vec<String>,
    cte_names: &HashSet<String>,
) -> Result<(), AppError> {
    match rel {
        TableFactor::Table { name, .. } => {
            if name.0.len() != 1 {
                return Err(AppError::Validation(
                    "physical schema.table FROM is not allowed; use cube name only".into(),
                ));
            }
            let n = name.0[0].to_string();
            if !cte_names.contains(&n) {
                push_cube_unique(out, &n);
            }
            Ok(())
        }
        TableFactor::Derived { subquery, .. } => collect_cubes_from_query(subquery, out, cte_names),
        _ => Err(AppError::Validation(
            "semantic FROM must be a cube, CTE, or derived SELECT".into(),
        )),
    }
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
            if name.0.len() != 1 {
                return Err(AppError::Validation(
                    "physical schema.table FROM is not allowed; use cube name only".into(),
                ));
            }
            let cube = name.0[0].to_string();
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
