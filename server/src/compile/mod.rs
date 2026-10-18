//! table/metric/cube → physical SQL (summary views, join/union pruning, filter pushdown).

mod rewrite;
mod view_prune;

#[cfg(test)]
mod tests;

pub use rewrite::{resolve_dim_as_code, wrap_as_code_joins};
pub use view_prune::{
    compile_view_from, compile_view_from_opts, resolve_view_for_cube, resolve_view_for_cube_opts,
};

use rewrite::{
    collect_member_view_cols, collect_order_by_req_cols, cube_member_to_physical_map, expr_references_any_metric, find_cube_member, find_dimension,
    inject_filter_into_aggregates, is_ident_char, member_encrypted, output_col_expr,
    rewrite_composite_metric_projection, rewrite_having_expr, rewrite_member_or_view_idents,
    rewrite_metric_expression, rewrite_order_by_clauses, rewrite_view_idents,
    validate_cube_member_expressions, wrap_as_code_joins_with_aliases,
};
use view_prune::pin_view_mappings_to_included;

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::ast_select::{SelectClauses, assemble_select_with_clauses};
use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::expr_guard::{
    assert_filter_expr, assert_having_expr, assert_idents_known, assert_metadata_metric_expr,
    assert_metric_expr, assert_scalar_expr, assert_segment_filter_expr, contains_window_sql,
    extract_idents,
};
use crate::select_guard::assert_select_only;

/// Quote a simple SQL identifier in **canonical ANSI** form (`"…"`).
/// Delegates to [`DialectProfile::Postgres`]; emit adapts via other profiles (D21).
pub fn quote_ident(name: &str) -> Result<String, AppError> {
    DialectProfile::Postgres.quote_ident(name)
}

/// Enforce RBAC `role` access on any semantic entity (`cube`, `table`, `metric`, `segment`, `dimension`, `fact`).
pub fn check_role_access(role_val: Option<&Value>, caller: Option<&str>) -> Result<(), AppError> {
    let Some(role_val) = role_val else {
        return Ok(());
    };
    if role_val.as_str() == Some("all") || role_val.is_null() {
        return Ok(());
    }
    if let Value::Array(arr) = role_val {
        if arr.iter().any(|v| v.as_str() == Some("all")) {
            return Ok(());
        }
    }
    let Some(caller) = caller else {
        return Err(AppError::Forbidden(
            "caller_role required for restricted resource".into(),
        ));
    };
    match role_val {
        Value::String(s) if s == "all" || s == caller => Ok(()),
        Value::Array(arr) if arr.iter().any(|v| v.as_str() == Some(caller)) => Ok(()),
        _ => Err(AppError::Forbidden(format!(
            "role {caller} cannot access this resource"
        ))),
    }
}

/// Physical FROM clause for a table registration document.
pub fn physical_from(table_doc: &Value) -> Result<String, AppError> {
    let schema = table_doc
        .get("schema")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("table.schema required".into()))?;
    let table = table_doc
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("table.table required".into()))?;
    Ok(format!(
        "{}.{}",
        quote_ident(schema)?,
        quote_ident(table)?
    ))
}

/// Compile `SELECT <cols> FROM physical` with AND-ed table filters (ARCHITECTURE §1).
pub fn compile_table_select(
    table_doc: &Value,
    columns: &[&str],
    role: Option<&str>,
) -> Result<String, AppError> {
    compile_table_select_with_binds(table_doc, columns, role, None)
}

pub fn compile_table_select_with_binds(
    table_doc: &Value,
    columns: &[&str],
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if table_doc.get("kind").and_then(|v| v.as_str()) != Some("table") {
        return Err(AppError::Validation("expected table document".into()));
    }
    check_role_access(table_doc.get("role"), role)?;
    let from = physical_from(table_doc)?;
    let cols = if columns.is_empty() {
        "*".to_string()
    } else {
        columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ")
    };
    let filters = collect_table_filters(table_doc, role, binds)?;
    let mut sql = format!("SELECT {cols} FROM {from}");
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    assert_select_only(&sql)?;
    Ok(sql)
}

pub(crate) fn collect_table_filters(
    table_doc: &Value,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<Vec<String>, AppError> {
    let known = table_column_names(table_doc);
    let mut filters = Vec::new();
    if let Some(expr) = filter_expr(table_doc.get("scope_filter")) {
        validate_table_filter(expr, &known)?;
        let resolved = apply_binds(expr, binds);
        filters.push(format!("({resolved})"));
    }
    if let Some(expr) = filter_expr(table_doc.get("default_filter")) {
        validate_table_filter(expr, &known)?;
        let resolved = apply_binds(expr, binds);
        filters.push(format!("({resolved})"));
    }
    if let Some(role) = role {
        if let Some(pfs) = table_doc.get("permission_filters").and_then(|v| v.as_array()) {
            for pf in pfs {
                if pf.get("role").and_then(|v| v.as_str()) == Some(role) {
                    if let Some(expr) = pf.get("expression").and_then(|v| v.as_str()) {
                        validate_table_filter(expr, &known)?;
                        let resolved = apply_binds(expr, binds);
                        filters.push(format!("({resolved})"));
                    }
                }
            }
        }
    }
    Ok(filters)
}

pub(crate) fn apply_binds(expr: &str, binds: Option<&HashMap<String, String>>) -> String {
    match binds {
        Some(b) if !b.is_empty() => crate::date_range::substitute_binds(expr, b),
        _ => expr.to_string(),
    }
}

pub(crate) fn table_column_names(table_doc: &Value) -> HashSet<String> {
    table_doc
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|cols| {
            cols.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn cube_member_names(cube: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    out.insert(n.to_string());
                }
            }
        }
    }
    out
}

pub(crate) fn view_column_name_set(view: &Value) -> HashSet<String> {
    view.get("columns")
        .and_then(|v| v.as_array())
        .map(|cols| {
            cols.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn validate_table_filter(expr: &str, known: &HashSet<String>) -> Result<(), AppError> {
    assert_filter_expr(expr)?;
    assert_idents_known(expr, known)
}

pub(crate) fn filter_expr(v: Option<&Value>) -> Option<&str> {
    v.and_then(|f| {
        if f.is_null() {
            None
        } else {
            f.get("expression").and_then(|e| e.as_str())
        }
    })
}

pub(crate) fn docs_by_kind<'a>(
    docs: &'a [Value],
    kind: &'a str,
) -> impl Iterator<Item = &'a Value> + 'a {
    docs.iter()
        .filter(move |d| d.get("kind").and_then(|v| v.as_str()) == Some(kind))
}

pub(crate) fn find_doc<'a>(
    docs: &'a [Value],
    kind: &'a str,
    name: &str,
) -> Result<&'a Value, AppError> {
    docs_by_kind(docs, kind)
        .find(|d| d.get("name").and_then(|v| v.as_str()) == Some(name))
        .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))
}

/// Compile a join/union/select view into a FROM clause.
/// Table filters are pushed into derived-table subqueries (spec Step 6).
/// When `used_view_cols` is set, join graph is pruned to a Steiner approx (Step 4).

#[derive(Debug, Clone)]
pub struct CubeProjection {
    pub member: String,
    pub alias: String,
}

#[derive(Debug, Clone, Default)]
pub struct CubeQuerySpec {
    pub projections: Vec<CubeProjection>,
    pub metrics: Vec<String>,
    pub dimensions: Vec<String>,
    pub segments: Vec<String>,
    pub where_exprs: Vec<String>,
    pub explicit_group_by: Option<Vec<String>>,
    pub aggregation: Option<String>,
    pub having: Option<String>,
    pub order_by: Vec<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
    /// D23 Ungrouped (detail) mode: dim/fact/scalar-only row-level SELECT (no metrics, no GROUP BY/HAVING).
    pub ungrouped: bool,
    /// Preserve `SELECT DISTINCT`.
    pub distinct: bool,
}

/// Compile cube metric (+ optional segments) into physical SELECT.
pub fn compile_metric_select(
    docs: &[Value],
    metric_name: &str,
    segment_names: &[&str],
    role: Option<&str>,
) -> Result<String, AppError> {
    compile_metric_select_with_binds(docs, metric_name, segment_names, role, None)
}

pub fn compile_metric_select_with_binds(
    docs: &[Value],
    metric_name: &str,
    segment_names: &[&str],
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let metric = find_doc(docs, "metric", metric_name)?;
    let cube_name = metric
        .get("cube")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("metric.cube required".into()))?;
    let spec = CubeQuerySpec {
        projections: Vec::new(),
        metrics: vec![metric_name.to_string()],
        dimensions: Vec::new(),
        segments: segment_names.iter().map(|s| (*s).to_string()).collect(),
        where_exprs: Vec::new(),
        explicit_group_by: None,
        aggregation: None,
        having: None,
        order_by: Vec::new(),
        limit: None,
        offset: None,
        ungrouped: false,
        distinct: false,
    };
    compile_cube_query(docs, cube_name, &spec, role, binds)
}

/// Validate a dimension/fact member name or a non-aggregate scalar expression over cube members.
pub(crate) fn validate_member_or_scalar_expr(
    cube: &Value,
    members: &HashSet<String>,
    expr: &str,
    role: Option<&str>,
) -> Result<(), AppError> {
    if members.contains(expr) {
        return check_member_role(cube, expr, role);
    }
    let trimmed = expr.trim();
    if !trimmed.is_empty() && trimmed.chars().all(is_ident_char) {
        return Err(AppError::Validation(format!(
            "unknown semantic member: {trimmed}"
        )));
    }
    assert_scalar_expr(expr)?;
    assert_idents_known(expr, members)?;
    for id in extract_idents(expr)? {
        check_member_role(cube, &id, role)?;
        if member_encrypted(cube, &id) {
            return Err(AppError::Validation(format!(
                "encrypted member {id} cannot be used in scalar expression"
            )));
        }
    }
    Ok(())
}

pub(crate) fn extract_member_idents(expr: &str, members: &HashSet<String>) -> Result<Vec<String>, AppError> {
    if members.contains(expr) {
        return Ok(vec![expr.to_string()]);
    }
    Ok(extract_idents(expr)?.into_iter().collect())
}

/// D23 Ungrouped (detail) mode: project dimensions/facts/scalars as row-level columns (no aggregation).
pub(crate) fn compile_cube_query_ungrouped(
    docs: &[Value],
    cube_name: &str,
    spec: &CubeQuerySpec,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if !spec.metrics.is_empty() {
        return Err(AppError::Validation(
            "ungrouped SELECT cannot include metrics (D23)".into(),
        ));
    }
    if spec.dimensions.is_empty() && spec.projections.is_empty() {
        return Err(AppError::Validation(
            "ungrouped SELECT must project at least one dimension or fact (D23)".into(),
        ));
    }
    if spec.having.is_some() {
        return Err(AppError::Validation(
            "HAVING is not allowed in dim/fact-only ungrouped SELECT (D23)".into(),
        ));
    }
    if spec
        .explicit_group_by
        .as_ref()
        .is_some_and(|gb| !gb.is_empty())
        || spec.aggregation.is_some()
    {
        return Err(AppError::Validation(
            "GROUP BY is not allowed in dim/fact-only ungrouped SELECT (D23)".into(),
        ));
    }

    let cube = find_doc(docs, "cube", cube_name)?;
    check_role_access(cube.get("role"), role)?;
    let members = cube_member_names(cube);

    let proj_members: Vec<(String, String)> = if spec.projections.is_empty() {
        spec.dimensions
            .iter()
            .map(|d| (d.clone(), d.clone()))
            .collect()
    } else {
        spec.projections
            .iter()
            .map(|p| (p.member.clone(), p.alias.clone()))
            .collect()
    };

    for (member, _) in &proj_members {
        validate_member_or_scalar_expr(cube, &members, member, role)?;
    }

    let proj_aliases: HashSet<String> = proj_members.iter().map(|(_, a)| a.clone()).collect();

    let mut req_cols: HashSet<String> = HashSet::new();
    for (member, _) in &proj_members {
        collect_member_view_cols(cube, member, &mut req_cols);
    }
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if seg.get("cube").and_then(|v| v.as_str()) != Some(cube_name) {
            return Err(AppError::Validation(format!(
                "segment {seg_name} not on cube {cube_name}"
            )));
        }
        check_role_access(seg.get("role"), role)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            assert_segment_filter_expr(expr)?;
            assert_idents_known(expr, &members)?;
            for id in extract_idents(expr)? {
                if member_encrypted(cube, &id) {
                    return Err(AppError::Validation(format!(
                        "encrypted member {id} cannot be used in segment filter"
                    )));
                }
            }
            collect_member_view_cols(cube, expr, &mut req_cols);
        }
    }
    for w_expr in &spec.where_exprs {
        assert_segment_filter_expr(w_expr)?;
        assert_idents_known(w_expr, &members)?;
        for id in extract_idents(w_expr)? {
            check_member_role(cube, &id, role)?;
            if member_encrypted(cube, &id) {
                return Err(AppError::Validation(format!(
                    "encrypted member {id} cannot be used in WHERE filter"
                )));
            }
        }
        collect_member_view_cols(cube, w_expr, &mut req_cols);
    }
    collect_order_by_req_cols(
        cube,
        &members,
        &proj_aliases,
        &HashSet::new(),
        &spec.order_by,
        false,
        role,
        &mut req_cols,
    )?;

    let mut query_preds: Vec<String> = Vec::new();
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            query_preds.push(expr.to_string());
        }
    }
    for w_expr in &spec.where_exprs {
        query_preds.push(w_expr.clone());
    }

    // D23 safety guard: non-DISTINCT ungrouped detail queries bypass summary: true views.
    let view = resolve_view_for_cube_opts(docs, cube, &req_cols, false, spec.distinct)?;
    let combine = view.get("combine").and_then(|v| v.as_str()).unwrap_or("");
    let set_alias = matches!(combine, "union" | "minus").then_some("_sem_uv");
    let mut from =
        compile_view_from_opts(docs, &view, role, Some(&req_cols), &query_preds, binds)?;
    if let Some(a) = set_alias {
        from = format!("{from} AS {}", quote_ident(a)?);
    }
    let view = pin_view_mappings_to_included(&view, &req_cols)?;

    let view_cols = view_column_name_set(&view);
    validate_cube_member_expressions(cube, &view_cols)?;

    let mut projections: Vec<(String, String)> = Vec::new();
    let mut as_code_dims: Vec<(String, String)> = Vec::new();
    for (member, alias) in &proj_members {
        let expr = compile_dimension_expr(cube, &view, member, set_alias)?;
        projections.push((expr, alias.clone()));
        if find_dimension(cube, member).is_some() && !as_code_dims.iter().any(|(d, _)| d == member)
        {
            as_code_dims.push((member.clone(), alias.clone()));
        }
    }

    let mut filters = Vec::new();
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            filters.push(rewrite_member_or_view_idents(
                expr, docs, cube, &view, set_alias, role, binds,
            )?);
        }
    }
    for w_expr in &spec.where_exprs {
        filters.push(rewrite_member_or_view_idents(
            w_expr, docs, cube, &view, set_alias, role, binds,
        )?);
    }

    let (rewritten_order_by, order_by_outer_compatible) = rewrite_order_by_clauses(
        &spec.order_by,
        &projections,
        &HashMap::new(),
        cube,
        &view,
        set_alias,
        binds,
        false,
    )?;

    let has_as_code = as_code_dims.iter().any(|(dim, _)| {
        !member_encrypted(cube, dim)
            && resolve_dim_as_code(docs, cube, &view, dim)
                .ok()
                .flatten()
                .is_some()
    });
    let defer_to_outer = has_as_code && order_by_outer_compatible;

    let inner_clauses = SelectClauses {
        distinct: spec.distinct,
        aggregation: Some("none".into()),
        having: None,
        order_by: if defer_to_outer {
            Vec::new()
        } else {
            rewritten_order_by.clone()
        },
        limit: if defer_to_outer {
            None
        } else {
            spec.limit.clone()
        },
        offset: if defer_to_outer {
            None
        } else {
            spec.offset.clone()
        },
    };

    let sql = assemble_select_with_clauses(&projections, &from, &filters, &[], &inner_clauses)?;
    let mut sql =
        wrap_as_code_joins_with_aliases(docs, cube, &view, &as_code_dims, &sql, role, binds)?;
    if defer_to_outer
        && (!rewritten_order_by.is_empty() || spec.limit.is_some() || spec.offset.is_some())
    {
        if !rewritten_order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&rewritten_order_by.join(", "));
        }
        if let Some(lim) = &spec.limit {
            sql.push_str(" LIMIT ");
            sql.push_str(lim.trim());
        }
        if let Some(off) = &spec.offset {
            sql.push_str(" OFFSET ");
            sql.push_str(off.trim());
        }
        let dialect = sqlparser::dialect::GenericDialect {};
        let stmts = sqlparser::parser::Parser::parse_sql(&dialect, &sql)
            .map_err(|e| AppError::Validation(format!("outer clauses parse: {e}")))?;
        if stmts.len() != 1 {
            return Err(AppError::Validation(
                "outer clauses: expected single statement".into(),
            ));
        }
        sql = stmts[0].to_string();
    }
    assert_select_only(&sql)?;
    Ok(sql)
}

pub fn compile_cube_query(
    docs: &[Value],
    cube_name: &str,
    spec: &CubeQuerySpec,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if spec.ungrouped {
        return compile_cube_query_ungrouped(docs, cube_name, spec, role, binds);
    }
    let cube = find_doc(docs, "cube", cube_name)?;
    check_role_access(cube.get("role"), role)?;

    let members = cube_member_names(cube);
    let mut all_metrics = spec.metrics.clone();
    // Discover any named metrics on this cube referenced in HAVING or ORDER BY.
    let mut extra_clause_idents = Vec::new();
    if let Some(h) = &spec.having {
        if let Ok(ids) = extract_idents(h) {
            extra_clause_idents.extend(ids);
        }
    }
    for ob in &spec.order_by {
        if let Ok(ids) = extract_idents(ob) {
            extra_clause_idents.extend(ids);
        }
    }
    for id in extra_clause_idents {
        if !all_metrics.contains(&id) && !members.contains(&id) {
            if let Ok(m_doc) = find_doc(docs, "metric", &id) {
                if m_doc.get("cube").and_then(|v| v.as_str()) == Some(cube_name) {
                    all_metrics.push(id);
                }
            }
        }
    }

    let has_group_context = spec
        .explicit_group_by
        .as_ref()
        .is_some_and(|gb| !gb.is_empty())
        || !spec.dimensions.is_empty()
        || spec.having.is_some()
        || spec.aggregation.is_some();
    if all_metrics.is_empty() && !has_group_context {
        return Err(AppError::Validation(
            "semantic SELECT must project at least one metric".into(),
        ));
    }

    let mut metric_docs = Vec::with_capacity(all_metrics.len());
    let mut named_metric_gbs: Vec<(&str, Vec<String>)> = Vec::new();
    let mut has_adhoc_metrics = false;
    let mut is_semi = false;
    let mut agg_mode: Option<String> = spec.aggregation.clone();

    for m_name in &all_metrics {
        let m_doc = find_doc(docs, "metric", m_name)?;
        if m_doc.get("cube").and_then(|v| v.as_str()) != Some(cube_name) {
            return Err(AppError::Validation(format!(
                "metric {m_name} is not on cube {cube_name}"
            )));
        }
        check_role_access(m_doc.get("role"), role)?;
        let m_expr = m_doc
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("metric.expression required".into()))?;
        if m_name.starts_with("__adhoc_") {
            assert_metric_expr(m_expr)?;
        } else {
            assert_metadata_metric_expr(m_expr)?;
        }
        assert_idents_known(m_expr, &members)?;
        for id in extract_idents(m_expr)? {
            if member_encrypted(cube, &id) {
                return Err(AppError::Validation(format!(
                    "encrypted member {id} cannot be aggregated in metric {m_name}"
                )));
            }
            // D23: ephemeral ad-hoc metrics use role=all — enforce member RBAC on idents.
            if m_name.starts_with("__adhoc_") {
                check_member_role(cube, &id, role)?;
            }
        }
        if let Some(f_expr) = filter_expr(m_doc.get("filter")) {
            assert_segment_filter_expr(f_expr)?;
            assert_idents_known(f_expr, &members)?;
            for id in extract_idents(f_expr)? {
                if member_encrypted(cube, &id) {
                    return Err(AppError::Validation(format!(
                        "encrypted member {id} cannot be used in metric filter"
                    )));
                }
            }
        }
        let lower = m_expr.to_ascii_lowercase();
        if lower.contains("count(distinct") || lower.contains("avg(") || lower.contains("median(") {
            is_semi = true;
        }
        if agg_mode.is_none() {
            agg_mode = m_doc
                .get("aggregation")
                .and_then(|v| v.as_str())
                .map(str::to_string);
        }
        if m_name.starts_with("__adhoc_") {
            has_adhoc_metrics = true;
        } else {
            let m_agg = m_doc
                .get("aggregation")
                .and_then(|v| v.as_str())
                .unwrap_or("group");
            if m_agg == "group" {
                let mut m_gb = Vec::new();
                if let Some(gb_arr) = m_doc.get("group_by").and_then(|v| v.as_array()) {
                    for g in gb_arr {
                        if let Some(gs) = g.as_str() {
                            m_gb.push(gs.to_string());
                        }
                    }
                }
                named_metric_gbs.push((m_name.as_str(), m_gb));
            }
        }
        metric_docs.push((m_name.as_str(), m_doc, m_expr));
    }

    // Validate that every active dimension is allowed by each named group metric's own group_by.
    let check_dims_against_named_metrics = |dims: &[String]| -> Result<(), AppError> {
        for d in dims {
            if contains_window_sql(d)? {
                return Err(AppError::Validation(
                    "window functions are not allowed in GROUP BY".into(),
                ));
            }
            validate_member_or_scalar_expr(cube, &members, d, role)?;
            let dim_ids = extract_member_idents(d, &members)?;
            for (_, m_gb) in &named_metric_gbs {
                for id in &dim_ids {
                    if !m_gb.iter().any(|g| g == id) {
                        return Err(AppError::Validation(format!(
                            "dimension {id} not in metric group_by"
                        )));
                    }
                }
            }
        }
        Ok(())
    };

    // Determine effective group_by dimensions.
    let active_gb: Vec<String> = if let Some(explicit) = &spec.explicit_group_by {
        check_dims_against_named_metrics(explicit)?;
        explicit.clone()
    } else if !spec.dimensions.is_empty() {
        check_dims_against_named_metrics(&spec.dimensions)?;
        spec.dimensions.clone()
    } else if !named_metric_gbs.is_empty() && !has_adhoc_metrics {
        let first_gb = &named_metric_gbs[0].1;
        let common_gb: Vec<String> = first_gb
            .iter()
            .filter(|d| {
                named_metric_gbs[1..]
                    .iter()
                    .all(|(_, gb)| gb.iter().any(|x| x == *d))
            })
            .cloned()
            .collect();
        for d in &common_gb {
            check_member_role(cube, d, role)?;
        }
        common_gb
    } else {
        Vec::new()
    };

    for d in &spec.dimensions {
        validate_member_or_scalar_expr(cube, &members, d, role)?;
    }

    let metric_name_set: HashSet<String> = all_metrics.iter().cloned().collect();
    for p in &spec.projections {
        if metric_name_set.contains(&p.member) {
            continue;
        }
        let refs_metric = extract_idents(&p.member)
            .map(|ids| ids.iter().any(|id| metric_name_set.contains(id)))
            .unwrap_or(false);
        if refs_metric {
            for id in extract_idents(&p.member)? {
                if members.contains(&id) {
                    check_member_role(cube, &id, role)?;
                    if member_encrypted(cube, &id) {
                        return Err(AppError::Validation(format!(
                            "encrypted member {id} cannot be used in expression"
                        )));
                    }
                }
            }
        } else {
            validate_member_or_scalar_expr(cube, &members, &p.member, role)?;
        }
    }
    let mut proj_aliases: HashSet<String> = spec.projections.iter().map(|p| p.alias.clone()).collect();
    for gname in &active_gb {
        proj_aliases.insert(gname.clone());
    }
    for mname in &spec.metrics {
        proj_aliases.insert(mname.clone());
    }

    let mut req_cols: HashSet<String> = HashSet::new();
    for gname in &active_gb {
        collect_member_view_cols(cube, gname, &mut req_cols);
    }
    for d in &spec.dimensions {
        collect_member_view_cols(cube, d, &mut req_cols);
    }
    for p in &spec.projections {
        if !metric_name_set.contains(&p.member) {
            collect_member_view_cols(cube, &p.member, &mut req_cols);
        }
    }
    for (_, m_doc, m_expr) in &metric_docs {
        collect_member_view_cols(cube, m_expr, &mut req_cols);
        if let Some(expr) = filter_expr(m_doc.get("filter")) {
            collect_member_view_cols(cube, expr, &mut req_cols);
        }
    }
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if seg.get("cube").and_then(|v| v.as_str()) != Some(cube_name) {
            return Err(AppError::Validation(format!(
                "segment {seg_name} not on cube {cube_name}"
            )));
        }
        check_role_access(seg.get("role"), role)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            assert_segment_filter_expr(expr)?;
            assert_idents_known(expr, &members)?;
            for id in extract_idents(expr)? {
                if member_encrypted(cube, &id) {
                    return Err(AppError::Validation(format!(
                        "encrypted member {id} cannot be used in segment filter"
                    )));
                }
            }
            collect_member_view_cols(cube, expr, &mut req_cols);
        }
    }
    for w_expr in &spec.where_exprs {
        assert_segment_filter_expr(w_expr)?;
        assert_idents_known(w_expr, &members)?;
        for id in extract_idents(w_expr)? {
            check_member_role(cube, &id, role)?;
            if member_encrypted(cube, &id) {
                return Err(AppError::Validation(format!(
                    "encrypted member {id} cannot be used in WHERE filter"
                )));
            }
        }
        collect_member_view_cols(cube, w_expr, &mut req_cols);
    }
    if let Some(h_expr) = &spec.having {
        assert_having_expr(h_expr)?;
        for id in extract_idents(h_expr)? {
            if members.contains(&id) {
                check_member_role(cube, &id, role)?;
                if member_encrypted(cube, &id) {
                    return Err(AppError::Validation(format!(
                        "encrypted member {id} cannot be used in HAVING filter"
                    )));
                }
                collect_member_view_cols(cube, &id, &mut req_cols);
            } else if !metric_name_set.contains(&id) && !proj_aliases.contains(&id) {
                return Err(AppError::Validation(format!(
                    "unknown identifier in HAVING: {id}"
                )));
            }
        }
    }
    collect_order_by_req_cols(
        cube,
        &members,
        &proj_aliases,
        &metric_name_set,
        &spec.order_by,
        true,
        role,
        &mut req_cols,
    )?;

    // Decide whether metric filters can be shared in outer WHERE or must be injected via CASE WHEN.
    let first_m_filter = metric_docs
        .first()
        .and_then(|(_, m_doc, _)| filter_expr(m_doc.get("filter")));
    let shared_metric_filter = !spec.metrics.is_empty()
        && all_metrics.len() == spec.metrics.len()
        && metric_docs
            .iter()
            .all(|(_, m_doc, _)| filter_expr(m_doc.get("filter")) == first_m_filter);

    let mut query_preds: Vec<String> = Vec::new();
    if shared_metric_filter {
        if let Some(expr) = first_m_filter {
            query_preds.push(expr.to_string());
        }
    }
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            query_preds.push(expr.to_string());
        }
    }
    for w_expr in &spec.where_exprs {
        query_preds.push(w_expr.clone());
    }

    let view = resolve_view_for_cube(docs, cube, &req_cols, is_semi)?;
    let combine = view.get("combine").and_then(|v| v.as_str()).unwrap_or("");
    let set_alias = matches!(combine, "union" | "minus").then_some("_sem_uv");
    let mut from =
        compile_view_from_opts(docs, &view, role, Some(&req_cols), &query_preds, binds)?;
    if let Some(a) = set_alias {
        from = format!("{from} AS {}", quote_ident(a)?);
    }
    let view = pin_view_mappings_to_included(&view, &req_cols)?;

    let view_cols = view_column_name_set(&view);
    validate_cube_member_expressions(cube, &view_cols)?;

    let mut rewritten_metrics: HashMap<String, String> = HashMap::new();
    for (m_name, m_doc, m_expr) in &metric_docs {
        let mut rewritten =
            rewrite_metric_expression(m_expr, docs, cube, &view, set_alias, role, binds)?;
        if !shared_metric_filter {
            if let Some(f_expr) = filter_expr(m_doc.get("filter")) {
                let f_rewritten =
                    rewrite_member_or_view_idents(f_expr, docs, cube, &view, set_alias, role, binds)?;
                rewritten = inject_filter_into_aggregates(&rewritten, &f_rewritten)?;
            }
        }
        rewritten_metrics.insert((*m_name).to_string(), rewritten);
    }

    let mut projections: Vec<(String, String)> = Vec::new();
    let mut as_code_dims: Vec<(String, String)> = Vec::new(); // (dim_name, projected_alias)

    if spec.projections.is_empty() {
        for gname in &active_gb {
            let expr = compile_dimension_expr(cube, &view, gname, set_alias)?;
            projections.push((expr, gname.clone()));
            if find_dimension(cube, gname).is_some() {
                as_code_dims.push((gname.clone(), gname.clone()));
            }
        }
        for m_name in &spec.metrics {
            let expr = rewritten_metrics.get(m_name).unwrap().clone();
            projections.push((expr, m_name.clone()));
        }
    } else {
        // If the user only wrote metrics in SELECT (no explicit dimensions), keep active_gb projected first
        // so group_by columns and as_code dictionary values are available in the result set.
        if spec.dimensions.is_empty() {
            for gname in &active_gb {
                let expr = compile_dimension_expr(cube, &view, gname, set_alias)?;
                projections.push((expr, gname.clone()));
                if find_dimension(cube, gname).is_some() {
                    as_code_dims.push((gname.clone(), gname.clone()));
                }
            }
        }
        for p in &spec.projections {
            if let Some(m_expr) = rewritten_metrics.get(&p.member) {
                projections.push((m_expr.clone(), p.alias.clone()));
            } else if expr_references_any_metric(&p.member, &rewritten_metrics) {
                let expr = rewrite_composite_metric_projection(
                    &p.member,
                    &rewritten_metrics,
                    cube,
                    &view,
                    set_alias,
                )?;
                projections.push((expr, p.alias.clone()));
            } else {
                let expr = compile_dimension_expr(cube, &view, &p.member, set_alias)?;
                projections.push((expr, p.alias.clone()));
                if find_dimension(cube, &p.member).is_some()
                    && !as_code_dims.iter().any(|(d, _)| d == &p.member)
                {
                    as_code_dims.push((p.member.clone(), p.alias.clone()));
                }
            }
        }
    }

    // Outer WHERE: shared metric filter (if applicable) + segments + user ad-hoc WHERE predicates.
    let mut filters = Vec::new();
    if shared_metric_filter {
        if let Some(expr) = first_m_filter {
            filters.push(rewrite_member_or_view_idents(
                expr, docs, cube, &view, set_alias, role, binds,
            )?);
        }
    }
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            filters.push(rewrite_member_or_view_idents(
                expr, docs, cube, &view, set_alias, role, binds,
            )?);
        }
    }
    for w_expr in &spec.where_exprs {
        filters.push(rewrite_member_or_view_idents(
            w_expr, docs, cube, &view, set_alias, role, binds,
        )?);
    }

    let mut gb = Vec::new();
    for gname in &active_gb {
        gb.push(compile_dimension_group_expr(cube, &view, gname, set_alias)?);
    }

    let has_as_code = as_code_dims.iter().any(|(dim, _)| {
        !member_encrypted(cube, dim)
            && resolve_dim_as_code(docs, cube, &view, dim)
                .ok()
                .flatten()
                .is_some()
    });

    let rewritten_having = if let Some(h) = &spec.having {
        Some(rewrite_having_expr(
            h,
            &projections,
            &rewritten_metrics,
            cube,
            &view,
            set_alias,
            binds,
        )?)
    } else {
        None
    };

    let (rewritten_order_by, order_by_outer_compatible) = rewrite_order_by_clauses(
        &spec.order_by,
        &projections,
        &rewritten_metrics,
        cube,
        &view,
        set_alias,
        binds,
        true,
    )?;
    let defer_to_outer = has_as_code && order_by_outer_compatible;

    let effective_agg_mode = if gb.is_empty() {
        Some("none".into())
    } else {
        agg_mode
    };

    let inner_clauses = SelectClauses {
        distinct: spec.distinct,
        aggregation: effective_agg_mode,
        having: rewritten_having,
        order_by: if defer_to_outer {
            Vec::new()
        } else {
            rewritten_order_by.clone()
        },
        limit: if defer_to_outer {
            None
        } else {
            spec.limit.clone()
        },
        offset: if defer_to_outer {
            None
        } else {
            spec.offset.clone()
        },
    };

    let sql = assemble_select_with_clauses(&projections, &from, &filters, &gb, &inner_clauses)?;
    // Step 7: after aggregation, LEFT JOIN code dictionaries for dims with as_code.
    let mut sql =
        wrap_as_code_joins_with_aliases(docs, cube, &view, &as_code_dims, &sql, role, binds)?;
    if defer_to_outer
        && (!rewritten_order_by.is_empty() || spec.limit.is_some() || spec.offset.is_some())
    {
        if !rewritten_order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&rewritten_order_by.join(", "));
        }
        if let Some(lim) = &spec.limit {
            sql.push_str(" LIMIT ");
            sql.push_str(lim.trim());
        }
        if let Some(off) = &spec.offset {
            sql.push_str(" OFFSET ");
            sql.push_str(off.trim());
        }
        let dialect = sqlparser::dialect::GenericDialect {};
        let stmts = sqlparser::parser::Parser::parse_sql(&dialect, &sql)
            .map_err(|e| AppError::Validation(format!("outer clauses parse: {e}")))?;
        if stmts.len() != 1 {
            return Err(AppError::Validation(
                "outer clauses: expected single statement".into(),
            ));
        }
        sql = stmts[0].to_string();
    }
    assert_select_only(&sql)?;
    Ok(sql)
}

pub(crate) fn check_member_role(cube: &Value, name: &str, role: Option<&str>) -> Result<(), AppError> {
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if item.get("name").and_then(|v| v.as_str()) == Some(name) {
                    return check_role_access(item.get("role"), role);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn compile_dimension_expr(
    cube: &Value,
    view: &Value,
    dim_name: &str,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    if member_encrypted(cube, dim_name) {
        return Ok("NULL".to_string());
    }
    compile_dimension_group_expr(cube, view, dim_name, set_alias)
}

pub(crate) fn compile_dimension_group_expr(
    cube: &Value,
    view: &Value,
    dim_name: &str,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    if let Some(member) = find_cube_member(cube, dim_name) {
        if let Some(expr) = member.get("expression").and_then(|v| v.as_str()) {
            return rewrite_view_idents(expr, view, set_alias);
        }
        if let Some(col) = member.get("column").and_then(|v| v.as_str()) {
            return output_col_expr(view, col, set_alias);
        }
    }
    let trimmed = dim_name.trim();
    if !trimmed.is_empty() && trimmed.chars().all(is_ident_char) {
        return output_col_expr(view, trimmed, set_alias);
    }
    let map = cube_member_to_physical_map(cube, view, set_alias)?;
    crate::ast_rewrite::rewrite_idents(dim_name, &map)
}

