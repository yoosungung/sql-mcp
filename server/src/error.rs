use serde_json::{Value, json};
use thiserror::Error;

/// ARCHITECTURE §3.7 error codes.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("not a single SELECT")]
    NotSelect,
    #[error("sql parse error: {0}")]
    SqlParse(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("connection error")]
    Connection(#[source] anyhow::Error),
    #[error("query timeout")]
    Timeout,
    #[error("row limit exceeded")]
    RowLimit,
    #[error("internal error")]
    Internal(#[source] anyhow::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotSelect => "not_select",
            Self::SqlParse(_) => "sql_parse",
            Self::NotFound(_) => "not_found",
            Self::Validation(_) => "validation",
            Self::Forbidden(_) => "forbidden",
            Self::Connection(_) => "connection",
            Self::Timeout => "timeout",
            Self::RowLimit => "row_limit",
            Self::Internal(_) => "internal",
        }
    }

    pub fn to_json(&self) -> Value {
        let msg = match self {
            Self::Connection(e) => format!(
                "connection error: {}",
                sanitize_secret_message(&e.to_string())
            ),
            Self::Internal(e) => format!(
                "internal error: {}",
                sanitize_secret_message(&e.to_string())
            ),
            _ => self.to_string(),
        };
        json!({
            "code": self.code(),
            "message": msg,
        })
    }
}

/// Scrub DSNs, credentials (`user:pass@`, `user/pass@`, `password=...`) from error messages (ARCHITECTURE §1.10).
pub fn sanitize_secret_message(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for token in raw.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        if token.contains("://")
            || token.contains('@')
            || token.to_ascii_lowercase().contains("password=")
            || token.to_ascii_lowercase().contains("pwd=")
        {
            out.push_str("[REDACTED_DSN]");
        } else {
            out.push_str(token);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_safe_db_error_details_and_redacts_dsn() {
        let err = AppError::Connection(anyhow::anyhow!(
            "db error on postgres://scott:tiger@db.internal:5432/dw: relation \"orders\" does not exist"
        ));
        let j = err.to_json();
        let msg = j["message"].as_str().unwrap();
        assert!(msg.contains("relation \"orders\" does not exist"), "{msg}");
        assert!(!msg.contains("tiger"), "{msg}");
        assert!(!msg.contains("scott"), "{msg}");
        assert!(msg.contains("[REDACTED_DSN]"), "{msg}");
    }

    #[test]
    fn redacts_oracle_slash_at_dsn() {
        let s = sanitize_secret_message("failed connect scott/tiger@//host:1521/orcl timeout");
        assert!(!s.contains("tiger"), "{s}");
        assert!(s.contains("[REDACTED_DSN]"), "{s}");
    }
}
