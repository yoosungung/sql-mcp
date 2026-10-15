use serde_json::Value;

/// D9: heuristic code detection from column name + sample values.
pub fn detect_code(column: &str, samples: &[Value]) -> (bool, Option<&'static str>) {
    let name = column.to_ascii_lowercase();
    if name.contains("sql")
        || name.contains("script")
        || name.ends_with("_code_src")
        || name.contains("expression")
    {
        return (true, guess_language(samples));
    }
    let mut codeish = 0usize;
    for v in samples.iter().take(20) {
        if let Some(s) = v.as_str() {
            if looks_like_code(s) {
                codeish += 1;
            }
        }
    }
    if !samples.is_empty() && codeish * 2 >= samples.len().min(20) {
        (true, guess_language(samples))
    } else {
        (false, None)
    }
}

fn looks_like_code(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.len() < 8 {
        return false;
    }
    let has_newline = trimmed.contains('\n');
    let keywords = ["SELECT ", "INSERT ", "function ", "def ", "return ", "=>{", "BEGIN "];
    let hit = keywords.iter().any(|k| trimmed.to_ascii_uppercase().contains(&k.to_ascii_uppercase()));
    (has_newline && (trimmed.contains(';') || trimmed.contains('{'))) || hit
}

fn guess_language(samples: &[Value]) -> Option<&'static str> {
    let joined = samples
        .iter()
        .filter_map(|v| v.as_str())
        .take(5)
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    if joined.contains("select ") || joined.contains("from ") {
        Some("sql")
    } else if joined.contains("def ") || joined.contains("import ") {
        Some("python")
    } else if joined.contains("function ") || joined.contains("=>") {
        Some("javascript")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn name_hint() {
        let (is_code, _) = detect_code("user_sql", &[]);
        assert!(is_code);
    }

    #[test]
    fn sample_sql() {
        let samples = vec![json!("SELECT id FROM t WHERE x = 1")];
        let (is_code, lang) = detect_code("payload", &samples);
        assert!(is_code);
        assert_eq!(lang, Some("sql"));
    }

    #[test]
    fn plain_values() {
        let samples = vec![json!("KR"), json!("US")];
        let (is_code, _) = detect_code("region_code", &samples);
        assert!(!is_code);
    }
}
