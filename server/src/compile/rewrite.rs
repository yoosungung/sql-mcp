//! Step 5–7: cube member→physical map, semantic subquery expand, HAVING/ORDER BY rewrite, filter inject, as_code JOIN wrap.
use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::expr_guard::{
    assert_filter_expr, assert_having_expr, assert_idents_known, assert_metric_expr,
    assert_scalar_expr, extract_idents,
};

use super::{
    apply_binds, check_member_role, check_role_access, collect_table_filters, find_doc,
    physical_from, quote_ident, table_column_names, view_column_name_set,
};
use super::view_prune::view_physical_expr;

/// Step 7: wrap aggregated SELECT with LEFT JOINs to as_code dictionaries.
pub fn wrap_as_code_joins(
    docs: &[Value],
    cube: &Value,
    view: &Value,
    dim_names: &[String],
    inner_sql: &str,
    role: Option<&str>,
) -> Result<String, AppError> {
    let pairs: Vec<(String, String)> = dim_names
        .iter()
        .map(|d| (d.clone(), d.clone()))
        .collect();
    wrap_as_code_joins_with_aliases(docs, cube, view, &pairs, inner_sql, role, None)
}

pub(crate) fn wrap_as_code_joins_with_aliases(
    docs: &[Value],
    cube: &Value,
    view: &Value,
    dim_aliases: &[(String, String)],
    inner_sql: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let mut joins = Vec::new();
    let mut extra_proj = Vec::new();
    for (i, (dim, out_alias)) in dim_aliases.iter().enumerate() {
        if member_encrypted(cube, dim) {
            continue;
        }
        let Some((code_table, as_code)) = resolve_dim_as_code(docs, cube, view, dim)? else {
            continue;
        };
        let key_col = as_code
            .get("key_column")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("{dim}: as_code.key_column")))?;
        let value_col = as_code
            .get("value_column")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("{dim}: as_code.value_column")))?;
        let alias = format!("_code_{i}");
        let qalias = quote_ident(&alias)?;
        let derived = table_inline_derived(docs, &code_table, as_code, role, binds, &alias)?;
        joins.push(format!(
            "LEFT JOIN {derived} ON {q_inner}.{q_dim} = {qalias}.{q_key}",
            q_inner = quote_ident("_sem_agg")?,
            q_dim = quote_ident(out_alias)?,
            q_key = quote_ident(key_col)?,
        ));
        extra_proj.push(format!(
            "{qalias}.{} AS {}",
            quote_ident(value_col)?,
            quote_ident(&format!("{dim}__value"))?
        ));
    }
    if joins.is_empty() {
        return Ok(inner_sql.to_string());
    }
    let mut sql = format!(
        "SELECT {inner_alias}.*",
        inner_alias = quote_ident("_sem_agg")?
    );
    for p in &extra_proj {
        sql.push_str(", ");
        sql.push_str(p);
    }
    sql.push_str(&format!(
        " FROM ({inner_sql}) AS {inner_alias}",
        inner_alias = quote_ident("_sem_agg")?
    ));
    for j in &joins {
        sql.push(' ');
        sql.push_str(j);
    }
    let dialect = sqlparser::dialect::GenericDialect {};
    let stmts = sqlparser::parser::Parser::parse_sql(&dialect, &sql)
        .map_err(|e| AppError::Validation(format!("as_code wrap parse: {e}")))?;
    if stmts.len() != 1 {
        return Err(AppError::Validation(
            "as_code wrap: expected single statement".into(),
        ));
    }
    Ok(stmts[0].to_string())
}

pub(crate) fn table_inline_derived(
    docs: &[Value],
    code_table_name: &str,
    as_code: &Value,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    alias: &str,
) -> Result<String, AppError> {
    let table_doc = find_doc(docs, "table", code_table_name)?;
    check_role_access(table_doc.get("role"), role)?;
    let phys = physical_from(table_doc)?;
    let known = table_column_names(table_doc);
    let mut filters = collect_table_filters(table_doc, role, binds)?;
    if let Some(w) = as_code.get("where").and_then(|v| v.as_str()) {
        assert_filter_expr(w)?;
        assert_idents_known(w, &known)?;
        let resolved = apply_binds(w, binds);
        filters.push(format!("({resolved})"));
    }
    let qalias = quote_ident(alias)?;
    if filters.is_empty() {
        Ok(format!("{phys} AS {qalias}"))
    } else {
        Ok(format!(
            "(SELECT * FROM {phys} WHERE {}) AS {qalias}",
            filters.join(" AND ")
        ))
    }
}

/// Resolve dimension `as_code` dictionary (on the cube member, not table column).
pub fn resolve_dim_as_code<'a>(
    docs: &'a [Value],
    cube: &'a Value,
    _view: &'a Value,
    dim_name: &str,
) -> Result<Option<(String, &'a Value)>, AppError> {
    let Some(member) = find_dimension(cube, dim_name) else {
        return Ok(None);
    };
    let Some(as_code) = member.get("as_code") else {
        return Ok(None);
    };
    if !as_code.is_object() {
        return Ok(None);
    }
    let code_table = as_code
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation(format!("{dim_name}: as_code.table")))?;
    let _ = find_doc(docs, "table", code_table)?;
    Ok(Some((code_table.to_string(), as_code)))
}

pub(crate) fn find_dimension<'a>(cube: &'a Value, name: &str) -> Option<&'a Value> {
    cube.get("dimensions")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(|v| v.as_str()) == Some(name))
        })
}

pub(crate) fn find_cube_member<'a>(cube: &'a Value, name: &str) -> Option<&'a Value> {
    find_dimension(cube, name).or_else(|| {
        cube.get("facts").and_then(|v| v.as_array()).and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(|v| v.as_str()) == Some(name))
        })
    })
}

pub(crate) fn output_col_expr(
    view: &Value,
    view_col: &str,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    if let Some(a) = set_alias {
        Ok(format!("{}.{}", quote_ident(a)?, quote_ident(view_col)?))
    } else {
        view_physical_expr(view, view_col)
    }
}

pub(crate) fn collect_member_view_cols(cube: &Value, expr: &str, out: &mut HashSet<String>) {
    let parsed_idents = extract_idents(expr).ok();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let matched = match &parsed_idents {
                    Some(set) => set.contains(name),
                    None => expr_has_ident(expr, name),
                };
                if matched {
                    if let Some(col) = item.get("column").and_then(|v| v.as_str()) {
                        out.insert(col.to_string());
                    } else if let Some(e) = item.get("expression").and_then(|v| v.as_str()) {
                        if let Ok(inner_ids) = extract_idents(e) {
                            for id in inner_ids {
                                out.insert(id);
                            }
                        }
                    } else {
                        out.insert(name.to_string());
                    }
                }
            }
        }
    }
}

pub(crate) fn expr_has_ident(expr: &str, ident: &str) -> bool {
    let chars: Vec<char> = expr.chars().collect();
    let id: Vec<char> = ident.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + id.len() <= chars.len() && chars[i..i + id.len()] == id[..] {
            let before_ok = i == 0 || !is_ident_char(chars[i - 1]);
            let after_ok = i + id.len() == chars.len() || !is_ident_char(chars[i + id.len()]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

pub(crate) fn member_encrypted(cube: &Value, name: &str) -> bool {
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if item.get("name").and_then(|v| v.as_str()) == Some(name) {
                    return item
                        .get("encryption")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                }
            }
        }
    }
    false
}

pub(crate) fn validate_cube_member_expressions(
    cube: &Value,
    view_cols: &HashSet<String>,
) -> Result<(), AppError> {
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(expr) = item.get("expression").and_then(|v| v.as_str()) {
                    assert_metric_expr(expr)?;
                    let _ = view_cols;
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn cube_member_to_physical_map(
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<HashMap<String, String>, AppError> {
    let view_cols = view_column_name_set(view);
    let mut map = HashMap::new();
    if let Some(cols) = view.get("columns").and_then(|v| v.as_array()) {
        for c in cols {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if !name.is_empty() {
                map.insert(name.to_string(), output_col_expr(view, name, set_alias)?);
            }
        }
    }
    for key in ["facts", "dimensions"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                if let Some(e) = item.get("expression").and_then(|v| v.as_str()) {
                    if extract_idents(e)
                        .map(|ids| ids.iter().all(|id| view_cols.contains(id)))
                        .unwrap_or(false)
                    {
                        map.insert(name.to_string(), rewrite_view_idents(e, view, set_alias)?);
                    }
                } else if let Some(col) = item.get("column").and_then(|v| v.as_str()) {
                    if view_cols.contains(col) {
                        map.insert(name.to_string(), output_col_expr(view, col, set_alias)?);
                    }
                }
            }
        }
    }
    Ok(map)
}

pub fn expand_semantic_subqueries(
    expr: &str,
    docs: &[Value],
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    use std::cell::Cell;
    thread_local! {
        static DEPTH: Cell<usize> = const { Cell::new(0) };
    }
    crate::ast_rewrite::rewrite_subqueries(expr, |sub_sql| {
        // Only compile subqueries whose top-level FROM is a known cube table.
        // Already-physical SQL (derived FROM, warehouse tables) must not re-enter compile (D24).
        if !top_level_from_is_known_cube(sub_sql, docs) {
            return Ok(None);
        }
        // Drop parent ephemeral __adhoc_* metrics so nested compile cannot re-expand them.
        let clean_docs: Vec<Value> = docs
            .iter()
            .filter(|d| {
                let is_adhoc = d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                    && d.get("name")
                        .and_then(|v| v.as_str())
                        .is_some_and(|n| n.starts_with("__adhoc_"));
                !is_adhoc
            })
            .cloned()
            .collect();
        DEPTH.with(|d| {
            let depth = d.get();
            if depth >= 32 {
                return Err(AppError::Validation(
                    "semantic subquery nesting too deep".into(),
                ));
            }
            d.set(depth + 1);
            let compiled = crate::pipeline::compile_semantic_sql_with_binds(
                &clean_docs,
                sub_sql,
                role,
                binds,
                DialectProfile::Postgres,
            );
            d.set(d.get().saturating_sub(1));
            compiled.map(Some)
        })
    })
}

fn top_level_from_is_known_cube(sql: &str, docs: &[Value]) -> bool {
    use sqlparser::ast::{SetExpr, TableFactor};
    let Ok(q) = crate::pipeline::parse_query(sql) else {
        return false;
    };
    let select = match q.body.as_ref() {
        SetExpr::Select(s) => s,
        SetExpr::Query(inner) => match inner.body.as_ref() {
            SetExpr::Select(s) => s,
            _ => return false,
        },
        _ => return false,
    };
    let Some(tw) = select.from.first() else {
        return false;
    };
    // Joined cube SELECTs are still semantic.
    let mut tables = vec![&tw.relation];
    for j in &tw.joins {
        tables.push(&j.relation);
    }
    tables.iter().any(|rel| match rel {
        TableFactor::Table { name, .. } if name.0.len() == 1 => {
            let n = name.0[0].to_string();
            docs.iter().any(|d| {
                d.get("kind").and_then(|v| v.as_str()) == Some("cube")
                    && d.get("name").and_then(|v| v.as_str()) == Some(n.as_str())
            })
        }
        _ => false,
    })
}

pub(crate) fn rewrite_metric_expression(
    expr: &str,
    docs: &[Value],
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let sub_expanded = expand_semantic_subqueries(expr, docs, role, binds)?;
    let map = cube_member_to_physical_map(cube, view, set_alias)?;
    crate::ast_rewrite::rewrite_idents(&sub_expanded, &map)
}

pub(crate) fn rewrite_member_or_view_idents(
    expr: &str,
    docs: &[Value],
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let sub_expanded = expand_semantic_subqueries(expr, docs, role, binds)?;
    let resolved = apply_binds(&sub_expanded, binds);
    let map = cube_member_to_physical_map(cube, view, set_alias)?;
    crate::ast_rewrite::rewrite_idents(&resolved, &map)
}

pub(crate) fn expr_references_any_metric(expr: &str, rewritten_metrics: &HashMap<String, String>) -> bool {
    if rewritten_metrics.is_empty() {
        return false;
    }
    extract_idents(expr)
        .map(|ids| ids.iter().any(|id| rewritten_metrics.contains_key(id)))
        .unwrap_or(false)
}

pub(crate) fn rewrite_composite_metric_projection(
    expr: &str,
    rewritten_metrics: &HashMap<String, String>,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    let mut map = cube_member_to_physical_map(cube, view, set_alias)?;
    for (m_name, m_phys) in rewritten_metrics {
        map.insert(m_name.clone(), format!("({m_phys})"));
    }
    crate::ast_rewrite::rewrite_idents(expr, &map)
}

pub(crate) fn rewrite_having_expr(
    expr: &str,
    projections: &[(String, String)],
    rewritten_metrics: &HashMap<String, String>,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    assert_having_expr(expr)?;
    let mut map = cube_member_to_physical_map(cube, view, set_alias)?;
    for (p_expr, p_alias) in projections {
        map.entry(p_alias.clone())
            .or_insert_with(|| format!("({p_expr})"));
    }
    for (m_name, m_phys) in rewritten_metrics {
        map.insert(m_name.clone(), format!("({m_phys})"));
    }
    let known: HashSet<String> = map.keys().cloned().collect();
    assert_idents_known(expr, &known)?;
    let resolved = apply_binds(expr, binds);
    crate::ast_rewrite::rewrite_idents(&resolved, &map)
}

pub(crate) fn parse_order_by_item(ob: &str) -> Result<sqlparser::ast::OrderByExpr, AppError> {
    use sqlparser::ast::{OrderByKind, Statement};
    use sqlparser::dialect::GenericDialect;
    use sqlparser::parser::Parser;

    let wrapped = format!("SELECT 1 ORDER BY {ob}");
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, &wrapped)
        .map_err(|e| AppError::Validation(format!("ORDER BY parse error: {e}")))?;
    let Some(Statement::Query(q)) = stmts.into_iter().next() else {
        return Err(AppError::Validation("ORDER BY: expected query".into()));
    };
    let Some(order_by) = q.order_by else {
        return Err(AppError::Validation("ORDER BY: missing clause".into()));
    };
    let OrderByKind::Expressions(mut exprs) = order_by.kind else {
        return Err(AppError::Validation(
            "ORDER BY ALL is not supported".into(),
        ));
    };
    if exprs.len() != 1 {
        return Err(AppError::Validation(
            "ORDER BY: expected single expression per item".into(),
        ));
    }
    Ok(exprs.remove(0))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_order_by_req_cols(
    cube: &Value,
    members: &HashSet<String>,
    proj_aliases: &HashSet<String>,
    metric_names: &HashSet<String>,
    order_by: &[String],
    allow_agg: bool,
    role: Option<&str>,
    req_cols: &mut HashSet<String>,
) -> Result<(), AppError> {
    use sqlparser::ast::Expr;

    for ob in order_by {
        let ob_expr = parse_order_by_item(ob)?;
        match &ob_expr.expr {
            Expr::Value(v) if v.to_string().parse::<usize>().is_ok() => {}
            Expr::Identifier(ident) => {
                let id = ident.value.as_str();
                if proj_aliases.contains(id) || metric_names.contains(id) {
                    continue;
                }
                if members.contains(id) {
                    check_member_role(cube, id, role)?;
                    if member_encrypted(cube, id) {
                        return Err(AppError::Validation(format!(
                            "encrypted member {id} cannot be used in ORDER BY"
                        )));
                    }
                    collect_member_view_cols(cube, id, req_cols);
                } else {
                    return Err(AppError::Validation(format!(
                        "unknown ORDER BY target: {id}"
                    )));
                }
            }
            other => {
                let expr_sql = other.to_string();
                if allow_agg {
                    assert_metric_expr(&expr_sql)?;
                } else {
                    assert_scalar_expr(&expr_sql)?;
                }
                for id in extract_idents(&expr_sql)? {
                    if members.contains(&id) {
                        check_member_role(cube, &id, role)?;
                        if member_encrypted(cube, &id) {
                            return Err(AppError::Validation(format!(
                                "encrypted member {id} cannot be used in ORDER BY"
                            )));
                        }
                        collect_member_view_cols(cube, &id, req_cols);
                    } else if !proj_aliases.contains(&id) && !metric_names.contains(&id) {
                        return Err(AppError::Validation(format!(
                            "unknown ORDER BY target: {id}"
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn rewrite_order_by_clauses(
    order_by: &[String],
    projections: &[(String, String)],
    rewritten_metrics: &HashMap<String, String>,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    _allow_agg: bool,
) -> Result<(Vec<String>, bool), AppError> {
    use sqlparser::ast::Expr;

    if order_by.is_empty() {
        return Ok((Vec::new(), true));
    }
    let proj_alias_set: HashSet<&str> = projections.iter().map(|(_, a)| a.as_str()).collect();
    let mut full_map = cube_member_to_physical_map(cube, view, set_alias)?;
    for (p_expr, p_alias) in projections {
        full_map
            .entry(p_alias.clone())
            .or_insert_with(|| p_expr.clone());
    }
    for (m_name, m_phys) in rewritten_metrics {
        full_map.insert(m_name.clone(), m_phys.clone());
    }

    let mut out = Vec::with_capacity(order_by.len());
    let mut outer_compatible = true;
    for ob in order_by {
        let mut ob_expr = parse_order_by_item(ob)?;
        match &mut ob_expr.expr {
            Expr::Value(v) if v.to_string().parse::<usize>().is_ok() => {
                out.push(ob_expr.to_string());
            }
            Expr::Identifier(ident) if proj_alias_set.contains(ident.value.as_str()) => {
                ident.quote_style = Some('"');
                out.push(ob_expr.to_string());
            }
            other => {
                outer_compatible = false;
                let raw_expr = other.to_string();
                let resolved = apply_binds(&raw_expr, binds);
                let rewritten = crate::ast_rewrite::rewrite_idents(&resolved, &full_map)?;
                let mut item_sql = rewritten;
                match ob_expr.options.asc {
                    Some(true) => item_sql.push_str(" ASC"),
                    Some(false) => item_sql.push_str(" DESC"),
                    None => {}
                }
                match ob_expr.options.nulls_first {
                    Some(true) => item_sql.push_str(" NULLS FIRST"),
                    Some(false) => item_sql.push_str(" NULLS LAST"),
                    None => {}
                }
                out.push(item_sql);
            }
        }
    }
    Ok((out, outer_compatible))
}

pub(crate) fn inject_filter_into_aggregates(
    metric_sql: &str,
    filter_sql: &str,
) -> Result<String, AppError> {
    use sqlparser::ast::{
        Expr, FunctionArg, FunctionArgExpr, FunctionArguments, SelectItem, SetExpr, Statement,
        VisitMut, VisitorMut,
    };
    use sqlparser::dialect::GenericDialect;
    use sqlparser::parser::Parser;

    struct AggCaseInjector<'a> {
        filter_sql: &'a str,
    }

    impl VisitorMut for AggCaseInjector<'_> {
        type Break = ();

        fn pre_visit_expr(&mut self, expr: &mut Expr) -> std::ops::ControlFlow<Self::Break> {
            if let Expr::Function(func) = expr {
                let lower = func.name.to_string().to_ascii_lowercase();
                if func.over.is_none()
                    && matches!(lower.as_str(), "sum" | "count" | "avg" | "min" | "max")
                {
                    if let FunctionArguments::List(list) = &mut func.args {
                        for arg in &mut list.args {
                            match arg {
                                FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => {
                                    let case_str =
                                        format!("CASE WHEN ({}) THEN 1 END", self.filter_sql);
                                    if let Some(parsed) = parse_single_expr(&case_str) {
                                        *arg = FunctionArg::Unnamed(FunctionArgExpr::Expr(parsed));
                                    }
                                }
                                FunctionArg::Unnamed(FunctionArgExpr::Expr(inner)) => {
                                    let case_str = format!(
                                        "CASE WHEN ({}) THEN ({}) END",
                                        self.filter_sql, inner
                                    );
                                    if let Some(parsed) = parse_single_expr(&case_str) {
                                        *inner = parsed;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    return std::ops::ControlFlow::Continue(());
                }
            }
            std::ops::ControlFlow::Continue(())
        }
    }

    fn parse_single_expr(sql: &str) -> Option<Expr> {
        let wrapped = format!("SELECT {sql}");
        let dialect = GenericDialect {};
        let stmts = Parser::parse_sql(&dialect, &wrapped).ok()?;
        let Statement::Query(q) = stmts.into_iter().next()? else {
            return None;
        };
        let SetExpr::Select(sel) = *q.body else {
            return None;
        };
        match sel.projection.into_iter().next()? {
            SelectItem::UnnamedExpr(e) => Some(e),
            _ => None,
        }
    }

    let wrapped = format!("SELECT ({metric_sql})");
    let dialect = GenericDialect {};
    let mut stmts = Parser::parse_sql(&dialect, &wrapped)
        .map_err(|e| AppError::Validation(format!("inject_filter parse: {e}")))?;
    let Some(Statement::Query(q)) = stmts.get_mut(0) else {
        return Err(AppError::Validation("inject_filter: expected query".into()));
    };
    let SetExpr::Select(sel) = q.body.as_mut() else {
        return Err(AppError::Validation("inject_filter: expected select".into()));
    };
    let Some(SelectItem::UnnamedExpr(e)) = sel.projection.get_mut(0) else {
        return Err(AppError::Validation("inject_filter: bad projection".into()));
    };
    let mut v = AggCaseInjector { filter_sql };
    let _ = e.visit(&mut v);
    Ok(e.to_string())
}

pub(crate) fn rewrite_view_idents(
    expr: &str,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    let mut map = HashMap::new();
    if let Some(cols) = view.get("columns").and_then(|v| v.as_array()) {
        for c in cols {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            map.insert(name.to_string(), output_col_expr(view, name, set_alias)?);
        }
    }
    crate::ast_rewrite::rewrite_idents(expr, &map)
}

pub(crate) fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

