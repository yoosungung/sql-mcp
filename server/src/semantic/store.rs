use std::path::Path;
use std::sync::Arc;

use libsql::{Builder, Connection, params};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::embed::Embedder;
use crate::error::AppError;
use crate::search::{self, SearchHit};

use super::links::validate_links;
use super::schema::validate_document;

const CATALOG_SQL: &str = include_str!("../../../semantic/libsql/catalog.sql");

#[derive(Clone)]
pub struct SemanticStore {
    conn: Arc<Mutex<Connection>>,
    embedder: Option<Arc<dyn Embedder>>,
}

impl SemanticStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref();
        let path_str = if path.as_os_str() == ":memory:" {
            ":memory:".to_string()
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.into()))?;
            }
            path.to_string_lossy().into_owned()
        };
        let db = Builder::new_local(&path_str)
            .build()
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        let conn = db
            .connect()
            .map_err(|e| AppError::Connection(e.into()))?;
        conn.execute_batch(CATALOG_SQL)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            embedder: None,
        })
    }

    pub fn with_embedder(mut self, embedder: Arc<dyn Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    pub fn conn_arc(&self) -> Arc<Mutex<Connection>> {
        self.conn.clone()
    }

    pub async fn create(&self, body: Value) -> Result<Value, AppError> {
        validate_document(&body)?;
        let kind = kind_of(&body)?;
        let name = name_of(&body)?;
        let description = description_of(&body);
        let mut docs = self.list_all().await?;
        if docs
            .iter()
            .any(|d| kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(name))
        {
            return Err(AppError::Validation(format!(
                "{kind}/{name} already exists"
            )));
        }
        docs.push(body.clone());
        validate_links(&docs)?;
        self.insert_row(kind, name, description.as_deref(), &body)
            .await?;
        self.reindex(&docs).await?;
        Ok(body)
    }

    pub async fn update(&self, kind: &str, name: &str, body: Value) -> Result<Value, AppError> {
        validate_document(&body)?;
        if kind_of(&body)? != kind || name_of(&body)? != name {
            return Err(AppError::Validation(
                "body.kind/name must match path kind/name".into(),
            ));
        }
        let description = description_of(&body);
        let mut docs = self.list_all().await?;
        let pos = docs
            .iter()
            .position(|d| kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(name))
            .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))?;
        docs[pos] = body.clone();
        validate_links(&docs)?;
        {
            let conn = self.conn.lock().await;
            let n = conn
                .execute(
                    "UPDATE document SET description = ?1, body = ?2 WHERE kind = ?3 AND name = ?4",
                    params![description, body.to_string(), kind, name],
                )
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            if n == 0 {
                return Err(AppError::NotFound(format!("{kind}/{name}")));
            }
        }
        self.reindex(&docs).await?;
        Ok(body)
    }

    pub async fn delete(&self, kind: &str, name: &str) -> Result<(), AppError> {
        let mut docs = self.list_all().await?;
        let before = docs.len();
        docs.retain(|d| !(kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(name)));
        if docs.len() == before {
            return Err(AppError::NotFound(format!("{kind}/{name}")));
        }
        validate_links(&docs)?;
        {
            let conn = self.conn.lock().await;
            let n = conn
                .execute(
                    "DELETE FROM document WHERE kind = ?1 AND name = ?2",
                    params![kind, name],
                )
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            if n == 0 {
                return Err(AppError::NotFound(format!("{kind}/{name}")));
            }
        }
        self.reindex(&docs).await?;
        Ok(())
    }

    pub async fn get(&self, kind: &str, name: &str) -> Result<Value, AppError> {
        let conn = self.conn.lock().await;
        let mut rows = conn
            .query(
                "SELECT body FROM document WHERE kind = ?1 AND name = ?2",
                params![kind, name],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let row = rows
            .next()
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))?;
        let body: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
        serde_json::from_str(&body).map_err(|e| AppError::Internal(e.into()))
    }

    pub async fn list(&self, kind: Option<&str>) -> Result<Vec<Value>, AppError> {
        let conn = self.conn.lock().await;
        let mut rows = if let Some(kind) = kind {
            conn.query(
                "SELECT body FROM document WHERE kind = ?1 ORDER BY name",
                params![kind],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        } else {
            conn.query("SELECT body FROM document ORDER BY kind, name", ())
                .await
                .map_err(|e| AppError::Internal(e.into()))?
        };
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        {
            let body: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
            out.push(serde_json::from_str(&body).map_err(|e| AppError::Internal(e.into()))?);
        }
        Ok(out)
    }

    pub async fn search_columns(
        &self,
        query: &str,
        mode: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        let embedder = self.embedder.as_deref();
        search::search_columns(&self.conn, query, mode, limit, embedder).await
    }

    async fn reindex(&self, docs: &[Value]) -> Result<(), AppError> {
        let conn = self.conn.lock().await;
        let embedder = self.embedder.as_deref();
        search::rebuild_index(&conn, docs, embedder).await
    }

    async fn list_all(&self) -> Result<Vec<Value>, AppError> {
        self.list(None).await
    }

    async fn insert_row(
        &self,
        kind: &str,
        name: &str,
        description: Option<&str>,
        body: &Value,
    ) -> Result<(), AppError> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO document (kind, name, description, body) VALUES (?1, ?2, ?3, ?4)",
            params![kind, name, description, body.to_string()],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        Ok(())
    }
}

fn kind_of(body: &Value) -> Result<&str, AppError> {
    body.get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("kind required".into()))
}

fn name_of(body: &Value) -> Result<&str, AppError> {
    body.get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("name required".into()))
}

fn description_of(body: &Value) -> Option<String> {
    body.get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::HashEmbedder;
    use serde_json::json;

    async fn mem() -> SemanticStore {
        SemanticStore::open(":memory:")
            .await
            .unwrap()
            .with_embedder(Arc::new(HashEmbedder::new(16)))
    }

    #[tokio::test]
    async fn open_file_twice_applies_catalog_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("semantic.db");
        {
            let store = SemanticStore::open(&path).await.unwrap();
            let customer: Value = serde_json::from_str(include_str!(
                "../../../semantic/examples/tables/customer.json"
            ))
            .unwrap();
            store.create(customer).await.unwrap();
        }
        let store = SemanticStore::open(&path).await.unwrap();
        let got = store.get("table", "customer").await.unwrap();
        assert_eq!(got["name"], "customer");
    }

    #[tokio::test]
    async fn create_get_list_update_delete() {
        let store = mem().await;
        let customer: Value =
            serde_json::from_str(include_str!("../../../semantic/examples/tables/customer.json"))
                .unwrap();
        store.create(customer.clone()).await.unwrap();
        let got = store.get("table", "customer").await.unwrap();
        assert_eq!(got["name"], "customer");
        let listed = store.list(Some("table")).await.unwrap();
        assert_eq!(listed.len(), 1);

        let mut updated = customer.clone();
        updated["description"] = json!("updated");
        store
            .update("table", "customer", updated.clone())
            .await
            .unwrap();
        let got = store.get("table", "customer").await.unwrap();
        assert_eq!(got["description"], "updated");

        store.delete("table", "customer").await.unwrap();
        assert!(matches!(
            store.get("table", "customer").await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn create_rejects_broken_fk_without_writing() {
        let store = mem().await;
        let orders: Value =
            serde_json::from_str(include_str!("../../../semantic/examples/tables/orders.json"))
                .unwrap();
        assert!(matches!(
            store.create(orders).await,
            Err(AppError::Validation(_))
        ));
        assert!(store.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_after_create() {
        let store = mem().await;
        let customer: Value =
            serde_json::from_str(include_str!("../../../semantic/examples/tables/customer.json"))
                .unwrap();
        store.create(customer).await.unwrap();
        let hits = store.search_columns("region", "keyword", 10).await.unwrap();
        assert!(hits.iter().any(|h| h.field_name == "region_code"));
    }
}
