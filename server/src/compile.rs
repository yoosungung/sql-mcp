use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use crate::ast_select::{SelectClauses, assemble_select_with_clauses};
use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::expr_guard::{
    assert_filter_expr, assert_idents_known, assert_metric_expr, extract_idents,
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

fn table_column_names(table_doc: &Value) -> HashSet<String> {
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

fn cube_member_names(cube: &Value) -> HashSet<String> {
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

fn view_column_name_set(view: &Value) -> HashSet<String> {
    view.get("columns")
        .and_then(|v| v.as_array())
        .map(|cols| {
            cols.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn validate_table_filter(expr: &str, known: &HashSet<String>) -> Result<(), AppError> {
    assert_filter_expr(expr)?;
    assert_idents_known(expr, known)
}

fn filter_expr(v: Option<&Value>) -> Option<&str> {
    v.and_then(|f| {
        if f.is_null() {
            None
        } else {
            f.get("expression").and_then(|e| e.as_str())
        }
    })
}

fn docs_by_kind<'a>(
    docs: &'a [Value],
    kind: &'a str,
) -> impl Iterator<Item = &'a Value> + 'a {
    docs.iter()
        .filter(move |d| d.get("kind").and_then(|v| v.as_str()) == Some(kind))
}

fn find_doc<'a>(
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
pub fn compile_view_from(docs: &[Value], view: &Value) -> Result<String, AppError> {
    compile_view_from_opts(docs, view, None, None, &[], None)
}

pub fn compile_view_from_opts(
    docs: &[Value],
    view: &Value,
    role: Option<&str>,
    used_view_cols: Option<&HashSet<String>>,
    query_preds: &[String],
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let combine = view
        .get("combine")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("view.combine required".into()))?;
    match combine {
        "join" => compile_join_view(docs, view, role, used_view_cols, binds),
        "union" => compile_set_view(docs, view, role, used_view_cols, false, query_preds, binds),
        "minus" => compile_set_view(docs, view, role, used_view_cols, true, query_preds, binds),
        "select" => Err(AppError::Validation(
            "combine=select must be resolved before compile_view_from".into(),
        )),
        other => Err(AppError::Validation(format!(
            "combine={other} not yet supported in compile"
        ))),
    }
}

/// UNION ALL / EXCEPT of aligned projections per source table (Step 4 union pruning).
fn compile_set_view(
    docs: &[Value],
    view: &Value,
    role: Option<&str>,
    used_view_cols: Option<&HashSet<String>>,
    is_minus: bool,
    query_preds: &[String],
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let sources = view
        .get("sources")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("view.sources required".into()))?;
    if sources.len() < 2 {
        return Err(AppError::Validation(
            "union/minus view needs at least two sources".into(),
        ));
    }
    let proj_cols: Vec<String> = if let Some(used) = used_view_cols {
        let mut v: Vec<_> = used.iter().cloned().collect();
        v.sort();
        v
    } else {
        let mut v: Vec<_> = view_column_name_set(view).into_iter().collect();
        v.sort();
        v
    };
    if proj_cols.is_empty() {
        return Err(AppError::Validation(
            "union/minus needs at least one projected column".into(),
        ));
    }

    let empty = HashMap::new();
    let bind_map = binds.unwrap_or(&empty);
    let mut query_dnf = Vec::new();
    for p in query_preds {
        query_dnf.extend(crate::date_range::extract_dnf_with_binds(p, bind_map));
    }

    let op = if is_minus { "EXCEPT" } else { "UNION ALL" };
    let mut branches = Vec::new();
    for src in sources {
        let tname = source_name(src)?;
        if !source_maps_any_cols(view, tname, &proj_cols) {
            continue; // prune: no mapping for required columns
        }
        let table = find_doc(docs, "table", tname)?;
        if scope_is_dead(table) {
            continue; // prune: sentinel unsatisfiable scope
        }
        if let Some(scope) = filter_expr(table.get("scope_filter")) {
            let scope_dnf = crate::date_range::extract_dnf_with_binds(scope, bind_map);
            if crate::date_range::is_disjoint_dnf(&scope_dnf, &query_dnf) {
                continue; // prune: date range disjoint from query
            }
        }
        let mut select_parts = Vec::new();
        for col in &proj_cols {
            let phys = view_physical_expr_for_source(view, col, tname)?;
            select_parts.push(format!("{phys} AS {}", quote_ident(col)?));
        }
        let from = inline_table_ref(docs, tname, role, binds)?;
        branches.push(format!("SELECT {} FROM {}", select_parts.join(", "), from));
    }
    if branches.is_empty() {
        return Err(AppError::Validation(
            "all union/minus branches pruned".into(),
        ));
    }
    if is_minus && branches.len() < 2 {
        return Err(AppError::Validation(
            "minus needs at least two live branches".into(),
        ));
    }
    Ok(format!("({})", branches.join(&format!(" {op} "))))
}

fn source_maps_any_cols(view: &Value, source: &str, cols: &[String]) -> bool {
    let Some(vcols) = view.get("columns").and_then(|v| v.as_array()) else {
        return false;
    };
    for col in cols {
        for c in vcols {
            if c.get("name").and_then(|v| v.as_str()) != Some(col.as_str()) {
                continue;
            }
            if let Some(maps) = c.get("mappings").and_then(|v| v.as_array()) {
                if maps
                    .iter()
                    .any(|m| m.get("source").and_then(|v| v.as_str()) == Some(source))
                {
                    return true;
                }
            }
        }
    }
    false
}

fn scope_is_dead(table: &Value) -> bool {
    filter_expr(table.get("scope_filter"))
        .map(|e| {
            let t = e.replace(' ', "").to_ascii_lowercase();
            t == "1=0" || t == "false" || t == "0=1"
        })
        .unwrap_or(false)
}

fn view_physical_expr_for_source(
    view: &Value,
    view_col: &str,
    source: &str,
) -> Result<String, AppError> {
    let cols = view
        .get("columns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("view.columns required".into()))?;
    let col = cols
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(view_col))
        .ok_or_else(|| AppError::NotFound(format!("view column {view_col}")))?;
    let mapping = col
        .get("mappings")
        .and_then(|v| v.as_array())
        .and_then(|m| {
            m.iter()
                .find(|x| x.get("source").and_then(|v| v.as_str()) == Some(source))
        })
        .ok_or_else(|| {
            AppError::Validation(format!("no mapping for {view_col} on source {source}"))
        })?;
    let column = mapping
        .get("column")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("mapping.column".into()))?;
    Ok(format!("{}.{}", quote_ident(source)?, quote_ident(column)?))
}

/// Resolve cube.view, applying summary-view routing when combine=select (Step 3).
pub fn resolve_view_for_cube(
    docs: &[Value],
    cube: &Value,
    req_cols: &HashSet<String>,
    is_semi_additive: bool,
) -> Result<Value, AppError> {
    let view_name = cube
        .get("view")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("cube.view required".into()))?;
    let view = find_doc(docs, "view", view_name)?.clone();
    let combine = view.get("combine").and_then(|v| v.as_str()).unwrap_or("");
    let prefer = view.get("prefer").and_then(|v| v.as_str());
    if combine != "select" {
        return Ok(view);
    }
    let sources = view
        .get("sources")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("select view.sources required".into()))?;
    let mut summary_first: Vec<&Value> = Vec::new();
    let mut rest: Vec<&Value> = Vec::new();
    for s in sources {
        if s.get("summary").and_then(|v| v.as_bool()).unwrap_or(false) {
            summary_first.push(s);
        } else {
            rest.push(s);
        }
    }
    let order: Vec<&Value> = if prefer == Some("summary") {
        summary_first.into_iter().chain(rest).collect()
    } else {
        rest.into_iter().chain(summary_first).collect()
    };
    for src in order {
        let sname = source_name(src)?;
        let skind = src
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("view");
        let is_summary = src.get("summary").and_then(|v| v.as_bool()).unwrap_or(false);
        let candidate = find_doc(docs, skind, sname)?;
        if skind == "view" && view_covers_cols(candidate, req_cols) {
            if is_semi_additive && is_summary {
                // Semi-additive: only accept a summary candidate if its grain matches req_cols exactly.
                let cand_cols = view_column_name_set(candidate);
                if cand_cols != *req_cols {
                    continue;
                }
            }
            return Ok(candidate.clone());
        }
        if skind == "table" {
            let tcols = table_column_names(candidate);
            if req_cols.is_subset(&tcols) {
                return Ok(json_singleton_table_view(sname, candidate)?);
            }
        }
    }
    Err(AppError::Validation(
        "no select-view source covers required columns".into(),
    ))
}

fn json_singleton_table_view(name: &str, _table: &Value) -> Result<Value, AppError> {
    Err(AppError::Validation(format!(
        "table source {name} in select view not yet wrapped; use a join view source"
    )))
}

fn view_covers_cols(view: &Value, req: &HashSet<String>) -> bool {
    let have = view_column_name_set(view);
    req.iter().all(|c| have.contains(c))
}

fn compile_join_view(
    docs: &[Value],
    view: &Value,
    role: Option<&str>,
    used_view_cols: Option<&HashSet<String>>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let sources = view
        .get("sources")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("view.sources required".into()))?;
    if sources.is_empty() {
        return Err(AppError::Validation("view.sources empty".into()));
    }
    let joins = view
        .get("joins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let all_names: HashSet<String> = sources
        .iter()
        .filter_map(|s| source_name(s).ok().map(str::to_string))
        .collect();
    let mut t_req = if let Some(cols) = used_view_cols {
        tables_for_view_cols(view, cols)
    } else {
        all_names.clone()
    };
    // Always anchor the primary/fact table (sources[0]) so LEFT JOIN direction is preserved.
    if let Some(base) = sources.first().and_then(|s| source_name(s).ok()) {
        t_req.insert(base.to_string());
    }
    let included = steiner_nodes(&joins, &t_req, &all_names)?;

    let mut ordered: Vec<String> = sources
        .iter()
        .filter_map(|s| source_name(s).ok().map(str::to_string))
        .filter(|n| included.contains(n))
        .collect();
    if ordered.is_empty() {
        return Err(AppError::Validation("no tables left after join prune".into()));
    }

    let first = ordered.remove(0);
    let mut sql = format!("{} ", inline_table_ref(docs, &first, role, binds)?);
    let mut placed: HashSet<String> = HashSet::from([first.clone()]);

    // Attach joins whose both ends are included and at least one side already placed.
    let mut pending = joins.clone();
    while placed.len() < included.len() {
        let mut progress = false;
        let mut join_err: Option<AppError> = None;
        pending.retain(|j| {
            if join_err.is_some() {
                return false;
            }
            let right = j.get("right").and_then(|v| v.as_str()).unwrap_or("");
            let left = j.get("left").and_then(|v| v.as_str()).unwrap_or("");
            if !included.contains(left) || !included.contains(right) {
                return false; // drop
            }
            let left_in = placed.contains(left);
            let right_in = placed.contains(right);
            if left_in && right_in {
                return false;
            }
            if !(left_in || right_in) {
                return true; // keep for later
            }
            // Attach the missing side.
            let new_side = if left_in { right } else { left };
            match format_join_fragment(docs, j, new_side, role, binds) {
                Ok(frag) => {
                    sql.push_str(&frag);
                    placed.insert(new_side.to_string());
                    progress = true;
                }
                Err(e) => {
                    join_err = Some(e);
                }
            }
            false
        });
        if let Some(e) = join_err {
            return Err(e);
        }
        if !progress {
            if let Some(missing) = included.iter().find(|n| !placed.contains(*n)) {
                sql.push_str(&format!(
                    " CROSS JOIN {} ",
                    inline_table_ref(docs, missing, role, binds)?
                ));
                placed.insert(missing.clone());
            } else {
                break;
            }
        }
    }
    Ok(sql.trim_end().to_string())
}

fn format_join_fragment(
    docs: &[Value],
    j: &Value,
    new_side: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let left = j
        .get("left")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("join.left required".into()))?;
    let right = j
        .get("right")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("join.right required".into()))?;
    let jtype = j.get("type").and_then(|v| v.as_str()).unwrap_or("inner");
    let reversed = new_side == left;
    let join_kw = match (jtype, reversed) {
        ("inner", _) => "INNER JOIN",
        ("left", false) | ("right", true) => "LEFT JOIN",
        ("right", false) | ("left", true) => "RIGHT JOIN",
        ("full", _) => "FULL JOIN",
        (other, _) => {
            return Err(AppError::Validation(format!("unsupported join type {other}")));
        }
    };
    let mut ons = Vec::new();
    for on in j
        .get("on")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("join.on required".into()))?
    {
        let lc = on
            .get("left_column")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("on.left_column".into()))?;
        let rc = on
            .get("right_column")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("on.right_column".into()))?;
        ons.push(format!(
            "{}.{} = {}.{}",
            quote_ident(left)?,
            quote_ident(lc)?,
            quote_ident(right)?,
            quote_ident(rc)?
        ));
    }
    Ok(format!(
        " {join_kw} {} ON {}",
        inline_table_ref(docs, new_side, role, binds)?,
        ons.join(" AND ")
    ))
}

/// Tables needed for the given view column names (first mapping source).
fn tables_for_view_cols(view: &Value, cols: &HashSet<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Some(vcols) = view.get("columns").and_then(|v| v.as_array()) {
        for c in vcols {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if !cols.contains(name) {
                continue;
            }
            if let Some(m) = c
                .get("mappings")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
            {
                if let Some(src) = m.get("source").and_then(|v| v.as_str()) {
                    out.insert(src.to_string());
                }
            }
        }
    }
    out
}

/// Steiner-tree approximation: connect all terminals with shortest paths on the join graph.
fn steiner_nodes(
    joins: &[Value],
    terminals: &HashSet<String>,
    all: &HashSet<String>,
) -> Result<HashSet<String>, AppError> {
    if terminals.is_empty() {
        return Ok(all.clone());
    }
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for j in joins {
        let l = j
            .get("left")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("join.left".into()))?;
        let r = j
            .get("right")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("join.right".into()))?;
        adj.entry(l.to_string()).or_default().push(r.to_string());
        adj.entry(r.to_string()).or_default().push(l.to_string());
    }
    for n in all {
        adj.entry(n.clone()).or_default();
    }

    let mut included: HashSet<String> = terminals.clone();
    let terms: Vec<String> = terminals.iter().cloned().collect();
    let root = &terms[0];
    for t in terms.iter().skip(1) {
        if let Some(path) = shortest_path(&adj, root, t) {
            for n in path {
                included.insert(n);
            }
        } else {
            return Err(AppError::Validation(format!(
                "join graph cannot connect {root} to {t}"
            )));
        }
    }
    Ok(included)
}

fn shortest_path(
    adj: &HashMap<String, Vec<String>>,
    start: &str,
    goal: &str,
) -> Option<Vec<String>> {
    if start == goal {
        return Some(vec![start.to_string()]);
    }
    let mut prev: HashMap<String, String> = HashMap::new();
    let mut q = VecDeque::from([start.to_string()]);
    let mut seen = HashSet::from([start.to_string()]);
    while let Some(cur) = q.pop_front() {
        for nxt in adj.get(&cur).into_iter().flatten() {
            if seen.insert(nxt.clone()) {
                prev.insert(nxt.clone(), cur.clone());
                if nxt == goal {
                    let mut path = vec![goal.to_string()];
                    let mut p = goal.to_string();
                    while let Some(x) = prev.get(&p) {
                        path.push(x.clone());
                        p = x.clone();
                    }
                    path.reverse();
                    return Some(path);
                }
                q.push_back(nxt.clone());
            }
        }
    }
    None
}

/// `(SELECT * FROM phys WHERE filters) AS "alias"` — filters unqualified (Step 6).
fn inline_table_ref(
    docs: &[Value],
    alias: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let table = find_doc(docs, "table", alias)?;
    check_role_access(table.get("role"), role)?;
    let phys = physical_from(table)?;
    let filters = collect_table_filters(table, role, binds)?;
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

fn source_name(src: &Value) -> Result<&str, AppError> {
    if let Some(s) = src.as_str() {
        return Ok(s);
    }
    src.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("source name missing".into()))
}

fn view_physical_expr(view: &Value, view_col: &str) -> Result<String, AppError> {
    let cols = view
        .get("columns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("view.columns required".into()))?;
    let col = cols
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(view_col))
        .ok_or_else(|| AppError::NotFound(format!("view column {view_col}")))?;
    let mapping = col
        .get("mappings")
        .and_then(|v| v.as_array())
        .and_then(|m| m.first())
        .ok_or_else(|| AppError::Validation(format!("no mapping for {view_col}")))?;
    let source = mapping
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("mapping.source".into()))?;
    let column = mapping
        .get("column")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("mapping.column".into()))?;
    Ok(format!("{}.{}", quote_ident(source)?, quote_ident(column)?))
}

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
    /// D23 (C): dim/fact-only row-level SELECT (no metrics, no GROUP BY/HAVING).
    pub ungrouped: bool,
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
    };
    compile_cube_query(docs, cube_name, &spec, role, binds)
}

/// D23 (C): project dimensions/facts as row-level columns (no aggregation).
fn compile_cube_query_ungrouped(
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
        if !members.contains(member) {
            return Err(AppError::Validation(format!(
                "unknown semantic member: {member}"
            )));
        }
        check_member_role(cube, member, role)?;
    }

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
            assert_filter_expr(expr)?;
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
        assert_filter_expr(w_expr)?;
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

    let view = resolve_view_for_cube(docs, cube, &req_cols, false)?;
    let combine = view.get("combine").and_then(|v| v.as_str()).unwrap_or("");
    let set_alias = matches!(combine, "union" | "minus").then_some("_sem_uv");
    let mut from =
        compile_view_from_opts(docs, &view, role, Some(&req_cols), &query_preds, binds)?;
    if let Some(a) = set_alias {
        from = format!("{from} AS {}", quote_ident(a)?);
    }

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
                expr, cube, &view, set_alias, binds,
            )?);
        }
    }
    for w_expr in &spec.where_exprs {
        filters.push(rewrite_member_or_view_idents(
            w_expr, cube, &view, set_alias, binds,
        )?);
    }

    let has_as_code = as_code_dims.iter().any(|(dim, _)| {
        !member_encrypted(cube, dim)
            && resolve_dim_as_code(docs, cube, &view, dim)
                .ok()
                .flatten()
                .is_some()
    });

    let inner_clauses = SelectClauses {
        aggregation: Some("none".into()),
        having: None,
        order_by: if has_as_code {
            Vec::new()
        } else {
            spec.order_by.clone()
        },
        limit: if has_as_code {
            None
        } else {
            spec.limit.clone()
        },
        offset: if has_as_code {
            None
        } else {
            spec.offset.clone()
        },
    };

    let sql = assemble_select_with_clauses(&projections, &from, &filters, &[], &inner_clauses)?;
    let mut sql =
        wrap_as_code_joins_with_aliases(docs, cube, &view, &as_code_dims, &sql, role, binds)?;
    if has_as_code
        && (!spec.order_by.is_empty() || spec.limit.is_some() || spec.offset.is_some())
    {
        if !spec.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&spec.order_by.join(", "));
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
    if spec.metrics.is_empty() {
        return Err(AppError::Validation(
            "semantic SELECT must project at least one metric".into(),
        ));
    }
    let cube = find_doc(docs, "cube", cube_name)?;
    check_role_access(cube.get("role"), role)?;

    let members = cube_member_names(cube);
    let mut metric_docs = Vec::with_capacity(spec.metrics.len());
    let mut allowed_gb: Vec<String> = Vec::new();
    let mut is_semi = false;
    let mut agg_mode: Option<String> = spec.aggregation.clone();

    for m_name in &spec.metrics {
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
        assert_metric_expr(m_expr)?;
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
            assert_filter_expr(f_expr)?;
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
        if let Some(gb_arr) = m_doc.get("group_by").and_then(|v| v.as_array()) {
            for g in gb_arr {
                if let Some(gs) = g.as_str() {
                    if !allowed_gb.iter().any(|x| x == gs) {
                        allowed_gb.push(gs.to_string());
                    }
                }
            }
        }
        metric_docs.push((m_name.as_str(), m_doc, m_expr));
    }

    // Determine effective group_by dimensions.
    let active_gb: Vec<String> = if let Some(explicit) = &spec.explicit_group_by {
        for d in explicit {
            if !allowed_gb.iter().any(|g| g == d) {
                return Err(AppError::Validation(format!(
                    "dimension {d} not in metric group_by"
                )));
            }
            check_member_role(cube, d, role)?;
        }
        explicit.clone()
    } else if !spec.dimensions.is_empty() {
        for d in &spec.dimensions {
            if !allowed_gb.iter().any(|g| g == d) {
                return Err(AppError::Validation(format!(
                    "dimension {d} not in metric group_by"
                )));
            }
            check_member_role(cube, d, role)?;
        }
        spec.dimensions.clone()
    } else {
        for d in &allowed_gb {
            check_member_role(cube, d, role)?;
        }
        allowed_gb.clone()
    };

    for d in &spec.dimensions {
        check_member_role(cube, d, role)?;
    }

    let mut req_cols: HashSet<String> = HashSet::new();
    for gname in &active_gb {
        collect_member_view_cols(cube, gname, &mut req_cols);
    }
    for d in &spec.dimensions {
        collect_member_view_cols(cube, d, &mut req_cols);
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
            assert_filter_expr(expr)?;
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
        assert_filter_expr(w_expr)?;
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

    // Decide whether metric filters can be shared in outer WHERE or must be injected via CASE WHEN.
    let first_m_filter = filter_expr(metric_docs[0].1.get("filter"));
    let shared_metric_filter = metric_docs
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

    let view_cols = view_column_name_set(&view);
    validate_cube_member_expressions(cube, &view_cols)?;

    let mut rewritten_metrics: HashMap<String, String> = HashMap::new();
    for (m_name, m_doc, m_expr) in &metric_docs {
        let mut rewritten = rewrite_metric_expression(m_expr, cube, &view, set_alias)?;
        if !shared_metric_filter {
            if let Some(f_expr) = filter_expr(m_doc.get("filter")) {
                let f_rewritten =
                    rewrite_member_or_view_idents(f_expr, cube, &view, set_alias, binds)?;
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
            as_code_dims.push((gname.clone(), gname.clone()));
        }
        for (m_name, _, _) in &metric_docs {
            let expr = rewritten_metrics.get(*m_name).unwrap().clone();
            projections.push((expr, (*m_name).to_string()));
        }
    } else {
        // If the user only wrote metrics in SELECT (no explicit dimensions), keep active_gb projected first
        // so group_by columns and as_code dictionary values are available in the result set.
        if spec.dimensions.is_empty() {
            for gname in &active_gb {
                let expr = compile_dimension_expr(cube, &view, gname, set_alias)?;
                projections.push((expr, gname.clone()));
                as_code_dims.push((gname.clone(), gname.clone()));
            }
        }
        for p in &spec.projections {
            if let Some(m_expr) = rewritten_metrics.get(&p.member) {
                projections.push((m_expr.clone(), p.alias.clone()));
            } else {
                let expr = compile_dimension_expr(cube, &view, &p.member, set_alias)?;
                projections.push((expr, p.alias.clone()));
                if !as_code_dims.iter().any(|(d, _)| d == &p.member) {
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
                expr, cube, &view, set_alias, binds,
            )?);
        }
    }
    for seg_name in &spec.segments {
        let seg = find_doc(docs, "segment", seg_name)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            filters.push(rewrite_member_or_view_idents(
                expr, cube, &view, set_alias, binds,
            )?);
        }
    }
    for w_expr in &spec.where_exprs {
        filters.push(rewrite_member_or_view_idents(
            w_expr, cube, &view, set_alias, binds,
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
            &rewritten_metrics,
            cube,
            &view,
            set_alias,
            binds,
        )?)
    } else {
        None
    };

    let inner_clauses = SelectClauses {
        aggregation: agg_mode,
        having: rewritten_having,
        order_by: if has_as_code {
            Vec::new()
        } else {
            spec.order_by.clone()
        },
        limit: if has_as_code {
            None
        } else {
            spec.limit.clone()
        },
        offset: if has_as_code {
            None
        } else {
            spec.offset.clone()
        },
    };

    let sql = assemble_select_with_clauses(&projections, &from, &filters, &gb, &inner_clauses)?;
    // Step 7: after aggregation, LEFT JOIN code dictionaries for dims with as_code.
    let mut sql =
        wrap_as_code_joins_with_aliases(docs, cube, &view, &as_code_dims, &sql, role, binds)?;
    if has_as_code
        && (!spec.order_by.is_empty() || spec.limit.is_some() || spec.offset.is_some())
    {
        if !spec.order_by.is_empty() {
            sql.push_str(" ORDER BY ");
            sql.push_str(&spec.order_by.join(", "));
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

fn check_member_role(cube: &Value, name: &str, role: Option<&str>) -> Result<(), AppError> {
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

fn compile_dimension_expr(
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

fn compile_dimension_group_expr(
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
    output_col_expr(view, dim_name, set_alias)
}

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

fn wrap_as_code_joins_with_aliases(
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

fn table_inline_derived(
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

fn find_dimension<'a>(cube: &'a Value, name: &str) -> Option<&'a Value> {
    cube.get("dimensions")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(|v| v.as_str()) == Some(name))
        })
}

fn find_cube_member<'a>(cube: &'a Value, name: &str) -> Option<&'a Value> {
    find_dimension(cube, name).or_else(|| {
        cube.get("facts").and_then(|v| v.as_array()).and_then(|items| {
            items
                .iter()
                .find(|item| item.get("name").and_then(|v| v.as_str()) == Some(name))
        })
    })
}

fn output_col_expr(
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

fn collect_member_view_cols(cube: &Value, expr: &str, out: &mut HashSet<String>) {
    let expr_idents = extract_idents(expr).unwrap_or_default();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                if expr_idents.contains(name) || expr_has_ident(expr, name) {
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

fn expr_has_ident(expr: &str, ident: &str) -> bool {
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

fn member_encrypted(cube: &Value, name: &str) -> bool {
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

fn validate_cube_member_expressions(
    cube: &Value,
    view_cols: &HashSet<String>,
) -> Result<(), AppError> {
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(expr) = item.get("expression").and_then(|v| v.as_str()) {
                    assert_metric_expr(expr)?;
                    assert_idents_known(expr, view_cols)?;
                }
            }
        }
    }
    Ok(())
}

fn cube_member_to_physical_map(
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<HashMap<String, String>, AppError> {
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
                let phys = if let Some(e) = item.get("expression").and_then(|v| v.as_str()) {
                    rewrite_view_idents(e, view, set_alias)?
                } else if let Some(col) = item.get("column").and_then(|v| v.as_str()) {
                    output_col_expr(view, col, set_alias)?
                } else {
                    continue;
                };
                map.insert(name.to_string(), phys);
            }
        }
    }
    Ok(map)
}

fn rewrite_metric_expression(
    expr: &str,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    let map = cube_member_to_physical_map(cube, view, set_alias)?;
    crate::ast_rewrite::rewrite_idents(expr, &map)
}

fn rewrite_member_or_view_idents(
    expr: &str,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let resolved = apply_binds(expr, binds);
    let map = cube_member_to_physical_map(cube, view, set_alias)?;
    crate::ast_rewrite::rewrite_idents(&resolved, &map)
}

fn rewrite_having_expr(
    expr: &str,
    rewritten_metrics: &HashMap<String, String>,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    assert_filter_expr(expr)?;
    let mut map = cube_member_to_physical_map(cube, view, set_alias)?;
    for (m_name, m_phys) in rewritten_metrics {
        map.insert(m_name.clone(), m_phys.clone());
    }
    let known: HashSet<String> = map.keys().cloned().collect();
    assert_idents_known(expr, &known)?;
    let resolved = apply_binds(expr, binds);
    crate::ast_rewrite::rewrite_idents(&resolved, &map)
}

fn inject_filter_into_aggregates(
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
                if matches!(lower.as_str(), "sum" | "count" | "avg" | "min" | "max") {
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

fn rewrite_view_idents(
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

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example_docs() -> Vec<Value> {
        [
            include_str!("../../semantic/examples/tables/customer.json"),
            include_str!("../../semantic/examples/tables/orders.json"),
            include_str!("../../semantic/examples/tables/orders_month.json"),
            include_str!("../../semantic/examples/tables/region_codes.json"),
            include_str!("../../semantic/examples/tables/status_codes.json"),
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
    fn as_code_join_after_aggregation() {
        let docs = example_docs();
        let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
        assert!(
            sql.contains("region_code__value") || sql.contains("\"region_code__value\""),
            "expected region_code__value in {sql}"
        );
        assert!(
            sql.contains("_sem_agg") || sql.contains("\"_sem_agg\""),
            "expected outer wrap alias in {sql}"
        );
        assert!(
            sql.to_ascii_uppercase().contains("LEFT JOIN"),
            "expected LEFT JOIN code dict in {sql}"
        );
        assert!(
            sql.contains("lang = 'ko'") || sql.contains("lang = \"ko\"") || sql.contains("lang='ko'"),
            "expected as_code.where in {sql}"
        );
        // Join must be outside GROUP BY aggregation
        let upper = sql.to_ascii_uppercase();
        let group_pos = upper.find("GROUP BY").expect("GROUP BY");
        let left_pos = upper.rfind("LEFT JOIN").expect("LEFT JOIN");
        assert!(
            left_pos > group_pos,
            "LEFT JOIN should wrap after GROUP BY: {sql}"
        );
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_orders_with_filters() {
        let orders: Value =
            serde_json::from_str(include_str!("../../semantic/examples/tables/orders.json"))
                .unwrap();
        let sql = compile_table_select(&orders, &["order_id", "region_code"], Some("branch"))
            .unwrap();
        assert!(sql.contains("\"sales\".\"orders\""));
        assert!(sql.contains("order_date >="));
        assert!(sql.contains("status_code <>"));
        assert!(sql.contains("region_code = :caller_region"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_bad_ident() {
        assert!(quote_ident("a;drop").is_err());
    }

    #[test]
    fn physical_from_ok() {
        let doc = json!({"schema": "sales", "table": "orders"});
        assert_eq!(physical_from(&doc).unwrap(), "\"sales\".\"orders\"");
    }

    #[test]
    fn compiles_revenue_metric_with_segment() {
        let docs = example_docs();
        let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
        assert!(sql.contains("sum("));
        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("PAID"));
        // Step 6: table filters pushed into derived table
        assert!(sql.contains("SELECT * FROM"));
        assert!(sql.contains("region_code = :caller_region"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn prunes_unused_customer_join() {
        let docs = example_docs();
        let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
        // region_code/order_date/sale_amount/status map to orders first → customer pruned
        assert!(
            !sql.contains("AS \"customer\""),
            "expected customer prune, got {sql}"
        );
        assert!(sql.contains("AS \"orders\"") || sql.contains("\"orders\""));
    }

    #[test]
    fn summary_view_routing_prefers_summary() {
        let mut docs = example_docs();
        docs.push(json!({
            "kind": "view",
            "name": "orders_only",
            "combine": "join",
            "sources": [{"kind": "table", "name": "orders"}],
            "joins": [],
            "columns": [
                {"name": "region_code", "mappings": [{"source": "orders", "column": "region_code"}]},
                {"name": "order_date", "mappings": [{"source": "orders", "column": "order_date"}]},
                {"name": "sale_amount", "mappings": [{"source": "orders", "column": "sale_amount"}]},
                {"name": "status_code", "mappings": [{"source": "orders", "column": "status_code"}]},
                {"name": "fx_rate", "mappings": [{"source": "orders", "column": "fx_rate"}]},
                {"name": "customer_id", "mappings": [{"source": "orders", "column": "customer_id"}]}
            ]
        }));
        docs.push(json!({
            "kind": "view",
            "name": "sales_select",
            "combine": "select",
            "prefer": "summary",
            "sources": [
                {"kind": "view", "name": "orders_only", "summary": true},
                {"kind": "view", "name": "orders_customer", "summary": false}
            ],
            "columns": []
        }));
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
                && d.get("name").and_then(|v| v.as_str()) == Some("sales")
            {
                d["view"] = json!("sales_select");
            }
        }
        let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
        assert!(
            !sql.contains("AS \"customer\""),
            "summary route should use orders_only: {sql}"
        );
        assert!(sql.contains("sum("));
    }

    #[test]
    fn rejects_unknown_metric_ident() {
        let mut docs = example_docs();
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                && d.get("name").and_then(|v| v.as_str()) == Some("revenue")
            {
                d["expression"] = json!("sum(not_a_fact)");
            }
        }
        assert!(compile_metric_select(&docs, "revenue", &[], None).is_err());
    }

    #[test]
    fn prunes_union_branch_with_bind_params() {
        let mut docs = example_docs();
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("table")
                && d.get("name").and_then(|v| v.as_str()) == Some("orders")
            {
                d["scope_filter"] = json!({
                    "expression": "order_date >= '2020-01-01' AND order_date < '2021-01-01'"
                });
            }
            if d.get("kind").and_then(|v| v.as_str()) == Some("table")
                && d.get("name").and_then(|v| v.as_str()) == Some("orders_month")
            {
                d["scope_filter"] = json!({
                    "expression": "order_date >= '2024-01-01' AND order_date < '2025-01-01'"
                });
            }
        }
        docs.push(json!({
            "kind": "view",
            "name": "orders_union",
            "combine": "union",
            "sources": [
                {"kind": "table", "name": "orders"},
                {"kind": "table", "name": "orders_month"}
            ],
            "columns": [
                {"name": "region_code", "mappings": [
                    {"source": "orders", "column": "region_code"},
                    {"source": "orders_month", "column": "region_code"}
                ]},
                {"name": "order_date", "mappings": [
                    {"source": "orders", "column": "order_date"},
                    {"source": "orders_month", "column": "order_month"}
                ]},
                {"name": "sale_amount", "mappings": [
                    {"source": "orders", "column": "sale_amount"},
                    {"source": "orders_month", "column": "sale_amount"}
                ]},
                {"name": "status_code", "mappings": [
                    {"source": "orders", "column": "status_code"},
                    {"source": "orders_month", "column": "status_code"}
                ]},
                {"name": "fx_rate", "mappings": [
                    {"source": "orders", "column": "fx_rate"},
                    {"source": "orders_month", "column": "fx_rate"}
                ]},
                {"name": "customer_id", "mappings": [
                    {"source": "orders", "column": "customer_id"},
                    {"source": "orders_month", "column": "region_code"}
                ]}
            ]
        }));
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("cube") {
                d["view"] = json!("orders_union");
            }
            if d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                && d.get("name").and_then(|v| v.as_str()) == Some("revenue")
            {
                d["filter"] = json!({
                    "expression": "order_date >= :q_from AND order_date < :q_to"
                });
            }
        }
        let mut binds = HashMap::new();
        binds.insert("q_from".into(), "2020-06-01".into());
        binds.insert("q_to".into(), "2020-07-01".into());
        let sql =
            compile_metric_select_with_binds(&docs, "revenue", &[], Some("branch"), Some(&binds))
                .unwrap();
        assert!(
            !sql.contains("orders_month"),
            "bind-resolved 2020 window should prune 2024 branch: {sql}"
        );
    }

    #[test]
    fn prunes_union_branch_on_date_disjoint() {
        let mut docs = example_docs();
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("table")
                && d.get("name").and_then(|v| v.as_str()) == Some("orders")
            {
                d["scope_filter"] =
                    json!({ "expression": "order_date >= '2020-01-01' AND order_date < '2021-01-01'" });
            }
            if d.get("kind").and_then(|v| v.as_str()) == Some("table")
                && d.get("name").and_then(|v| v.as_str()) == Some("orders_month")
            {
                d["scope_filter"] = json!({
                    "expression": "order_date >= '2024-01-01' AND order_date < '2025-01-01'"
                });
                // align column name for interval extract
                if let Some(cols) = d.get_mut("columns").and_then(|v| v.as_array_mut()) {
                    for c in cols.iter_mut() {
                        if c.get("name").and_then(|v| v.as_str()) == Some("order_month") {
                            c["name"] = json!("order_date");
                        }
                    }
                }
            }
        }
        docs.push(json!({
            "kind": "view",
            "name": "orders_union",
            "combine": "union",
            "sources": [
                {"kind": "table", "name": "orders"},
                {"kind": "table", "name": "orders_month"}
            ],
            "columns": [
                {"name": "region_code", "mappings": [
                    {"source": "orders", "column": "region_code"},
                    {"source": "orders_month", "column": "region_code"}
                ]},
                {"name": "order_date", "mappings": [
                    {"source": "orders", "column": "order_date"},
                    {"source": "orders_month", "column": "order_date"}
                ]},
                {"name": "sale_amount", "mappings": [
                    {"source": "orders", "column": "sale_amount"},
                    {"source": "orders_month", "column": "sale_amount"}
                ]},
                {"name": "status_code", "mappings": [
                    {"source": "orders", "column": "status_code"},
                    {"source": "orders_month", "column": "status_code"}
                ]},
                {"name": "fx_rate", "mappings": [
                    {"source": "orders", "column": "fx_rate"},
                    {"source": "orders_month", "column": "fx_rate"}
                ]},
                {"name": "customer_id", "mappings": [
                    {"source": "orders", "column": "customer_id"},
                    {"source": "orders_month", "column": "region_code"}
                ]}
            ]
        }));
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
                && d.get("name").and_then(|v| v.as_str()) == Some("sales")
            {
                d["view"] = json!("orders_union");
            }
            if d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                && d.get("name").and_then(|v| v.as_str()) == Some("revenue")
            {
                d["filter"] = json!({
                    "expression": "order_date >= '2020-06-01' AND order_date < '2020-07-01'"
                });
            }
        }
        let sql = compile_metric_select(&docs, "revenue", &[], Some("branch")).unwrap();
        assert!(
            !sql.contains("orders_month"),
            "2024 branch should be pruned vs 2020 query: {sql}"
        );
        assert!(sql.contains("2020") || sql.contains("orders"), "{sql}");
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_union_view_and_prunes_dead_scope() {
        let mut docs = example_docs();
        // Dead branch: unsatisfiable scope
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("table")
                && d.get("name").and_then(|v| v.as_str()) == Some("orders_month")
            {
                d["scope_filter"] = json!({ "expression": "1=0" });
            }
        }
        docs.push(json!({
            "kind": "view",
            "name": "orders_union",
            "combine": "union",
            "sources": [
                {"kind": "table", "name": "orders"},
                {"kind": "table", "name": "orders_month"}
            ],
            "columns": [
                {"name": "region_code", "mappings": [
                    {"source": "orders", "column": "region_code"},
                    {"source": "orders_month", "column": "region_code"}
                ]},
                {"name": "order_date", "mappings": [
                    {"source": "orders", "column": "order_date"},
                    {"source": "orders_month", "column": "order_month"}
                ]},
                {"name": "sale_amount", "mappings": [
                    {"source": "orders", "column": "sale_amount"},
                    {"source": "orders_month", "column": "sale_amount"}
                ]},
                {"name": "status_code", "mappings": [
                    {"source": "orders", "column": "status_code"},
                    {"source": "orders_month", "column": "status_code"}
                ]},
                {"name": "fx_rate", "mappings": [
                    {"source": "orders", "column": "fx_rate"},
                    {"source": "orders_month", "column": "fx_rate"}
                ]},
                {"name": "customer_id", "mappings": [
                    {"source": "orders", "column": "customer_id"},
                    {"source": "orders_month", "column": "region_code"}
                ]}
            ]
        }));
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("cube")
                && d.get("name").and_then(|v| v.as_str()) == Some("sales")
            {
                d["view"] = json!("orders_union");
            }
        }
        let sql = compile_metric_select(&docs, "revenue", &["paid"], Some("branch")).unwrap();
        assert!(sql.contains("UNION ALL") || !sql.contains("orders_month"), "{sql}");
        assert!(
            !sql.contains("orders_month") || sql.matches("orders_month").count() == 0,
            "dead scope branch should be pruned: {sql}"
        );
        assert!(sql.contains("sum(") || sql.to_ascii_lowercase().contains("sum"), "{sql}");
        assert_select_only(&sql).unwrap();
    }
}
