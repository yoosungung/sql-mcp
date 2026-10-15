use serde_json::Value;

use crate::error::AppError;
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
    let mut filters = Vec::new();
    if let Some(expr) = filter_expr(table_doc.get("scope_filter")) {
        filters.push(format!("({expr})"));
    }
    if let Some(expr) = filter_expr(table_doc.get("default_filter")) {
        filters.push(format!("({expr})"));
    }
    if let Some(role) = role {
        if let Some(pfs) = table_doc.get("permission_filters").and_then(|v| v.as_array()) {
            for pf in pfs {
                if pf.get("role").and_then(|v| v.as_str()) == Some(role) {
                    if let Some(expr) = pf.get("expression").and_then(|v| v.as_str()) {
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

/// Compile a join/union view into a FROM clause with aliases = logical table names.
pub fn compile_view_from(docs: &[Value], view: &Value) -> Result<String, AppError> {
    let combine = view
        .get("combine")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("view.combine required".into()))?;
    if combine != "join" {
        return Err(AppError::Validation(format!(
            "combine={combine} not yet supported in compile"
        )));
    }
    let sources = view
        .get("sources")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("view.sources required".into()))?;
    if sources.is_empty() {
        return Err(AppError::Validation("view.sources empty".into()));
    }
    let first_name = source_name(&sources[0])?;
    let first_table = find_doc(docs, "table", first_name)?;
    let mut sql = format!("{} AS {}", physical_from(first_table)?, quote_ident(first_name)?);
    let joins = view
        .get("joins")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for j in joins {
        let right = j
            .get("right")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("join.right required".into()))?;
        let left = j
            .get("left")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("join.left required".into()))?;
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
        let right_table = find_doc(docs, "table", right)?;
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
        sql.push_str(&format!(
            " {join_kw} {} AS {} ON {}",
            physical_from(right_table)?,
            quote_ident(right)?,
            ons.join(" AND ")
        ));
    }
    Ok(sql)
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
    let view_name = cube
        .get("view")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("cube.view required".into()))?;
    let view = find_doc(docs, "view", view_name)?;
    let from = compile_view_from(docs, view)?;

    let mut select_parts = Vec::new();
    let group_by = metric
        .get("group_by")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for g in &group_by {
        let gname = g
            .as_str()
            .ok_or_else(|| AppError::Validation("group_by entry".into()))?;
        let expr = view_physical_expr(view, gname)?;
        select_parts.push(format!("{expr} AS {}", quote_ident(gname)?));
    }
    let metric_expr = metric
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("metric.expression required".into()))?;
    // Rewrite simple fact names inside metric expression via cube_field_sql when possible.
    let rewritten = rewrite_metric_expression(metric_expr, cube, view)?;
    select_parts.push(format!("{rewritten} AS {}", quote_ident(metric_name)?));

    let mut filters = Vec::new();
    // Table filters for sources
    if let Some(sources) = view.get("sources").and_then(|v| v.as_array()) {
        for src in sources {
            let tname = source_name(src)?;
            let table = find_doc(docs, "table", tname)?;
            if let Some(expr) = filter_expr(table.get("scope_filter")) {
                filters.push(qualify_filter(expr, tname)?);
            }
            if let Some(expr) = filter_expr(table.get("default_filter")) {
                filters.push(qualify_filter(expr, tname)?);
            }
            if let Some(role) = role {
                if let Some(pfs) = table.get("permission_filters").and_then(|v| v.as_array()) {
                    for pf in pfs {
                        if pf.get("role").and_then(|v| v.as_str()) == Some(role) {
                            if let Some(expr) = pf.get("expression").and_then(|v| v.as_str()) {
                                filters.push(qualify_filter(expr, tname)?);
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(expr) = filter_expr(metric.get("filter")) {
        filters.push(format!("({expr})"));
    }
    for seg_name in segment_names {
        let seg = find_doc(docs, "segment", seg_name)?;
        if seg.get("cube").and_then(|v| v.as_str()) != Some(cube_name) {
            return Err(AppError::Validation(format!(
                "segment {seg_name} not on cube {cube_name}"
            )));
        }
        if let Some(expr) = filter_expr(seg.get("filter")) {
            // segment filters use cube/view column names — map via view_physical when simple ident
            filters.push(format!("({})", rewrite_view_idents(expr, view)?));
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
            gb.push(view_physical_expr(view, gname)?);
        }
        sql.push_str(" GROUP BY ");
        sql.push_str(&gb.join(", "));
    }
    assert_select_only(&sql)?;
    Ok(sql)
}

fn rewrite_metric_expression(expr: &str, cube: &Value, view: &Value) -> Result<String, AppError> {
    // Replace bare fact/dimension identifiers that appear as whole words.
    let mut out = expr.to_string();
    for key in ["facts", "dimensions"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let phys = if let Some(e) = item.get("expression").and_then(|v| v.as_str()) {
                    format!("({e})")
                } else if let Some(col) = item.get("column").and_then(|v| v.as_str()) {
                    view_physical_expr(view, col)?
                } else {
                    continue;
                };
                out = replace_ident(&out, name, &phys);
            }
        }
    }
    Ok(out)
}

fn rewrite_view_idents(expr: &str, view: &Value) -> Result<String, AppError> {
    let mut out = expr.to_string();
    if let Some(cols) = view.get("columns").and_then(|v| v.as_array()) {
        for c in cols {
            let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() {
                continue;
            }
            let phys = view_physical_expr(view, name)?;
            out = replace_ident(&out, name, &phys);
        }
    }
    Ok(out)
}

fn replace_ident(hay: &str, ident: &str, with: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = hay.chars().collect();
    let id: Vec<char> = ident.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + id.len() <= chars.len() && chars[i..i + id.len()] == id[..] {
            let before_ok = i == 0 || !is_ident_char(chars[i - 1]);
            let after_ok = i + id.len() == chars.len() || !is_ident_char(chars[i + id.len()]);
            if before_ok && after_ok {
                out.push_str(with);
                i += id.len();
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn qualify_filter(expr: &str, alias: &str) -> Result<String, AppError> {
    // Prefix simple identifiers with alias. Keep bind params (:x) and string literals intact.
    let mut out = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\'' {
            out.push('\'');
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\'' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if chars[i] == ':' {
            out.push(':');
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let upper = word.to_ascii_uppercase();
            if matches!(
                upper.as_str(),
                "AND" | "OR" | "NOT" | "NULL" | "IS" | "IN" | "LIKE" | "BETWEEN" | "TRUE" | "FALSE"
            ) {
                out.push_str(&word);
            } else {
                out.push_str(&format!("{}.{}", quote_ident(alias)?, quote_ident(&word)?));
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(format!("({out})"))
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
        assert!(sql.contains("INNER JOIN"));
        assert!(sql.contains("sum("));
        assert!(sql.contains("GROUP BY"));
        assert!(sql.contains("PAID"));
        assert_select_only(&sql).unwrap();
    }
}
