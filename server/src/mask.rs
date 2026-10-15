//! D17: response masking for `encryption: true` dimension/fact fields.

use std::collections::HashSet;

use serde_json::{Value, json};

/// Mask token used when null is undesirable; primary mask is JSON null.
pub const MASK_TOKEN: &str = "***";

/// Names of encrypted cube members and their mapped view/physical columns.
pub fn encrypted_names(docs: &[Value]) -> HashSet<String> {
    let mut out = HashSet::new();
    for doc in docs {
        if doc.get("kind").and_then(|v| v.as_str()) != Some("cube") {
            continue;
        }
        for key in ["dimensions", "facts"] {
            let Some(items) = doc.get(key).and_then(|v| v.as_array()) else {
                continue;
            };
            for item in items {
                if item.get("encryption").and_then(|v| v.as_bool()) != Some(true) {
                    continue;
                }
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    out.insert(n.to_string());
                }
                if let Some(c) = item.get("column").and_then(|v| v.as_str()) {
                    out.insert(c.to_string());
                }
            }
        }
    }
    out
}

pub fn is_encrypted_column(docs: &[Value], column: &str) -> bool {
    encrypted_names(docs).contains(column)
}

/// Replace values for encrypted column names with null.
pub fn mask_result_rows(
    columns: &[String],
    rows: Vec<Vec<Value>>,
    encrypted: &HashSet<String>,
) -> Vec<Vec<Value>> {
    if encrypted.is_empty() {
        return rows;
    }
    let mask_idx: Vec<usize> = columns
        .iter()
        .enumerate()
        .filter(|(_, c)| encrypted.contains(c.as_str()))
        .map(|(i, _)| i)
        .collect();
    if mask_idx.is_empty() {
        return rows;
    }
    rows.into_iter()
        .map(|mut row| {
            for &i in &mask_idx {
                if i < row.len() {
                    row[i] = Value::Null;
                }
            }
            row
        })
        .collect()
}

pub fn mask_sample_values(values: Vec<Value>) -> Vec<Value> {
    values.into_iter().map(|_| Value::Null).collect()
}

/// Optional explicit mask marker for clients that distinguish absent vs masked.
pub fn masked_null() -> Value {
    json!(null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn collects_encrypted_names() {
        let cube = json!({
            "kind": "cube",
            "dimensions": [
                {"name": "ssn", "column": "ssn_col", "encryption": true},
                {"name": "region", "column": "region_code", "encryption": false}
            ],
            "facts": [
                {"name": "salary", "column": "salary_amt", "encryption": true}
            ]
        });
        let names = encrypted_names(&[cube]);
        assert!(names.contains("ssn"));
        assert!(names.contains("ssn_col"));
        assert!(names.contains("salary"));
        assert!(names.contains("salary_amt"));
        assert!(!names.contains("region"));
    }

    #[test]
    fn masks_matching_columns() {
        let enc: HashSet<_> = ["ssn"].into_iter().map(str::to_string).collect();
        let cols = vec!["id".into(), "ssn".into()];
        let rows = vec![vec![json!(1), json!("secret")]];
        let out = mask_result_rows(&cols, rows, &enc);
        assert_eq!(out[0][0], json!(1));
        assert_eq!(out[0][1], Value::Null);
    }
}
