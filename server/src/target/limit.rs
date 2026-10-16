//! Engine-specific SELECT row-limit wrappers.

/// Wrap a SELECT so the engine returns at most `max_rows`.
pub fn wrap_select(engine: &str, sql: &str, max_rows: usize) -> String {
    match engine {
        "oracle" => format!(
            "SELECT * FROM ({sql}) _sql_mcp_q FETCH FIRST {max_rows} ROWS ONLY"
        ),
        _ => format!("SELECT * FROM ({sql}) AS _sql_mcp_q LIMIT {max_rows}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_mysql_sqlite_use_limit() {
        let s = wrap_select("postgres", "SELECT 1 AS n", 10);
        assert!(s.contains("LIMIT 10"));
        assert!(!s.contains("FETCH FIRST"));
        let s = wrap_select("mysql", "SELECT 1", 5);
        assert!(s.contains("LIMIT 5"));
        let s = wrap_select("sqlite", "SELECT 1", 3);
        assert!(s.contains("LIMIT 3"));
    }

    #[test]
    fn oracle_uses_fetch_first() {
        let s = wrap_select("oracle", "SELECT 1 AS n FROM dual", 7);
        assert!(s.contains("FETCH FIRST 7 ROWS ONLY"));
        assert!(!s.contains("LIMIT"));
    }
}
