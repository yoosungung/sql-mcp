use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;

use crate::config::{Config, TargetConfig};
use crate::error::AppError;

use super::mysql::MysqlTarget;
use super::oracle::OracleTarget;
use super::postgres::PostgresTarget;
use super::sqlite::SqliteTarget;
use super::TargetDb;

#[derive(Debug, Clone, Serialize)]
pub struct TargetInfo {
    pub id: String,
    pub name: String,
    pub engine: String,
}

#[derive(Clone)]
pub struct TargetRegistry {
    targets: HashMap<String, Arc<dyn TargetDb>>,
    infos: Vec<TargetInfo>,
}

impl TargetRegistry {
    pub async fn from_config(cfg: &Config) -> Result<Self, AppError> {
        let mut targets: HashMap<String, Arc<dyn TargetDb>> = HashMap::new();
        let mut infos = Vec::new();
        for t in &cfg.targets {
            let info = TargetInfo {
                id: t.id.clone(),
                name: t.name.clone(),
                engine: t.engine.clone(),
            };
            let client = build_target(t, info.clone()).await?;
            targets.insert(t.id.clone(), client);
            infos.push(info);
        }
        Ok(Self { targets, infos })
    }

    pub fn empty() -> Self {
        Self {
            targets: HashMap::new(),
            infos: vec![],
        }
    }

    pub fn list(&self) -> &[TargetInfo] {
        &self.infos
    }

    pub fn get(&self, id: &str) -> Result<Arc<dyn TargetDb>, AppError> {
        self.targets
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("target {id}")))
    }

    #[cfg(test)]
    pub fn insert_for_test(&mut self, db: Arc<dyn TargetDb>) {
        let info = db.info();
        self.targets.insert(info.id.clone(), db);
        self.infos.push(info);
    }
}

async fn build_target(
    cfg: &TargetConfig,
    info: TargetInfo,
) -> Result<Arc<dyn TargetDb>, AppError> {
    let url = std::env::var(&cfg.url_env).map_err(|_| {
        AppError::Validation(format!(
            "missing env {} for target {}",
            cfg.url_env, cfg.id
        ))
    })?;
    match cfg.engine.as_str() {
        "postgres" => {
            let pg = PostgresTarget::connect(info, &url).await?;
            Ok(Arc::new(pg))
        }
        "mysql" => {
            let mysql = MysqlTarget::connect(info, &url).await?;
            Ok(Arc::new(mysql))
        }
        "sqlite" => {
            let sqlite = SqliteTarget::connect(info, &url).await?;
            Ok(Arc::new(sqlite))
        }
        "oracle" => {
            let oracle = OracleTarget::connect(info, &url).await?;
            Ok(Arc::new(oracle))
        }
        other => Err(AppError::Validation(format!(
            "unsupported target engine: {other}"
        ))),
    }
}

/// Marker used by tools for SELECT execution entry.
pub struct TargetSql;
