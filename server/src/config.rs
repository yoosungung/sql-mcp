use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::error::AppError;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_max_rows")]
    pub max_rows: usize,
    #[serde(default = "default_sample_limit")]
    pub sample_limit: usize,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetConfig {
    /// Stable handle: MCP `list_databases.name` / tool `database` / document `target`.
    pub id: String,
    /// Human/agent-facing blurb (`list_databases.description`). TOML may still use `name=`.
    #[serde(alias = "name")]
    pub description: String,
    pub engine: String,
    pub url_env: String,
}

fn default_listen() -> String {
    "127.0.0.1:8080".into()
}
fn default_max_rows() -> usize {
    1000
}
fn default_sample_limit() -> usize {
    20
}
fn default_timeout_ms() -> u64 {
    30_000
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|e| {
            AppError::Validation(format!(
                "cannot read config {}: {e} (set SQL_MCP_CONFIG or create the file; see --help)",
                path.display()
            ))
        })?;
        let mut cfg: Config = toml::from_str(&raw).map_err(|e| {
            AppError::Validation(format!(
                "invalid config {}: {e}",
                path.display()
            ))
        })?;
        if let Ok(listen) = env::var("SQL_MCP_LISTEN") {
            cfg.listen = listen;
        }
        if let Ok(v) = env::var("SQL_MCP_MAX_ROWS") {
            cfg.max_rows = v
                .parse()
                .map_err(|e| AppError::Validation(format!("SQL_MCP_MAX_ROWS: {e}")))?;
        }
        if let Ok(v) = env::var("SQL_MCP_SAMPLE_LIMIT") {
            cfg.sample_limit = v
                .parse()
                .map_err(|e| AppError::Validation(format!("SQL_MCP_SAMPLE_LIMIT: {e}")))?;
        }
        if let Ok(v) = env::var("SQL_MCP_TIMEOUT_MS") {
            cfg.timeout_ms = v
                .parse()
                .map_err(|e| AppError::Validation(format!("SQL_MCP_TIMEOUT_MS: {e}")))?;
        }
        Ok(cfg)
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    /// MCP persona: `query` (DW agent, default) or `author` (metadata registration).
    pub fn mode(&self) -> McpMode {
        McpMode::from_env()
    }

    pub fn default_role(&self) -> Option<String> {
        env::var("SQL_MCP_DEFAULT_ROLE")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn semantic_path() -> PathBuf {
        env::var("SQL_MCP_SEMANTIC_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data/semantic.db"))
    }

    pub fn config_path() -> PathBuf {
        env::var("SQL_MCP_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./server/examples/config.example.toml"))
    }
}

/// Server tool surface mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpMode {
    /// DW agent: query tools; metadata mutations forbidden.
    Query,
    /// Coding agent: metadata CRUD allowed.
    Author,
}

impl McpMode {
    pub fn from_env() -> Self {
        match env::var("SQL_MCP_MODE") {
            Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
                "author" | "coding" => Self::Author,
                "query" | "dw" => Self::Query,
                other => {
                    eprintln!(
                        "sql-mcp: unknown SQL_MCP_MODE={other:?}, expected query|author; using query"
                    );
                    Self::Query
                }
            },
            Err(_) => Self::Query,
        }
    }

    pub fn allows_metadata_mutation(self) -> bool {
        matches!(self, Self::Author)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn missing_config_names_path_and_hint() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("no-such-config.toml");
        let err = Config::load(&missing).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(missing.to_string_lossy().as_ref()), "{msg}");
        assert!(msg.contains("SQL_MCP_CONFIG"), "{msg}");
        assert!(msg.contains("--help"), "{msg}");
    }

    #[test]
    fn invalid_toml_names_path() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        fs::write(&path, "listen = [").unwrap();
        let err = Config::load(&path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains(path.to_string_lossy().as_ref()), "{msg}");
        assert!(msg.contains("invalid config"), "{msg}");
    }

    #[test]
    fn mode_from_env_query_and_author() {
        // Single test to avoid parallel env races.
        let prev_mode = env::var("SQL_MCP_MODE").ok();
        let restore = || match &prev_mode {
            Some(v) => env::set_var("SQL_MCP_MODE", v),
            None => env::remove_var("SQL_MCP_MODE"),
        };

        env::remove_var("SQL_MCP_MODE");
        assert_eq!(McpMode::from_env(), McpMode::Query);

        env::set_var("SQL_MCP_MODE", "author");
        assert_eq!(McpMode::from_env(), McpMode::Author);

        env::set_var("SQL_MCP_MODE", "query");
        assert_eq!(McpMode::from_env(), McpMode::Query);

        restore();
    }

    #[test]
    fn target_description_accepts_name_alias() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cfg.toml");
        fs::write(
            &path,
            r#"
listen = "127.0.0.1:0"
[[targets]]
id = "dw"
name = "legacy display"
engine = "sqlite"
url_env = "SQL_MCP_TARGET_DW_URL"
"#,
        )
        .unwrap();
        let cfg = Config::load(&path).unwrap();
        assert_eq!(cfg.targets[0].id, "dw");
        assert_eq!(cfg.targets[0].description, "legacy display");
    }
}
