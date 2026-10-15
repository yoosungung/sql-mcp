use std::sync::OnceLock;

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
    let msg = validator()
        .iter_errors(body)
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    Err(AppError::Validation(msg))
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
}
