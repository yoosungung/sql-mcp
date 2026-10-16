use std::path::Path;
use std::sync::Arc;

use libsql::{Builder, Connection};
use serde_json::Value;
use tokio::sync::Mutex;

use crate::embed::Embedder;
use crate::error::AppError;
use crate::search::{self, SearchHit};

use super::links::validate_links;
use super::schema::validate_document;
use super::typed;

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
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        conn.execute_batch(CATALOG_SQL)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        migrate_drop_member_default_filter(&conn).await?;
        migrate_fact_type_from_amount_fx(&conn).await?;
        migrate_as_code_json(&conn).await?;
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
        {
            let conn = self.conn.lock().await;
            conn.execute_batch("BEGIN IMMEDIATE;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            if let Err(e) = typed::insert_document(&conn, &body).await {
                let _ = conn.execute_batch("ROLLBACK;").await;
                return Err(e);
            }
            conn.execute_batch("COMMIT;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }
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
        let mut docs = self.list_all().await?;
        let pos = docs
            .iter()
            .position(|d| kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(name))
            .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))?;
        docs[pos] = body.clone();
        validate_links(&docs)?;
        {
            let conn = self.conn.lock().await;
            conn.execute_batch("BEGIN IMMEDIATE;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            let del = typed::delete_kind(&conn, kind, name).await;
            if let Err(e) = del {
                let _ = conn.execute_batch("ROLLBACK;").await;
                return Err(e);
            }
            if let Err(e) = typed::insert_document(&conn, &body).await {
                let _ = conn.execute_batch("ROLLBACK;").await;
                return Err(e);
            }
            conn.execute_batch("COMMIT;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
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
            conn.execute_batch("BEGIN IMMEDIATE;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            match typed::delete_kind(&conn, kind, name).await {
                Ok(0) => {
                    let _ = conn.execute_batch("ROLLBACK;").await;
                    return Err(AppError::NotFound(format!("{kind}/{name}")));
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK;").await;
                    return Err(e);
                }
            }
            conn.execute_batch("COMMIT;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }
        self.reindex(&docs).await?;
        Ok(())
    }

    pub async fn get(&self, kind: &str, name: &str) -> Result<Value, AppError> {
        let conn = self.conn.lock().await;
        typed::load_document(&conn, kind, name).await
    }

    pub async fn list(&self, kind: Option<&str>) -> Result<Vec<Value>, AppError> {
        let conn = self.conn.lock().await;
        match kind {
            Some(k) => typed::list_kind(&conn, k).await,
            None => typed::list_all(&conn).await,
        }
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

/// Older catalogs had `default_filter` on dim/fact; drop if present (idempotent).
async fn migrate_drop_member_default_filter(conn: &Connection) -> Result<(), AppError> {
    for table in ["meta_cube_dimensions", "meta_cube_facts"] {
        if !table_has_column(conn, table, "default_filter").await? {
            continue;
        }
        conn.execute(
            &format!("ALTER TABLE {table} DROP COLUMN default_filter"),
            (),
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

/// Older catalogs used amount/fx booleans; rebuild to fact `type` enum (idempotent).
async fn migrate_fact_type_from_amount_fx(conn: &Connection) -> Result<(), AppError> {
    if !table_has_column(conn, "meta_cube_facts", "amount").await? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        CREATE TABLE meta_cube_facts_new (
          cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
          name TEXT NOT NULL,
          type TEXT NOT NULL CHECK (type IN ('amount', 'fx', 'quantity')),
          column_ref TEXT,
          expression TEXT,
          encryption INTEGER NOT NULL,
          role_json TEXT NOT NULL DEFAULT '"all"',
          PRIMARY KEY (cube_name, name)
        ) STRICT;
        INSERT INTO meta_cube_facts_new (cube_name, name, type, column_ref, expression, encryption, role_json)
        SELECT cube_name, name,
               CASE WHEN fx != 0 THEN 'fx' WHEN amount != 0 THEN 'amount' ELSE 'quantity' END,
               column_ref, expression, encryption, role_json
        FROM meta_cube_facts;
        DROP TABLE meta_cube_facts;
        ALTER TABLE meta_cube_facts_new RENAME TO meta_cube_facts;
        "#,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Move as_code onto dimensions: add dim as_code_json; drop table-column as_code(_json).
async fn migrate_as_code_json(conn: &Connection) -> Result<(), AppError> {
    if !table_has_column(conn, "meta_cube_dimensions", "as_code_json").await? {
        conn.execute(
            "ALTER TABLE meta_cube_dimensions ADD COLUMN as_code_json TEXT",
            (),
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    let drop_table_as_code = table_has_column(conn, "meta_table_columns", "as_code_json").await?
        || table_has_column(conn, "meta_table_columns", "as_code").await?;
    if drop_table_as_code {
        conn.execute_batch(
            r#"
            CREATE TABLE meta_table_columns_new (
              table_name TEXT NOT NULL REFERENCES meta_tables(name) ON DELETE CASCADE,
              name TEXT NOT NULL,
              type TEXT NOT NULL,
              nullable INTEGER NOT NULL,
              PRIMARY KEY (table_name, name)
            ) STRICT;
            INSERT INTO meta_table_columns_new (table_name, name, type, nullable)
            SELECT table_name, name, type, nullable FROM meta_table_columns;
            DROP TABLE meta_table_columns;
            ALTER TABLE meta_table_columns_new RENAME TO meta_table_columns;
            "#,
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

async fn table_has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, AppError> {
    let mut rows = conn
        .query(&format!("PRAGMA table_info({table})"), ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let name: String = row.get(1).map_err(|e| AppError::Internal(e.into()))?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
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
        for path in [
            include_str!("../../../semantic/examples/tables/customer.json"),
            include_str!("../../../semantic/examples/tables/region_codes.json"),
            include_str!("../../../semantic/examples/tables/status_codes.json"),
            include_str!("../../../semantic/examples/tables/orders.json"),
            include_str!("../../../semantic/examples/views/orders_customer.json"),
            include_str!("../../../semantic/examples/cubes/sales.json"),
        ] {
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
        let hits = store.search_columns("region", "keyword", 10).await.unwrap();
        assert!(hits.iter().any(|h| h.field_name == "region_code" && h.field_role == "dimension"));
    }

    #[tokio::test]
    async fn examples_roundtrip() {
        let store = mem().await;
        for path in [
            include_str!("../../../semantic/examples/tables/customer.json"),
            include_str!("../../../semantic/examples/tables/region_codes.json"),
            include_str!("../../../semantic/examples/tables/status_codes.json"),
            include_str!("../../../semantic/examples/tables/orders.json"),
            include_str!("../../../semantic/examples/tables/orders_month.json"),
            include_str!("../../../semantic/examples/views/orders_customer.json"),
            include_str!("../../../semantic/examples/cubes/sales.json"),
            include_str!("../../../semantic/examples/metrics/revenue.json"),
            include_str!("../../../semantic/examples/segments/paid.json"),
        ] {
            let doc: Value = serde_json::from_str(path).unwrap();
            store.create(doc.clone()).await.unwrap();
            let kind = doc["kind"].as_str().unwrap();
            let name = doc["name"].as_str().unwrap();
            let got = store.get(kind, name).await.unwrap();
            assert_eq!(got["kind"], doc["kind"]);
            assert_eq!(got["name"], doc["name"]);
        }
    }
}
