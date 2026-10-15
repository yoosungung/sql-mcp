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
            Self::Connection(_) => "connection",
            Self::Timeout => "timeout",
            Self::RowLimit => "row_limit",
            Self::Internal(_) => "internal",
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code(),
            "message": self.to_string(),
        })
    }
}
