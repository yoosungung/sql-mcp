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
    pub id: String,
    pub name: String,
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
        let raw = fs::read_to_string(path).map_err(|e| AppError::Internal(e.into()))?;
        let mut cfg: Config = toml::from_str(&raw).map_err(|e| AppError::Validation(e.to_string()))?;
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

    pub fn readonly_metadata(&self) -> bool {
        env::var("SQL_MCP_READONLY_METADATA")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false)
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
