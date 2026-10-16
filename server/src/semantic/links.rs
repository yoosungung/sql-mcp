use std::collections::{HashMap, HashSet};

use serde_json::Value;

use crate::error::AppError;

/// Validate referential links across the full registration set (ARCHITECTURE §1).
pub fn validate_links(docs: &[Value]) -> Result<(), AppError> {
    let mut by_kind: HashMap<&str, HashMap<String, &Value>> = HashMap::new();
    for doc in docs {
        let kind = doc
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("document missing kind".into()))?;
        let name = doc
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("document missing name".into()))?
            .to_string();
        let slot = by_kind.entry(kind).or_default();
        if slot.insert(name.clone(), doc).is_some() {
            return Err(AppError::Validation(format!(
                "duplicate {kind}/{name}"
            )));
        }
    }

    let tables = by_kind.get("table").cloned().unwrap_or_default();
    let views = by_kind.get("view").cloned().unwrap_or_default();
    let cubes = by_kind.get("cube").cloned().unwrap_or_default();
    let metrics = by_kind.get("metric").cloned().unwrap_or_default();
    let segments = by_kind.get("segment").cloned().unwrap_or_default();

    for (name, table) in &tables {
        let table_target = doc_target(table).ok_or_else(|| {
            AppError::Validation(format!("table {name}: target missing"))
        })?;
        if let Some(fks) = table.get("fk").and_then(|v| v.as_array()) {
            for fk in fks {
                let ref_table = fk
                    .pointer("/references/table")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        AppError::Validation(format!("table {name}: fk.references.table missing"))
                    })?;
                if !tables.contains_key(ref_table) {
                    return Err(AppError::Validation(format!(
                        "table {name}: fk references unknown table {ref_table}"
                    )));
                }
                let ref_target = doc_target(tables[ref_table]).ok_or_else(|| {
                    AppError::Validation(format!("table {ref_table}: target missing"))
                })?;
                if ref_target != table_target {
                    return Err(AppError::Validation(format!(
                        "table {name}: fk target {table_target} != referenced table {ref_table} target {ref_target}"
                    )));
                }
            }
        }
    }

    for (name, view) in &views {
        let view_target = doc_target(view).ok_or_else(|| {
            AppError::Validation(format!("view {name}: target missing"))
        })?;
        let combine = view
            .get("combine")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("view {name}: combine missing")))?;
        let sources = view
            .get("sources")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AppError::Validation(format!("view {name}: sources missing")))?;
        let mut source_names: HashSet<String> = HashSet::new();
        for src in sources {
            let src_name = source_name(src).ok_or_else(|| {
                AppError::Validation(format!("view {name}: source name missing"))
            })?;
            let ok = match combine {
                "select" => views.contains_key(src_name),
                _ => tables.contains_key(src_name),
            };
            if !ok {
                return Err(AppError::Validation(format!(
                    "view {name}: unknown source {src_name} for combine={combine}"
                )));
            }
            source_names.insert(src_name.to_string());
        }
        let leaf_targets =
            leaf_table_targets(view, &tables, &views, &mut HashSet::new())?;
        for lt in &leaf_targets {
            if *lt != view_target {
                return Err(AppError::Validation(format!(
                    "view {name}: target {view_target} != leaf table target {lt}"
                )));
            }
        }
        if let Some(cols) = view.get("columns").and_then(|v| v.as_array()) {
            for col in cols {
                if let Some(mappings) = col.get("mappings").and_then(|v| v.as_array()) {
                    for m in mappings {
                        let src = m
                            .get("source")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                AppError::Validation(format!(
                                    "view {name}: mapping.source missing"
                                ))
                            })?;
                        if !source_names.contains(src) {
                            return Err(AppError::Validation(format!(
                                "view {name}: mapping source {src} not in sources"
                            )));
                        }
                    }
                }
            }
        }
    }

    for (name, cube) in &cubes {
        let cube_target = doc_target(cube).ok_or_else(|| {
            AppError::Validation(format!("cube {name}: target missing"))
        })?;
        let view_name = cube
            .get("view")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("cube {name}: view missing")))?;
        if !views.contains_key(view_name) {
            return Err(AppError::Validation(format!(
                "cube {name}: unknown view {view_name}"
            )));
        }
        let view = views[view_name];
        let view_target = doc_target(view).ok_or_else(|| {
            AppError::Validation(format!("view {view_name}: target missing"))
        })?;
        if cube_target != view_target {
            return Err(AppError::Validation(format!(
                "cube {name}: target {cube_target} != view {view_name} target {view_target}"
            )));
        }
        let view_cols = view_column_names(view);
        let mut dim_fact_names: HashSet<String> = HashSet::new();
        for key in ["dimensions", "facts"] {
            if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
                for item in items {
                    let iname = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            AppError::Validation(format!("cube {name}: {key} item missing name"))
                        })?;
                    if !dim_fact_names.insert(iname.to_string()) {
                        return Err(AppError::Validation(format!(
                            "cube {name}: duplicate dimension/fact name {iname}"
                        )));
                    }
                    let has_col = item.get("column").and_then(|v| v.as_str()).is_some();
                    let has_expr = item.get("expression").and_then(|v| v.as_str()).is_some();
                    if has_col == has_expr {
                        return Err(AppError::Validation(format!(
                            "cube {name}.{iname}: exactly one of column|expression required"
                        )));
                    }
                    if let Some(col) = item.get("column").and_then(|v| v.as_str()) {
                        if !view_cols.contains(col) {
                            return Err(AppError::Validation(format!(
                                "cube {name}.{iname}: column {col} not in view {view_name}"
                            )));
                        }
                    }
                    if key == "dimensions" {
                        validate_dim_as_code(name, cube_target, iname, item, &tables)?;
                    } else if item.get("as_code").is_some() {
                        return Err(AppError::Validation(format!(
                            "cube {name}.{iname}: as_code is only allowed on dimensions"
                        )));
                    }
                }
            }
        }
        // stash for metric name clash checks
        let _ = dim_fact_names;
    }

    for (name, metric) in &metrics {
        let cube_name = metric
            .get("cube")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("metric {name}: cube missing")))?;
        let cube = cubes.get(cube_name).ok_or_else(|| {
            AppError::Validation(format!("metric {name}: unknown cube {cube_name}"))
        })?;
        let clash = dim_fact_name_set(cube);
        if clash.contains(name) {
            return Err(AppError::Validation(format!(
                "metric {name}: name collides with dimension/fact on cube {cube_name}"
            )));
        }
        let aggregation = metric
            .get("aggregation")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if aggregation == "group" {
            let view_name = cube.get("view").and_then(|v| v.as_str()).unwrap_or("");
            let view = views.get(view_name).ok_or_else(|| {
                AppError::Validation(format!("metric {name}: cube view missing"))
            })?;
            let view_cols = view_column_names(view);
            let group_by = metric
                .get("group_by")
                .and_then(|v| v.as_array())
                .ok_or_else(|| {
                    AppError::Validation(format!("metric {name}: group_by required for aggregation=group"))
                })?;
            for g in group_by {
                let gname = g.as_str().ok_or_else(|| {
                    AppError::Validation(format!("metric {name}: group_by entry must be string"))
                })?;
                if !view_cols.contains(gname) {
                    return Err(AppError::Validation(format!(
                        "metric {name}: group_by {gname} not in view columns"
                    )));
                }
            }
        }
    }

    for (name, segment) in &segments {
        let cube_name = segment
            .get("cube")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("segment {name}: cube missing")))?;
        if !cubes.contains_key(cube_name) {
            return Err(AppError::Validation(format!(
                "segment {name}: unknown cube {cube_name}"
            )));
        }
    }

    Ok(())
}

fn validate_dim_as_code(
    cube: &str,
    cube_target: &str,
    dim: &str,
    item: &Value,
    tables: &HashMap<String, &Value>,
) -> Result<(), AppError> {
    let Some(as_code) = item.get("as_code") else {
        return Ok(());
    };
    if as_code.is_null() {
        return Ok(());
    }
    if as_code.as_bool().is_some() {
        return Err(AppError::Validation(format!(
            "cube {cube}.{dim}: as_code must be an object, not boolean"
        )));
    }
    let code_table = as_code
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Validation(format!("cube {cube}.{dim}: as_code.table missing"))
        })?;
    let code_doc = tables.get(code_table).ok_or_else(|| {
        AppError::Validation(format!(
            "cube {cube}.{dim}: as_code unknown table {code_table}"
        ))
    })?;
    let code_target = doc_target(code_doc).ok_or_else(|| {
        AppError::Validation(format!("table {code_table}: target missing"))
    })?;
    if code_target != cube_target {
        return Err(AppError::Validation(format!(
            "cube {cube}.{dim}: as_code table {code_table} target {code_target} != cube target {cube_target}"
        )));
    }
    let code_cols = table_column_names(code_doc);
    for key in ["key_column", "value_column"] {
        let cname = as_code.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
            AppError::Validation(format!("cube {cube}.{dim}: as_code.{key} missing"))
        })?;
        if !code_cols.contains(cname) {
            return Err(AppError::Validation(format!(
                "cube {cube}.{dim}: as_code.{key} {cname} not in {code_table}"
            )));
        }
    }
    Ok(())
}

fn doc_target(doc: &Value) -> Option<&str> {
    doc.get("target").and_then(|v| v.as_str())
}

/// Collect distinct leaf table `target` values under a view (follows select→view).
fn leaf_table_targets<'a>(
    view: &'a Value,
    tables: &HashMap<String, &'a Value>,
    views: &HashMap<String, &'a Value>,
    visiting: &mut HashSet<String>,
) -> Result<HashSet<&'a str>, AppError> {
    let name = view
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    if !visiting.insert(name.to_string()) {
        return Err(AppError::Validation(format!(
            "view {name}: cyclic sources"
        )));
    }
    let combine = view
        .get("combine")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let sources = view
        .get("sources")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut out = HashSet::new();
    for src in &sources {
        let src_name = source_name(src).unwrap_or("");
        if combine == "select" {
            let child = views.get(src_name).ok_or_else(|| {
                AppError::Validation(format!("view {name}: unknown source view {src_name}"))
            })?;
            out.extend(leaf_table_targets(child, tables, views, visiting)?);
        } else {
            let table = tables.get(src_name).ok_or_else(|| {
                AppError::Validation(format!("view {name}: unknown source table {src_name}"))
            })?;
            let t = doc_target(table).ok_or_else(|| {
                AppError::Validation(format!("table {src_name}: target missing"))
            })?;
            out.insert(t);
        }
    }
    visiting.remove(name);
    Ok(out)
}

fn source_name(src: &Value) -> Option<&str> {
    if let Some(s) = src.as_str() {
        return Some(s);
    }
    src.get("name").and_then(|v| v.as_str())
}

fn table_column_names(table: &Value) -> HashSet<String> {
    table
        .get("columns")
        .and_then(|v| v.as_array())
        .map(|cols| {
            cols.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn view_column_names(view: &Value) -> HashSet<String> {
    table_column_names(view)
}

fn dim_fact_name_set(cube: &Value) -> HashSet<String> {
    let mut set = HashSet::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    set.insert(n.to_string());
                }
            }
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn load_examples() -> Vec<Value> {
        [
            include_str!("../../../semantic/examples/tables/customer.json"),
            include_str!("../../../semantic/examples/tables/orders.json"),
            include_str!("../../../semantic/examples/tables/orders_month.json"),
            include_str!("../../../semantic/examples/tables/region_codes.json"),
            include_str!("../../../semantic/examples/tables/status_codes.json"),
            include_str!("../../../semantic/examples/views/orders_customer.json"),
            include_str!("../../../semantic/examples/cubes/sales.json"),
            include_str!("../../../semantic/examples/metrics/revenue.json"),
            include_str!("../../../semantic/examples/segments/paid.json"),
        ]
        .into_iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
    }

    #[test]
    fn example_set_links_ok() {
        validate_links(&load_examples()).unwrap();
    }

    #[test]
    fn rejects_unknown_fk() {
        let docs = vec![json!({
            "kind": "table",
            "name": "orders",
            "target": "dw",
            "db": "dw",
            "schema": "sales",
            "table": "orders",
            "columns": [{"name": "id", "type": "bigint", "nullable": false}],
            "pk": ["id"],
            "fk": [{"columns": ["customer_id"], "references": {"table": "missing", "columns": ["id"]}}],
            "permission_filters": [],
            "scope_filter": null,
            "default_filter": null
        })];
        assert!(matches!(
            validate_links(&docs),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn rejects_mismatched_view_target() {
        let mut docs = load_examples();
        for d in &mut docs {
            if d.get("kind").and_then(|v| v.as_str()) == Some("view") {
                d["target"] = json!("other");
            }
        }
        assert!(matches!(
            validate_links(&docs),
            Err(AppError::Validation(_))
        ));
    }
}
