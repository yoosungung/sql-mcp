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
            }
        }
    }

    for (name, view) in &views {
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

fn source_name(src: &Value) -> Option<&str> {
    if let Some(s) = src.as_str() {
        return Some(s);
    }
    src.get("name").and_then(|v| v.as_str())
}

fn view_column_names(view: &Value) -> HashSet<String> {
    view.get("columns")
        .and_then(|v| v.as_array())
        .map(|cols| {
            cols.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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
}
