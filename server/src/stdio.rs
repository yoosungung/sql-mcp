use rmcp::ServiceExt;
use rmcp::transport::stdio;

use crate::tools::SqlMcp;

/// Serve MCP over stdin/stdout (local Cursor / Claude Desktop child process).
pub async fn serve(server: SqlMcp) -> anyhow::Result<()> {
    tracing::info!("sql-mcp listening (stdio)");
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}

/// `--stdio` or `SQL_MCP_TRANSPORT=stdio` (case-insensitive).
pub fn want_stdio(args: impl IntoIterator<Item = String>) -> bool {
    if args.into_iter().any(|a| a == "--stdio") {
        return true;
    }
    std::env::var("SQL_MCP_TRANSPORT")
        .map(|v| v.eq_ignore_ascii_case("stdio"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_flag() {
        assert!(want_stdio(["sql-mcp".into(), "--stdio".into()]));
        assert!(!want_stdio(["sql-mcp".into()]));
    }
}
