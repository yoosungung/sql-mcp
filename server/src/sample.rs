//! Resolve cube member → physical sample target (table column or as_code dictionary).

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::compile::{
    apply_binds, check_role_access, collect_table_filters, physical_from, resolve_dim_as_code,
};
use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::expr_guard::extract_idents;

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

/// Resolve `cube` + dim/fact `member` to a physical sample target (structural resolution + default/scope filters).
pub fn resolve_sample_target(
    docs: &[Value],
    cube_name: &str,
    member: &str,
) -> Result<SampleTarget, AppError> {
    resolve_sample_target_inner(docs, cube_name, member, None, None, false)
}

/// Resolve `cube` + dim/fact `member` with full RBAC `role` check, `permission_filters`, and `:bind` substitution.
pub fn resolve_sample_target_with_context(
    docs: &[Value],
    cube_name: &str,
    member: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<SampleTarget, AppError> {
    resolve_sample_target_inner(docs, cube_name, member, role, binds, true)
}

fn resolve_sample_target_inner(
    docs: &[Value],
    cube_name: &str,
    member: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    enforce_rbac: bool,
) -> Result<SampleTarget, AppError> {
    let cube = find_doc(docs, "cube", cube_name)?;
    if enforce_rbac {
        check_role_access(cube.get("role"), role)?;
    }
    let member_doc = find_cube_member(cube, member)
        .ok_or_else(|| AppError::NotFound(format!("cube {cube_name} has no member {member}")))?;
    if enforce_rbac {
        check_role_access(member_doc.get("role"), role)?;
    }
    let view_name = cube
        .get("view")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation(format!("cube {cube_name}: view missing")))?;
    let view = find_doc(docs, "view", view_name)?;

    if let Some((code_table_name, as_code)) = resolve_dim_as_code(docs, cube, view, member)? {
        let code_doc = find_doc(docs, "table", &code_table_name)?;
        if enforce_rbac {
            check_role_access(code_doc.get("role"), role)?;
        }
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
        let mut filters = collect_table_filters(code_doc, role, binds)?;
        if let Some(w) = as_code.get("where").and_then(|v| v.as_str()) {
            let resolved = apply_binds(w, binds);
            filters.push(format!("({resolved})"));
        }
        let where_extra = (!filters.is_empty()).then(|| filters.join(" AND "));
        return Ok(SampleTarget {
            schema: schema.to_string(),
            table: table.to_string(),
            column: key.to_string(),
            value_column: Some(value.to_string()),
            where_extra,
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

    if member_doc.get("expression").is_some() && member_doc.get("column").is_none() {
        return resolve_expression_sample(
            docs,
            cube_name,
            view,
            member_doc,
            member,
            role,
            binds,
            enforce_rbac,
        );
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
    let catalog = crate::compile::catalog_for_alias(view, source)?;
    let table_doc = find_doc(docs, "table", &catalog)?;
    if enforce_rbac {
        check_role_access(table_doc.get("role"), role)?;
    }
    let schema = str_field(table_doc, "schema")?;
    let table = str_field(table_doc, "table")?;
    let filters = collect_table_filters(table_doc, role, binds)?;
    let where_extra = (!filters.is_empty()).then(|| filters.join(" AND "));
    Ok(SampleTarget {
        schema: schema.to_string(),
        table: table.to_string(),
        column: phys_col.to_string(),
        value_column: None,
        where_extra,
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
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    enforce_rbac: bool,
) -> Result<SampleTarget, AppError> {
    let expr = member_doc
        .get("expression")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("expression missing".into()))?;
    let idents = extract_idents(expr).unwrap_or_default();
    let mut sources: Vec<String> = Vec::new();
    let mut first_phys_col: Option<String> = None;
    for tok in &idents {
        if let Ok(Some(m)) = view_first_mapping(view, tok) {
            if let Some(src) = m.get("source").and_then(|v| v.as_str()) {
                if !sources.iter().any(|s| s == src) {
                    sources.push(src.to_string());
                }
            }
            if first_phys_col.is_none() {
                first_phys_col = m.get("column").and_then(|v| v.as_str()).map(str::to_string);
            }
        }
    }
    if sources.len() != 1 {
        return Err(AppError::Validation(format!(
            "expression member {member} spans multiple tables; use execute_sql"
        )));
    }
    let source = &sources[0];
    let catalog = crate::compile::catalog_for_alias(view, source)?;
    let table_doc = find_doc(docs, "table", &catalog)?;
    if enforce_rbac {
        check_role_access(table_doc.get("role"), role)?;
    }
    let schema = str_field(table_doc, "schema")?;
    let table = str_field(table_doc, "table")?;
    let phys_col = first_phys_col.ok_or_else(|| {
        AppError::Validation(format!("cannot resolve physical column for {member}"))
    })?;
    let _ = physical_from(table_doc)?;
    let filters = collect_table_filters(table_doc, role, binds)?;
    let where_extra = (!filters.is_empty()).then(|| filters.join(" AND "));
    Ok(SampleTarget {
        schema: schema.to_string(),
        table: table.to_string(),
        column: phys_col.clone(),
        value_column: None,
        where_extra,
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

const SAMPLE_LIKE_MAX_CHARS: usize = 256;

/// Build SELECT for sampling; used when value_column / where need custom SQL.
/// Identifiers use `dialect` quoting (D21). Row limit is applied by `TargetDb::execute_select`.
/// `like` is an escaped substring (not a raw LIKE pattern). `as_code` matches `value_column`.
pub fn sample_sql(
    target: &SampleTarget,
    _limit: usize,
    dialect: DialectProfile,
    like: Option<&str>,
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
    if let Some(needle) = like.map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(" AND ");
        sql.push_str(&like_predicate(dialect, target, needle)?);
    }
    // where_extra / expressions may contain canonical date helpers (D21·D25).
    dialect.emit_physical_sql(&sql)
}

fn like_predicate(
    dialect: DialectProfile,
    target: &SampleTarget,
    needle: &str,
) -> Result<String, AppError> {
    if needle.chars().count() > SAMPLE_LIKE_MAX_CHARS {
        return Err(AppError::Validation(format!(
            "sample_column.like must be at most {SAMPLE_LIKE_MAX_CHARS} characters"
        )));
    }
    let like_col = if target.from_as_code {
        let value = target
            .value_column
            .as_deref()
            .ok_or_else(|| AppError::Validation("as_code sample requires value_column".into()))?;
        dialect.quote_ident(value)?
    } else {
        dialect.quote_ident(&target.column)?
    };
    let casted = match dialect {
        DialectProfile::Mysql => format!("CAST({like_col} AS CHAR)"),
        DialectProfile::Oracle => format!("CAST({like_col} AS VARCHAR2(4000))"),
        DialectProfile::Postgres | DialectProfile::Sqlite => {
            format!("CAST({like_col} AS TEXT)")
        }
    };
    Ok(format!(
        "{casted} LIKE {} ESCAPE '!'",
        like_contains_literal(needle)
    ))
}

/// Substring pattern with `%` `_` `!` escaped; SQL string quotes doubled.
fn like_contains_literal(needle: &str) -> String {
    let mut pat = String::from("%");
    for c in needle.chars() {
        if matches!(c, '%' | '_' | '!') {
            pat.push('!');
        }
        pat.push(c);
    }
    pat.push('%');
    format!("'{}'", pat.replace('\'', "''"))
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
        let pg = sample_sql(&t, 10, DialectProfile::Postgres, None).unwrap();
        assert!(pg.contains("\"sale_amount\""));
        assert!(pg.contains("\"sales\".\"orders\""));
        assert!(!pg.contains(" LIKE "));
        let my = sample_sql(&t, 10, DialectProfile::Mysql, None).unwrap();
        assert!(my.contains("`sale_amount`"));
        assert!(my.contains("`sales`.`orders`"));
        assert!(!my.contains('"'));
    }

    fn scalar_target() -> SampleTarget {
        SampleTarget {
            schema: "sales".into(),
            table: "orders".into(),
            column: "sale_amount".into(),
            value_column: None,
            where_extra: None,
            resolved: json!({}),
            from_as_code: false,
        }
    }

    fn as_code_target() -> SampleTarget {
        SampleTarget {
            schema: "ref".into(),
            table: "status_codes".into(),
            column: "code".into(),
            value_column: Some("label".into()),
            where_extra: Some("lang = 'en'".into()),
            resolved: json!({}),
            from_as_code: true,
        }
    }

    #[test]
    fn sample_sql_like_filters_scalar_and_escapes() {
        let sql = sample_sql(
            &scalar_target(),
            10,
            DialectProfile::Postgres,
            Some("a%b_c!'d"),
        )
        .unwrap();
        assert!(
            sql.contains(r#"CAST("sale_amount" AS TEXT) LIKE '%a!%b!_c!!''d%' ESCAPE '!'"#),
            "{sql}"
        );
        let my = sample_sql(&scalar_target(), 10, DialectProfile::Mysql, Some("80")).unwrap();
        assert!(
            my.contains("CAST(`sale_amount` AS CHAR) LIKE '%80%' ESCAPE '!'"),
            "{my}"
        );
    }

    #[test]
    fn sample_sql_like_matches_as_code_value_column() {
        let sql = sample_sql(
            &as_code_target(),
            10,
            DialectProfile::Postgres,
            Some("Paid"),
        )
        .unwrap();
        assert!(
            sql.contains(r#"CAST("label" AS TEXT) LIKE '%Paid%' ESCAPE '!'"#),
            "{sql}"
        );
        assert!(
            !sql.contains(r#"CAST("code" AS TEXT)"#),
            "like must not match as_code key: {sql}"
        );
        assert!(sql.contains("lang = 'en'"), "{sql}");
    }

    #[test]
    fn sample_sql_like_blank_is_off() {
        let sql = sample_sql(&scalar_target(), 10, DialectProfile::Postgres, Some("  ")).unwrap();
        assert!(!sql.contains(" LIKE "), "{sql}");
    }

    #[test]
    fn sample_sql_like_rejects_too_long() {
        let needle = "x".repeat(257);
        let err = sample_sql(
            &scalar_target(),
            10,
            DialectProfile::Postgres,
            Some(&needle),
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn sample_with_context_enforces_rbac_and_table_filters() {
        let docs = example_docs();
        // sale_amount is restricted to ["finance"]
        assert!(matches!(
            resolve_sample_target_with_context(&docs, "sales", "sale_amount", Some("branch"), None),
            Err(AppError::Forbidden(_))
        ));
        let mut binds = HashMap::new();
        binds.insert("caller_region".into(), "KR".into());
        // order_date is "all", on orders table which has permission_filter for "branch" and default_filter
        let t = resolve_sample_target_with_context(
            &docs,
            "sales",
            "order_date",
            Some("branch"),
            Some(&binds),
        )
        .unwrap();
        let w = t.where_extra.as_deref().unwrap();
        assert!(
            w.contains("region_code = 'KR'"),
            "expected resolved permission filter in {w}"
        );
        assert!(w.contains("CANCELLED"), "expected default_filter in {w}");
    }
}
