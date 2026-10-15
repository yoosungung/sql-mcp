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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
