//! Step 3–4: summary-view routing, join Steiner prune, union prune, inline table security views.
use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use crate::error::AppError;

use super::{
    check_role_access,
    collect_table_filters,
    filter_expr,
    find_doc,
    physical_from,
    quote_ident,
    table_column_names,
    view_column_name_set,
};

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
pub(crate) fn compile_set_view(
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

pub(crate) fn source_maps_any_cols(view: &Value, source: &str, cols: &[String]) -> bool {
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

pub(crate) fn scope_is_dead(table: &Value) -> bool {
    filter_expr(table.get("scope_filter"))
        .map(|e| {
            let t = e.replace(' ', "").to_ascii_lowercase();
            t == "1=0" || t == "false" || t == "0=1"
        })
        .unwrap_or(false)
}

pub(crate) fn view_physical_expr_for_source(
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
    resolve_view_for_cube_opts(docs, cube, req_cols, is_semi_additive, true)
}

/// Resolve cube.view with explicit control over whether `summary: true` candidates are allowed.
/// Non-DISTINCT ungrouped detail queries set `allow_summary = false` to avoid returning pre-aggregated rows.
pub fn resolve_view_for_cube_opts(
    docs: &[Value],
    cube: &Value,
    req_cols: &HashSet<String>,
    is_semi_additive: bool,
    allow_summary: bool,
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
        if is_summary && !allow_summary {
            continue;
        }
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

pub(crate) fn json_singleton_table_view(name: &str, _table: &Value) -> Result<Value, AppError> {
    Err(AppError::Validation(format!(
        "table source {name} in select view not yet wrapped; use a join view source"
    )))
}

pub(crate) fn view_covers_cols(view: &Value, req: &HashSet<String>) -> bool {
    let have = view_column_name_set(view);
    req.iter().all(|c| have.contains(c))
}

pub(crate) fn included_tables_for_join_view(
    view: &Value,
    used_view_cols: Option<&HashSet<String>>,
) -> Result<HashSet<String>, AppError> {
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
    let has_outer_join = joins.iter().any(|j| {
        !matches!(
            j.get("type").and_then(|v| v.as_str()).unwrap_or("inner"),
            "inner"
        )
    });
    // Anchor the primary/fact table (sources[0]) only when outer joins require root directionality
    // or when no specific columns resolved to a source table.
    if has_outer_join || t_req.is_empty() {
        if let Some(base) = sources.first().and_then(|s| source_name(s).ok()) {
            t_req.insert(base.to_string());
        }
    }
    steiner_nodes(&joins, &t_req, &all_names)
}

pub(crate) fn compile_join_view(
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

    let included = included_tables_for_join_view(view, used_view_cols)?;

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
    // Re-check `left_in && right_in` before each attachment so cyclic join graphs emit a spanning tree.
    let mut pending = joins.clone();
    while placed.len() < included.len() {
        let mut progress = false;
        let mut join_err: Option<AppError> = None;
        let mut next_pending = Vec::new();
        for j in pending {
            if join_err.is_some() {
                break;
            }
            let right = j.get("right").and_then(|v| v.as_str()).unwrap_or("");
            let left = j.get("left").and_then(|v| v.as_str()).unwrap_or("");
            if !included.contains(left) || !included.contains(right) {
                continue; // drop
            }
            let left_in = placed.contains(left);
            let right_in = placed.contains(right);
            if left_in && right_in {
                continue; // both already placed; skip cycle edge
            }
            if !(left_in || right_in) {
                next_pending.push(j); // keep for later pass
                continue;
            }
            // Attach the missing side.
            let new_side = if left_in { right } else { left };
            match format_join_fragment(docs, &j, new_side, role, binds) {
                Ok(frag) => {
                    sql.push_str(&frag);
                    placed.insert(new_side.to_string());
                    progress = true;
                }
                Err(e) => {
                    join_err = Some(e);
                }
            }
        }
        pending = next_pending;
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

pub(crate) fn format_join_fragment(
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

/// Tables needed for the given view column names.
/// Pass 1 collects single-source columns; Pass 2 resolves multi-mapped columns to an already
/// required source when possible, avoiding pulling in an unrelated table for a shared join key.
pub(crate) fn tables_for_view_cols(view: &Value, cols: &HashSet<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(vcols) = view.get("columns").and_then(|v| v.as_array()) else {
        return out;
    };
    let mut multi: Vec<&Vec<Value>> = Vec::new();
    for c in vcols {
        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if !cols.contains(name) {
            continue;
        }
        if let Some(maps) = c.get("mappings").and_then(|v| v.as_array()) {
            if maps.len() == 1 {
                if let Some(src) = maps[0].get("source").and_then(|v| v.as_str()) {
                    out.insert(src.to_string());
                }
            } else if maps.len() > 1 {
                multi.push(maps);
            }
        }
    }
    for maps in multi {
        let already_covered = maps.iter().any(|m| {
            m.get("source")
                .and_then(|v| v.as_str())
                .is_some_and(|s| out.contains(s))
        });
        if !already_covered {
            if let Some(src) = maps
                .first()
                .and_then(|m| m.get("source"))
                .and_then(|v| v.as_str())
            {
                out.insert(src.to_string());
            }
        }
    }
    out
}

/// Steiner-tree approximation: connect all terminals with shortest paths on the join graph.
pub(crate) fn steiner_nodes(
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

    let mut terms: Vec<String> = terminals.iter().cloned().collect();
    terms.sort();
    let mut included: HashSet<String> = HashSet::from([terms[0].clone()]);
    let mut remaining: Vec<String> = terms.into_iter().skip(1).collect();
    while !remaining.is_empty() {
        let mut best_idx = None;
        let mut best_path: Option<Vec<String>> = None;
        let mut sorted_inc: Vec<&String> = included.iter().collect();
        sorted_inc.sort();
        for (idx, target) in remaining.iter().enumerate() {
            if included.contains(target) {
                best_idx = Some(idx);
                best_path = Some(Vec::new());
                break;
            }
            for start in &sorted_inc {
                if let Some(path) = shortest_path(&adj, start, target) {
                    if best_path.as_ref().is_none_or(|bp| path.len() < bp.len()) {
                        best_path = Some(path);
                        best_idx = Some(idx);
                    }
                }
            }
        }
        if let (Some(idx), Some(path)) = (best_idx, best_path) {
            let t = remaining.remove(idx);
            included.insert(t);
            for n in path {
                included.insert(n);
            }
        } else {
            return Err(AppError::Validation(format!(
                "join graph cannot connect {:?} to {}",
                included, remaining[0]
            )));
        }
    }
    Ok(included)
}

pub(crate) fn shortest_path(
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
pub(crate) fn inline_table_ref(
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

pub(crate) fn source_name(src: &Value) -> Result<&str, AppError> {
    if let Some(s) = src.as_str() {
        return Ok(s);
    }
    src.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("source name missing".into()))
}

pub(crate) fn pin_view_mappings_to_included(
    view: &Value,
    req_cols: &HashSet<String>,
) -> Result<Value, AppError> {
    if view.get("combine").and_then(|v| v.as_str()) != Some("join") {
        return Ok(view.clone());
    }
    let inc = included_tables_for_join_view(view, Some(req_cols))?;
    let mut cloned = view.clone();
    if let Some(cols) = cloned.get_mut("columns").and_then(|v| v.as_array_mut()) {
        for col in cols {
            if let Some(maps) = col.get_mut("mappings").and_then(|v| v.as_array_mut()) {
                if maps.len() > 1
                    && maps.iter().any(|m| {
                        m.get("source")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| inc.contains(s))
                    })
                {
                    maps.sort_by_key(|m| {
                        let in_inc = m
                            .get("source")
                            .and_then(|v| v.as_str())
                            .is_some_and(|s| inc.contains(s));
                        !in_inc
                    });
                }
            }
        }
    }
    Ok(cloned)
}

pub(crate) fn view_physical_expr(view: &Value, view_col: &str) -> Result<String, AppError> {
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
        .and_then(|a| a.first())
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

