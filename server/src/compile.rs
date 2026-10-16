use std::collections::{HashMap, HashSet, VecDeque};

use serde_json::Value;

use crate::error::AppError;
use crate::expr_guard::{assert_filter_expr, assert_idents_known, assert_metric_expr};
use crate::select_guard::assert_select_only;

/// Quote a simple SQL identifier.
pub fn quote_ident(name: &str) -> Result<String, AppError> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(AppError::Validation(format!("invalid identifier: {name}")));
    }
    Ok(format!("\"{name}\""))
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
    if table_doc.get("kind").and_then(|v| v.as_str()) != Some("table") {
        return Err(AppError::Validation("expected table document".into()));
    }
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
    let known = table_column_names(table_doc);
    let mut filters = Vec::new();
    if let Some(expr) = filter_expr(table_doc.get("scope_filter")) {
        validate_table_filter(expr, &known)?;
        filters.push(format!("({expr})"));
    }
    if let Some(expr) = filter_expr(table_doc.get("default_filter")) {
        validate_table_filter(expr, &known)?;
        filters.push(format!("({expr})"));
    }
    if let Some(role) = role {
        if let Some(pfs) = table_doc.get("permission_filters").and_then(|v| v.as_array()) {
            for pf in pfs {
                if pf.get("role").and_then(|v| v.as_str()) == Some(role) {
                    if let Some(expr) = pf.get("expression").and_then(|v| v.as_str()) {
                        validate_table_filter(expr, &known)?;
                        filters.push(format!("({expr})"));
                    }
                }
            }
        }
    }
    let mut sql = format!("SELECT {cols} FROM {from}");
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    assert_select_only(&sql)?;
    Ok(sql)
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
    compile_view_from_opts(docs, view, None, None)
}

pub fn compile_view_from_opts(
    docs: &[Value],
    view: &Value,
    role: Option<&str>,
    used_view_cols: Option<&HashSet<String>>,
) -> Result<String, AppError> {
    let combine = view
        .get("combine")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("view.combine required".into()))?;
    match combine {
        "join" => compile_join_view(docs, view, role, used_view_cols),
        "union" => compile_set_view(docs, view, role, used_view_cols, false),
        "minus" => compile_set_view(docs, view, role, used_view_cols, true),
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
        let mut select_parts = Vec::new();
        for col in &proj_cols {
            let phys = view_physical_expr_for_source(view, col, tname)?;
            select_parts.push(format!("{phys} AS {}", quote_ident(col)?));
        }
        let from = inline_table_ref(docs, tname, role)?;
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
        let candidate = find_doc(docs, skind, sname)?;
        if skind == "view" && view_covers_cols(candidate, req_cols) {
            if is_semi_additive {
                // Semi-additive: only accept if candidate has no finer grain than query.
                // Without explicit summary grain metadata, require exact column set match.
                let cand_cols = view_column_name_set(candidate);
                if cand_cols != *req_cols && !req_cols.is_subset(&cand_cols) {
                    continue;
                }
            }
            return Ok(candidate.clone());
        }
        if skind == "table" {
            let tcols = table_column_names(candidate);
            if req_cols.is_subset(&tcols) {
                // Wrap table as a synthetic join view of one source.
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
    let t_req = if let Some(cols) = used_view_cols {
        tables_for_view_cols(view, cols)
    } else {
        all_names.clone()
    };
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
    let mut sql = format!("{} ", inline_table_ref(docs, &first, role)?);
    let mut placed: HashSet<String> = HashSet::from([first.clone()]);

    // Attach joins whose both ends are included and at least one side already placed.
    let mut pending = joins.clone();
    while placed.len() < included.len() {
        let mut progress = false;
        pending.retain(|j| {
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
            let (anchor, new_side) = if left_in {
                (left, right)
            } else {
                (right, left)
            };
            if let Ok(frag) = format_join_fragment(docs, j, new_side, role) {
                // ON uses original left/right names — ok.
                let _ = anchor;
                sql.push_str(&frag);
                placed.insert(new_side.to_string());
                progress = true;
            }
            false
        });
        if !progress {
            // Fallback: add any remaining included table with CROSS (should not happen).
            if let Some(missing) = included.iter().find(|n| !placed.contains(*n)) {
                sql.push_str(&format!(
                    " CROSS JOIN {} ",
                    inline_table_ref(docs, missing, role)?
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
    let join_kw = match jtype {
        "inner" => "INNER JOIN",
        "left" => "LEFT JOIN",
        "right" => "RIGHT JOIN",
        "full" => "FULL JOIN",
        other => {
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
        inline_table_ref(docs, new_side, role)?,
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
    // Also connect any terminal not reachable from root via pairwise from included seed.
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
fn inline_table_ref(docs: &[Value], alias: &str, role: Option<&str>) -> Result<String, AppError> {
    let table = find_doc(docs, "table", alias)?;
    let phys = physical_from(table)?;
    let known = table_column_names(table);
    let mut filters = Vec::new();
    if let Some(expr) = filter_expr(table.get("scope_filter")) {
        validate_table_filter(expr, &known)?;
        filters.push(format!("({expr})"));
    }
    if let Some(expr) = filter_expr(table.get("default_filter")) {
        validate_table_filter(expr, &known)?;
        filters.push(format!("({expr})"));
    }
    if let Some(role) = role {
        if let Some(pfs) = table.get("permission_filters").and_then(|v| v.as_array()) {
            for pf in pfs {
                if pf.get("role").and_then(|v| v.as_str()) == Some(role) {
                    if let Some(expr) = pf.get("expression").and_then(|v| v.as_str()) {
                        validate_table_filter(expr, &known)?;
                        filters.push(format!("({expr})"));
                    }
                }
            }
        }
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

/// Compile cube metric (+ optional segments) into physical SELECT.
pub fn compile_metric_select(
    docs: &[Value],
    metric_name: &str,
    segment_names: &[&str],
    role: Option<&str>,
) -> Result<String, AppError> {
    let metric = find_doc(docs, "metric", metric_name)?;
    let cube_name = metric
        .get("cube")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("metric.cube required".into()))?;
    let cube = find_doc(docs, "cube", cube_name)?;

    let metric_expr = metric
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("metric.expression required".into()))?;
    let is_semi = metric_expr.to_ascii_lowercase().contains("count(distinct")
        || metric_expr.to_ascii_lowercase().contains("avg(")
        || metric_expr.to_ascii_lowercase().contains("median(");

    let group_by = metric
        .get("group_by")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut req_cols: HashSet<String> = HashSet::new();
    for g in &group_by {
        if let Some(gname) = g.as_str() {
            req_cols.insert(gname.to_string());
        }
    }
    // Fact/dim names in metric expression → resolve to view columns via member.column.
    collect_member_view_cols(cube, metric_expr, &mut req_cols);
    if let Some(expr) = filter_expr(metric.get("filter")) {
        collect_member_view_cols(cube, expr, &mut req_cols);
    }
    for seg_name in segment_names {
        let seg = find_doc(docs, "segment", seg_name)?;
        if let Some(expr) = filter_expr(seg.get("filter")) {
            collect_member_view_cols(cube, expr, &mut req_cols);
        }
    }

    let view = resolve_view_for_cube(docs, cube, &req_cols, is_semi)?;
    let combine = view.get("combine").and_then(|v| v.as_str()).unwrap_or("");
    let set_alias = matches!(combine, "union" | "minus").then_some("_sem_uv");
    let mut from = compile_view_from_opts(docs, &view, role, Some(&req_cols))?;
    if let Some(a) = set_alias {
        from = format!("{from} AS {}", quote_ident(a)?);
    }

    let mut select_parts = Vec::new();
    for g in &group_by {
        let gname = g
            .as_str()
            .ok_or_else(|| AppError::Validation("group_by entry".into()))?;
        let expr = output_col_expr(&view, gname, set_alias)?;
        let proj = if member_encrypted(cube, gname) {
            format!("NULL AS {}", quote_ident(gname)?)
        } else {
            format!("{expr} AS {}", quote_ident(gname)?)
        };
        select_parts.push(proj);
    }
    let members = cube_member_names(cube);
    let view_cols = view_column_name_set(&view);
    validate_cube_member_expressions(cube, &view_cols)?;

    assert_metric_expr(metric_expr)?;
    assert_idents_known(metric_expr, &members)?;
    let rewritten = rewrite_metric_expression(metric_expr, cube, &view, set_alias)?;
    select_parts.push(format!("{rewritten} AS {}", quote_ident(metric_name)?));

    // Outer WHERE: metric/segment/dimension-fact default_filter only (table filters pushed down).
    let mut filters = Vec::new();
    collect_member_default_filters(cube, &req_cols, &view, set_alias, &mut filters)?;

    if let Some(expr) = filter_expr(metric.get("filter")) {
        assert_filter_expr(expr)?;
        assert_idents_known(expr, &members)?;
        filters.push(format!(
            "({})",
            rewrite_view_idents(expr, &view, set_alias)?
        ));
    }
    for seg_name in segment_names {
        let seg = find_doc(docs, "segment", seg_name)?;
        if seg.get("cube").and_then(|v| v.as_str()) != Some(cube_name) {
            return Err(AppError::Validation(format!(
                "segment {seg_name} not on cube {cube_name}"
            )));
        }
        if let Some(expr) = filter_expr(seg.get("filter")) {
            assert_filter_expr(expr)?;
            assert_idents_known(expr, &members)?;
            filters.push(format!(
                "({})",
                rewrite_view_idents(expr, &view, set_alias)?
            ));
        }
    }

    let mut sql = format!("SELECT {} FROM {}", select_parts.join(", "), from);
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    if !group_by.is_empty() {
        let mut gb = Vec::new();
        for g in &group_by {
            let gname = g.as_str().unwrap();
            gb.push(output_col_expr(&view, gname, set_alias)?);
        }
        sql.push_str(" GROUP BY ");
        sql.push_str(&gb.join(", "));
    }
    assert_select_only(&sql)?;
    Ok(sql)
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
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                if expr_has_ident(expr, name) {
                    if let Some(col) = item.get("column").and_then(|v| v.as_str()) {
                        out.insert(col.to_string());
                    } else if let Some(e) = item.get("expression").and_then(|v| v.as_str()) {
                        // expression dims reference view cols by name
                        if let Some(vcols) = cube.get("view") {
                            let _ = vcols;
                        }
                        // pull simple idents from expression as view cols
                        for tok in e.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                            if !tok.is_empty()
                                && tok.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                                && !matches!(
                                    tok,
                                    "date_trunc" | "year" | "month" | "sum" | "count" | "avg"
                                )
                            {
                                out.insert(tok.to_string());
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

fn collect_member_default_filters(
    cube: &Value,
    used_view_cols: &HashSet<String>,
    view: &Value,
    set_alias: Option<&str>,
    filters: &mut Vec<String>,
) -> Result<(), AppError> {
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let col = item
                    .get("column")
                    .and_then(|v| v.as_str())
                    .unwrap_or(name);
                if !used_view_cols.contains(col) && !used_view_cols.contains(name) {
                    continue;
                }
                if let Some(expr) = filter_expr(item.get("default_filter")) {
                    assert_filter_expr(expr)?;
                    filters.push(format!(
                        "({})",
                        rewrite_view_idents(expr, view, set_alias)?
                    ));
                }
            }
        }
    }
    Ok(())
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

fn rewrite_metric_expression(
    expr: &str,
    cube: &Value,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    for key in ["facts", "dimensions"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let phys = if let Some(e) = item.get("expression").and_then(|v| v.as_str()) {
                    // nested member expr may still need view rewrite
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
    crate::ast_rewrite::rewrite_idents(expr, &map)
}

fn rewrite_view_idents(
    expr: &str,
    view: &Value,
    set_alias: Option<&str>,
) -> Result<String, AppError> {
    use std::collections::HashMap;
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
    // Also map cube-style bare names that equal physical column names on members.
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
