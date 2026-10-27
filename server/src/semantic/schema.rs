use std::sync::OnceLock;

use jsonschema::error::{ValidationError, ValidationErrorKind};
use jsonschema::Validator;
use serde_json::Value;

use crate::error::AppError;

/// MCP Resource URI — equals JSON Schema `$id` (not an HTTP download URL).
pub const DOCUMENT_SCHEMA_URI: &str = "https://sql-mcp.local/semantic/document.schema.json";

pub const DOCUMENT_SCHEMA_JSON: &str =
    include_str!("../../../semantic/schema/document.schema.json");

fn validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(DOCUMENT_SCHEMA_JSON).expect("document.schema.json must parse");
        Validator::new(&schema).expect("document.schema.json must compile")
    })
}

pub fn validate_document(body: &Value) -> Result<(), AppError> {
    if validator().is_valid(body) {
        return Ok(());
    }
    let kind = body.get("kind").and_then(|v| v.as_str());
    let mut parts = Vec::new();
    for err in validator().iter_errors(body) {
        collect_validation_messages(&err, kind, &mut parts);
    }
    if parts.is_empty() {
        parts.push("document does not match table, view, cube, metric, or segment".into());
    }
    parts.sort();
    parts.dedup();
    Err(AppError::Validation(parts.join("; ")))
}

fn collect_validation_messages(err: &ValidationError<'_>, kind: Option<&str>, out: &mut Vec<String>) {
    match err.kind() {
        ValidationErrorKind::OneOfNotValid { context } | ValidationErrorKind::AnyOf { context } => {
            let idx = match kind {
                Some("table") => Some(0),
                Some("view") => Some(1),
                Some("cube") => Some(2),
                Some("metric") => Some(3),
                Some("segment") => Some(4),
                _ => None,
            };
            let branch = idx
                .and_then(|i| context.get(i))
                .filter(|errors| !errors.is_empty())
                .or_else(|| context.iter().find(|errors| !errors.is_empty()));
            if let Some(errors) = branch {
                for inner in errors {
                    collect_validation_messages(inner, kind, out);
                }
            } else {
                out.push(leaf_message(err));
            }
        }
        _ => out.push(leaf_message(err)),
    }
}

fn leaf_message(err: &ValidationError<'_>) -> String {
    let path = err.instance_path().to_string();
    let msg = err.masked().to_string();
    if path.is_empty() {
        msg
    } else {
        format!("{path}: {msg}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_example_orders_table() {
        let raw = include_str!("../../../semantic/examples/tables/orders.json");
        let body: Value = serde_json::from_str(raw).unwrap();
        validate_document(&body).unwrap();
    }

    #[test]
    fn rejects_missing_kind() {
        let body = json!({"name": "x"});
        assert!(matches!(
            validate_document(&body),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn accepts_relationship_and_cube_grain() {
        let mut view: Value = serde_json::from_str(include_str!(
            "../../../semantic/examples/views/orders_customer.json"
        ))
        .unwrap();
        view["joins"][0]["relationship"] = json!("many_to_one");
        validate_document(&view).unwrap();
        view["joins"][0]["relationship"] = json!("many_to_many");
        assert!(validate_document(&view).is_err());

        let mut cube: Value =
            serde_json::from_str(include_str!("../../../semantic/examples/cubes/sales.json"))
                .unwrap();
        cube["grain"] = json!(["order_id"]);
        validate_document(&cube).unwrap();
    }

    #[test]
    fn accepts_example_cube_metric_segment() {
        for raw in [
            include_str!("../../../semantic/examples/cubes/sales.json"),
            include_str!("../../../semantic/examples/metrics/revenue.json"),
            include_str!("../../../semantic/examples/segments/paid.json"),
        ] {
            let body: Value = serde_json::from_str(raw).unwrap();
            validate_document(&body).unwrap();
        }
    }

    #[test]
    fn rejects_dimension_without_description() {
        let mut body: Value =
            serde_json::from_str(include_str!("../../../semantic/examples/cubes/sales.json"))
                .unwrap();
        body["dimensions"][0]
            .as_object_mut()
            .unwrap()
            .remove("description");
        assert!(matches!(
            validate_document(&body),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn accepts_physical_column_with_spaces() {
        let body = json!({
            "kind": "table",
            "name": "frpm",
            "target": "california_schools",
            "db": "california_schools",
            "schema": "main",
            "table": "frpm",
            "columns": [
                {"name": "CDSCode", "type": "TEXT", "nullable": false},
                {"name": "Academic Year", "type": "TEXT", "nullable": true},
                {"name": "Percent (%) Eligible Free (K-12)", "type": "REAL", "nullable": true}
            ],
            "pk": ["CDSCode"],
            "fk": [],
            "permission_filters": [],
            "scope_filter": null,
            "default_filter": null,
            "role": "all"
        });
        validate_document(&body).unwrap();
    }

    #[test]
    fn rejects_spaced_name_on_semantic_document_name() {
        let body = json!({
            "kind": "table",
            "name": "bad name",
            "target": "t",
            "db": "t",
            "schema": "main",
            "table": "t",
            "columns": [{"name": "id", "type": "INT", "nullable": false}],
            "pk": ["id"],
            "fk": [],
            "permission_filters": [],
            "scope_filter": null,
            "default_filter": null
        });
        assert!(matches!(
            validate_document(&body),
            Err(AppError::Validation(_))
        ));
    }

    fn identity_view_json() -> Value {
        json!({
            "kind": "view",
            "name": "tox_molecule_view",
            "target": "toxicology",
            "combine": "join",
            "sources": [{"kind": "table", "table": "molecule"}],
            "joins": [],
            "columns": [{
                "name": "molecule_id",
                "mappings": [{"source": "molecule", "column": "molecule_id"}]
            }]
        })
    }

    #[test]
    fn accepts_identity_join_view_one_source() {
        validate_document(&identity_view_json()).unwrap();
    }

    #[test]
    fn rejects_two_source_join_view_without_joins() {
        let mut body = identity_view_json();
        body["sources"] = json!([
            {"kind": "table", "table": "molecule"},
            {"kind": "table", "table": "atom"}
        ]);
        assert!(
            matches!(validate_document(&body), Err(AppError::Validation(_))),
            "two-source join view must require joins"
        );
    }

    #[test]
    fn accepts_dimension_type_id() {
        let body = json!({
            "kind": "cube",
            "name": "sales",
            "target": "dw",
            "view": "orders_customer",
            "description": "",
            "dimensions": [{
                "name": "order_id",
                "description": "",
                "type": "id",
                "column": "order_id",
                "encryption": false
            }],
            "facts": []
        });
        validate_document(&body).unwrap();
    }

    #[test]
    fn accepts_mapped_cube_with_id_and_quantity_members() {
        let body = json!({
            "kind": "cube",
            "name": "superhero",
            "target": "superhero",
            "view": "join_superhero",
            "description": "",
            "dimensions": [{
                "name": "hero_id",
                "description": "",
                "type": "id",
                "column": "hero_id",
                "encryption": false
            }],
            "facts": [{
                "name": "height_cm",
                "description": "",
                "type": "quantity",
                "column": "height_cm",
                "encryption": false
            }]
        });
        validate_document(&body).unwrap();
    }

    #[test]
    fn names_invalid_dimension_type_without_dumping_the_document() {
        let body = json!({
            "kind": "cube",
            "name": "superhero",
            "target": "superhero",
            "view": "join_superhero",
            "description": "",
            "dimensions": [{
                "name": "hero_id",
                "description": "",
                "type": "nope",
                "column": "hero_id",
                "encryption": false
            }],
            "facts": []
        });
        let Err(AppError::Validation(msg)) = validate_document(&body) else {
            panic!("expected validation error");
        };
        assert!(
            msg.contains("/dimensions/0/type") || msg.contains("nope"),
            "got {msg}"
        );
        assert!(
            !msg.contains("\"kind\":\"cube\""),
            "must not dump the whole document: {msg}"
        );
    }

    #[test]
    fn accepts_non_additive_expression_fact() {
        let body = json!({
            "kind": "cube",
            "name": "sales",
            "target": "dw",
            "view": "orders_customer",
            "description": "",
            "dimensions": [{
                "name": "order_date",
                "description": "",
                "type": "date",
                "column": "order_date",
                "encryption": false
            }],
            "facts": [{
                "name": "eligible_frpm_rate_k12",
                "description": "row rate",
                "type": "quantity",
                "expression": "sale_amount / fx_rate",
                "additive": false,
                "encryption": false
            }]
        });
        validate_document(&body).unwrap();
    }

    #[test]
    fn rejects_physical_ident_with_quote_or_semicolon() {
        let mut body = json!({
            "kind": "table",
            "name": "t",
            "target": "t",
            "db": "t",
            "schema": "main",
            "table": "t",
            "columns": [{"name": "id", "type": "INT", "nullable": false}],
            "pk": ["id"],
            "fk": [],
            "permission_filters": [],
            "scope_filter": null,
            "default_filter": null
        });
        body["columns"][0]["name"] = json!("a;drop");
        assert!(matches!(
            validate_document(&body),
            Err(AppError::Validation(_))
        ));
        body["columns"][0]["name"] = json!("we\"ird");
        assert!(matches!(
            validate_document(&body),
            Err(AppError::Validation(_))
        ));
    }
}
