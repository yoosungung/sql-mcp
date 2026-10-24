//! Process argv for local binary UX (`--help`, unknown flags). MCP protocol is separate.

/// Printed to stdout for `--help` / `-h`. Exit 0 before opening config or transport.
pub const HELP: &str = "\
sql-mcp — semantic SQL MCP server (HTTP or stdio)

USAGE:
  sql-mcp [OPTIONS]

OPTIONS:
  -h, --help     Show this help and exit
      --stdio    Serve MCP on stdin/stdout (Cursor / Claude Desktop child process)
                 Same as SQL_MCP_TRANSPORT=stdio

ENVIRONMENT (common):
  SQL_MCP_CONFIG           Path to config.toml (default: ./server/examples/config.example.toml)
  SQL_MCP_SEMANTIC_PATH    libSQL semantic DB path (default: ./data/semantic.db)
  SQL_MCP_LISTEN           HTTP bind address (default from config, else 127.0.0.1:8080)
  SQL_MCP_TRANSPORT        stdio | http (default: http unless --stdio)
  Target DSN env vars      Named by each [[targets]].url_env in config.toml

On failure, stderr shows the path/env that failed and a short hint. Full setup: README.md
";

/// True if argv contains `-h` or `--help`.
pub fn wants_help(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().skip(1).any(|a| a == "-h" || a == "--help")
}

/// First unrecognized flag (leading `-`), if any. `--stdio` / help are known.
pub fn first_unknown_flag(args: impl IntoIterator<Item = String>) -> Option<String> {
    for a in args.into_iter().skip(1) {
        if a == "-h" || a == "--help" || a == "--stdio" {
            continue;
        }
        if a.starts_with('-') {
            return Some(a);
        }
    }
    None
}

/// First non-option positional argument, if any.
pub fn first_unexpected_arg(args: impl IntoIterator<Item = String>) -> Option<String> {
    for a in args.into_iter().skip(1) {
        if a == "-h" || a == "--help" || a == "--stdio" {
            continue;
        }
        if !a.starts_with('-') {
            return Some(a);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_help_flags() {
        assert!(wants_help(["sql-mcp".into(), "--help".into()]));
        assert!(wants_help(["sql-mcp".into(), "-h".into()]));
        assert!(!wants_help(["sql-mcp".into(), "--stdio".into()]));
        assert!(!wants_help(["sql-mcp".into()]));
    }

    #[test]
    fn help_mentions_stdio_and_config_env() {
        assert!(HELP.contains("--stdio"));
        assert!(HELP.contains("SQL_MCP_CONFIG"));
        assert!(HELP.contains("SQL_MCP_SEMANTIC_PATH"));
    }

    #[test]
    fn rejects_unknown_flags() {
        assert_eq!(
            first_unknown_flag(["sql-mcp".into(), "--foo".into()]).as_deref(),
            Some("--foo")
        );
        assert!(first_unknown_flag(["sql-mcp".into(), "--stdio".into(), "-h".into()]).is_none());
    }

    #[test]
    fn rejects_positional_args() {
        assert_eq!(
            first_unexpected_arg(["sql-mcp".into(), "serve".into()]).as_deref(),
            Some("serve")
        );
        assert!(first_unexpected_arg(["sql-mcp".into(), "--stdio".into()]).is_none());
    }
}
