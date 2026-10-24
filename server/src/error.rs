use serde_json::{json, Value};
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

/// Plain-language `SELECT *` rejection. Keeps the legacy phrase classifiers match.
pub fn select_star_message() -> String {
    "SELECT * is not a column on a cube. The * was rejected. Name each dimension or fact instead. SELECT * not allowed in semantic SQL".into()
}

/// Bare aggregate or other non-alias expression in `ORDER BY`.
pub fn order_by_not_alias_message(expr: &str) -> String {
    format!(
        "ORDER BY cannot use {expr} directly. Give that expression an alias in the SELECT list, then ORDER BY the alias. ORDER BY must reference a semantic column or alias, got: {expr}"
    )
}

/// Function outside the D25 / arithmetic allowlist.
pub fn forbidden_function_message(place: &str, name: &str) -> String {
    format!(
        "{name} is not allowed in this {place}. Use EXTRACT, DATEDIFF, date_trunc, or + - * / instead. function not allowed in {place}: {name}"
    )
}

/// Unknown dimension, fact, or alias.
pub fn unknown_member_message(legacy: &str) -> String {
    format!(
        "That name is not a dimension, fact, or SELECT alias on this cube. Copy the member name from describe_table. {legacy}"
    )
}

/// Attach one fix hint to a sqlparser error. The parser text stays verbatim.
pub fn clarify_sql_parse(raw: &str) -> String {
    let low = raw.to_ascii_lowercase();
    let hint = if low.contains("found: when") {
        "A CASE expression is missing END. Add END after the last THEN value."
    } else if low.contains("expected: )") {
        "A '(' has no matching ')'. Close it before the token the parser found."
    } else if low.contains("found: )") {
        "There is an extra ')'. Remove the unmatched closing parenthesis."
    } else {
        "The statement did not parse. Fix the token named in the parser text."
    };
    format!("{hint} Parser said: {raw}")
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

    #[test]
    fn agent_messages_name_the_token_and_one_fix() {
        let star = select_star_message();
        assert!(star.contains("SELECT *"), "{star}");
        assert!(star.contains("dimension or fact"), "{star}");

        let order = order_by_not_alias_message("MIN(T2.cost)");
        assert!(order.contains("MIN(T2.cost)"), "{order}");
        assert!(
            order.contains("ORDER BY must reference a semantic column or alias"),
            "{order}"
        );
        assert!(order.to_ascii_lowercase().contains("alias"), "{order}");

        let func = forbidden_function_message("metric expr", "SUBTRACT");
        assert!(
            func.contains("function not allowed in metric expr: SUBTRACT"),
            "{func}"
        );
        assert!(func.contains("date_trunc"), "{func}");

        let member = unknown_member_message("unknown semantic member: T2.member");
        assert!(
            member.contains("unknown semantic member: T2.member"),
            "{member}"
        );
        assert!(member.contains("describe_table"), "{member}");

        let case =
            clarify_sql_parse("sql parser error: Expected: ), found: WHEN at Line: 1, Column: 24");
        assert!(case.contains("END"), "{case}");
        assert!(case.contains("found: WHEN"), "{case}");

        let paren =
            clarify_sql_parse("sql parser error: Expected: ), found: where at Line: 1, Column: 25");
        assert!(paren.contains("')'"), "{paren}");
        assert!(paren.contains("found: where"), "{paren}");
    }
}
