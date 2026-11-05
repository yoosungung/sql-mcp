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
        let mut cfg: Config = toml::from_str(&raw)
            .map_err(|e| AppError::Validation(format!("invalid config {}: {e}", path.display())))?;
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

/// D29: comma-separated cube `target` ids. Unset or empty keeps fan-out/chasm checks off.
pub fn grain_guard_on(target: &str) -> bool {
    let Some(raw) = grain_guard_raw() else {
        return false;
    };
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|s| s == target)
}

fn grain_guard_raw() -> Option<String> {
    #[cfg(test)]
    {
        if let Some(over) = GRAIN_GUARD_OVERRIDE.with(|slot| slot.borrow().clone()) {
            return over.filter(|s| !s.is_empty());
        }
    }
    env::var("SQL_MCP_GRAIN_GUARD")
        .ok()
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
thread_local! {
    static GRAIN_GUARD_OVERRIDE: std::cell::RefCell<Option<Option<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Forces `SQL_MCP_GRAIN_GUARD` for this thread. `None` turns the guard off.
/// Parallel tests must not use `env::set_var` for this flag.
#[cfg(test)]
pub struct GrainGuardOverride {
    prev: Option<Option<String>>,
}

#[cfg(test)]
impl GrainGuardOverride {
    pub fn set(value: Option<&str>) -> Self {
        let prev = GRAIN_GUARD_OVERRIDE.with(|slot| slot.replace(Some(value.map(str::to_string))));
        Self { prev }
    }
}

#[cfg(test)]
impl Drop for GrainGuardOverride {
    fn drop(&mut self) {
        let prev = self.prev.take();
        GRAIN_GUARD_OVERRIDE.with(|slot| *slot.borrow_mut() = prev);
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
    fn grain_guard_matches_listed_targets_only() {
        let _off = GrainGuardOverride::set(None);
        assert!(!grain_guard_on("dw"));
        drop(_off);
        let _other = GrainGuardOverride::set(Some("other_db"));
        assert!(!grain_guard_on("dw"));
        drop(_other);
        let _listed = GrainGuardOverride::set(Some(" dw , other "));
        assert!(grain_guard_on("dw"));
        assert!(grain_guard_on("other"));
        assert!(!grain_guard_on("other_db"));
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
