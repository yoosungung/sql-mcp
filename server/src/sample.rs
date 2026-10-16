//! Resolve cube member → physical sample target (table column or as_code dictionary).

use serde_json::{json, Value};

use crate::compile::{physical_from, resolve_dim_as_code};
use crate::dialect::DialectProfile;
use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct SampleTarget {
    pub schema: String,
    pub table: String,
    pub column: String,
    /// Optional second column (as_code value) sampled alongside key.
    pub value_column: Option<String>,
    pub where_extra: Option<String>,
    pub resolved: Value,
    pub from_as_code: bool,
}

/// Resolve `cube` + dim/fact `member` to a physical sample target.
pub fn resolve_sample_target(
    docs: &[Value],
    cube_name: &str,
    member: &str,
) -> Result<SampleTarget, AppError> {
    let cube = find_doc(docs, "cube", cube_name)?;
    let view_name = cube
        .get("view")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation(format!("cube {cube_name}: view missing")))?;
    let view = find_doc(docs, "view", view_name)?;

    if let Some((code_table_name, as_code)) = resolve_dim_as_code(docs, cube, view, member)? {
        let code_doc = find_doc(docs, "table", &code_table_name)?;
        let key = as_code
            .get("key_column")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("as_code.key_column".into()))?;
        let value = as_code
            .get("value_column")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation("as_code.value_column".into()))?;
        let schema = str_field(code_doc, "schema")?;
        let table = str_field(code_doc, "table")?;
        return Ok(SampleTarget {
            schema: schema.to_string(),
            table: table.to_string(),
            column: key.to_string(),
            value_column: Some(value.to_string()),
            where_extra: as_code
                .get("where")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            resolved: json!({
                "cube": cube_name,
                "member": member,
                "source": code_table_name,
                "db": code_doc.get("db"),
                "schema": schema,
                "table": table,
                "key_column": key,
                "value_column": value,
                "as_code": true,
            }),
            from_as_code: true,
        });
    }

    let member_doc = find_cube_member(cube, member).ok_or_else(|| {
        AppError::NotFound(format!("cube {cube_name} has no member {member}"))
    })?;
    if member_doc.get("expression").is_some() && member_doc.get("column").is_none() {
        return resolve_expression_sample(docs, cube_name, view, member_doc, member);
    }
    let view_col = member_doc
        .get("column")
        .and_then(|v| v.as_str())
        .unwrap_or(member);
    let mapping = view_first_mapping(view, view_col)?.ok_or_else(|| {
        AppError::Validation(format!("view {view_name}: no mapping for {view_col}"))
    })?;
    let source = mapping
        .get("source")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("mapping.source".into()))?;
    let phys_col = mapping
        .get("column")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("mapping.column".into()))?;
    let table_doc = find_doc(docs, "table", source)?;
    let schema = str_field(table_doc, "schema")?;
    let table = str_field(table_doc, "table")?;
    Ok(SampleTarget {
        schema: schema.to_string(),
        table: table.to_string(),
        column: phys_col.to_string(),
        value_column: None,
        where_extra: None,
        resolved: json!({
            "cube": cube_name,
            "member": member,
            "source": source,
            "db": table_doc.get("db"),
            "schema": schema,
            "table": table,
            "column": phys_col,
            "as_code": false,
        }),
        from_as_code: false,
    })
}

fn resolve_expression_sample(
    docs: &[Value],
    cube_name: &str,
    view: &Value,
    member_doc: &Value,
    member: &str,
) -> Result<SampleTarget, AppError> {
    let expr = member_doc
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("expression missing".into()))?;
    // Collect simple idents from expression and require single physical source table.
    let mut sources: Vec<String> = Vec::new();
    for tok in expr.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
        if tok.is_empty()
            || matches!(
                tok,
                "date_trunc" | "year" | "month" | "sum" | "count" | "avg" | "day"
            )
        {
            continue;
        }
        if let Ok(Some(m)) = view_first_mapping(view, tok) {
            if let Some(src) = m.get("source").and_then(|v| v.as_str()) {
                if !sources.iter().any(|s| s == src) {
                    sources.push(src.to_string());
                }
            }
        }
    }
    if sources.len() != 1 {
        return Err(AppError::Validation(format!(
            "expression member {member} spans multiple tables; use execute_sql"
        )));
    }
    let source = &sources[0];
    let table_doc = find_doc(docs, "table", source)?;
    let schema = str_field(table_doc, "schema")?;
    let table = str_field(table_doc, "table")?;
    // Sample the first mapped physical column referenced in the expression.
    let phys_col = expr
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .find_map(|tok| {
            view_first_mapping(view, tok)
                .ok()
                .flatten()
                .and_then(|m| m.get("column").and_then(|v| v.as_str()).map(str::to_string))
        })
        .ok_or_else(|| {
            AppError::Validation(format!("cannot resolve physical column for {member}"))
        })?;
    let _ = physical_from(table_doc)?;
    Ok(SampleTarget {
        schema: schema.to_string(),
        table: table.to_string(),
        column: phys_col.clone(),
        value_column: None,
        where_extra: None,
        resolved: json!({
            "cube": cube_name,
            "member": member,
            "source": source,
            "db": table_doc.get("db"),
            "schema": schema,
            "table": table,
            "column": phys_col,
            "expression": expr,
            "as_code": false,
        }),
        from_as_code: false,
    })
}

fn find_doc<'a>(docs: &'a [Value], kind: &str, name: &str) -> Result<&'a Value, AppError> {
    docs.iter()
        .find(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some(kind)
                && d.get("name").and_then(|v| v.as_str()) == Some(name)
        })
        .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))
}

fn find_cube_member<'a>(cube: &'a Value, name: &str) -> Option<&'a Value> {
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if item.get("name").and_then(|v| v.as_str()) == Some(name) {
                    return Some(item);
                }
            }
        }
    }
    None
}

fn view_first_mapping<'a>(view: &'a Value, view_col: &str) -> Result<Option<&'a Value>, AppError> {
    let cols = view
        .get("columns")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation("view.columns required".into()))?;
    let Some(col) = cols
        .iter()
        .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(view_col))
    else {
        return Ok(None);
    };
    Ok(col
        .get("mappings")
        .and_then(|v| v.as_array())
        .and_then(|m| m.first()))
}

fn str_field<'a>(doc: &'a Value, key: &str) -> Result<&'a str, AppError> {
    doc.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation(format!("{key} required")))
}

/// Build SELECT for sampling; used when value_column / where need custom SQL.
/// Identifiers use `dialect` quoting (D21). Row limit is applied by `TargetDb::execute_select`.
pub fn sample_sql(
    target: &SampleTarget,
    _limit: usize,
    dialect: DialectProfile,
) -> Result<String, AppError> {
    let schema = dialect.quote_ident(&target.schema)?;
    let table = dialect.quote_ident(&target.table)?;
    let col = dialect.quote_ident(&target.column)?;
    let mut sql = if let Some(ref vc) = target.value_column {
        let vq = dialect.quote_ident(vc)?;
        format!("SELECT {col} AS k, {vq} AS v FROM {schema}.{table} WHERE {col} IS NOT NULL")
    } else {
        format!("SELECT {col} AS v FROM {schema}.{table} WHERE {col} IS NOT NULL")
    };
    if let Some(ref w) = target.where_extra {
        sql.push_str(" AND (");
        sql.push_str(w);
        sql.push(')');
    }
    // where_extra / expressions may contain canonical date_trunc.
    if sql.to_ascii_lowercase().contains("date_trunc") {
        dialect.rewrite_date_trunc(&sql)
    } else {
        Ok(sql)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn resolves_region_to_code_dict() {
        let docs = example_docs();
        let t = resolve_sample_target(&docs, "sales", "region_code").unwrap();
        assert!(t.from_as_code);
        assert_eq!(t.table, "region_codes");
        assert_eq!(t.column, "code");
        assert_eq!(t.value_column.as_deref(), Some("name"));
        assert!(t.where_extra.as_deref().unwrap().contains("lang"));
    }

    #[test]
    fn resolves_sale_amount_to_orders() {
        let docs = example_docs();
        let t = resolve_sample_target(&docs, "sales", "sale_amount").unwrap();
        assert!(!t.from_as_code);
        assert_eq!(t.table, "orders");
        assert_eq!(t.column, "sale_amount");
    }

    #[test]
    fn sample_sql_uses_dialect_quotes() {
        let t = SampleTarget {
            schema: "sales".into(),
            table: "orders".into(),
            column: "sale_amount".into(),
            value_column: None,
            where_extra: None,
            resolved: json!({}),
            from_as_code: false,
        };
        let pg = sample_sql(&t, 10, DialectProfile::Postgres).unwrap();
        assert!(pg.contains("\"sale_amount\""));
        assert!(pg.contains("\"sales\".\"orders\""));
        let my = sample_sql(&t, 10, DialectProfile::Mysql).unwrap();
        assert!(my.contains("`sale_amount`"));
        assert!(my.contains("`sales`.`orders`"));
        assert!(!my.contains('"'));
    }
}
