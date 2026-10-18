use std::path::Path;
use std::sync::Arc;

use libsql::{Builder, Connection};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};

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
    cache: Arc<RwLock<Option<Vec<Value>>>>,
}

impl SemanticStore {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let path = path.as_ref();
        let path_str = if path.as_os_str() == ":memory:" {
            ":memory:".to_string()
        } else {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        AppError::Validation(format!(
                            "cannot create semantic DB directory {}: {e} (set SQL_MCP_SEMANTIC_PATH; see --help)",
                            parent.display()
                        ))
                    })?;
                }
            }
            path.to_string_lossy().into_owned()
        };
        let db = Builder::new_local(&path_str)
            .build()
            .await
            .map_err(|e| {
                AppError::Connection(anyhow::anyhow!(
                    "cannot open semantic DB at {path_str}: {e}"
                ))
            })?;
        let conn = db.connect().map_err(|e| {
            AppError::Connection(anyhow::anyhow!(
                "cannot connect to semantic DB at {path_str}: {e}"
            ))
        })?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        conn.execute_batch(CATALOG_SQL)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        migrate_drop_member_default_filter(&conn).await?;
        migrate_fact_type_from_amount_fx(&conn).await?;
        migrate_as_code_json(&conn).await?;
        migrate_add_target_columns(&conn).await?;
        migrate_add_cube_role(&conn).await?;
        migrate_add_column_index_target(&conn).await?;
        migrate_add_member_descriptions(&conn).await?;
        migrate_add_view_source_catalog(&conn).await?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            embedder: None,
            cache: Arc::new(RwLock::new(None)),
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
            let embedder = self.embedder.as_deref();
            let index_rows = search::prepare_index_rows(&conn, &docs, embedder).await?;
            conn.execute_batch("BEGIN IMMEDIATE;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            let tx_res = async {
                typed::insert_document(&conn, &body).await?;
                search::write_prepared_index(&conn, &index_rows).await?;
                Ok::<(), AppError>(())
            }
            .await;
            if let Err(e) = tx_res {
                let _ = conn.execute_batch("ROLLBACK;").await;
                return Err(e);
            }
            conn.execute_batch("COMMIT;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }
        *self.cache.write().await = Some(docs);
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
            let embedder = self.embedder.as_deref();
            let index_rows = search::prepare_index_rows(&conn, &docs, embedder).await?;
            conn.execute_batch("BEGIN IMMEDIATE;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            let tx_res = async {
                typed::delete_kind(&conn, kind, name).await?;
                typed::insert_document(&conn, &body).await?;
                search::write_prepared_index(&conn, &index_rows).await?;
                Ok::<(), AppError>(())
            }
            .await;
            if let Err(e) = tx_res {
                let _ = conn.execute_batch("ROLLBACK;").await;
                return Err(e);
            }
            conn.execute_batch("COMMIT;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }
        *self.cache.write().await = Some(docs);
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
            let embedder = self.embedder.as_deref();
            let index_rows = search::prepare_index_rows(&conn, &docs, embedder).await?;
            conn.execute_batch("BEGIN IMMEDIATE;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            let tx_res = async {
                match typed::delete_kind(&conn, kind, name).await? {
                    0 => return Err(AppError::NotFound(format!("{kind}/{name}"))),
                    _ => {}
                }
                search::write_prepared_index(&conn, &index_rows).await?;
                Ok::<(), AppError>(())
            }
            .await;
            if let Err(e) = tx_res {
                let _ = conn.execute_batch("ROLLBACK;").await;
                return Err(e);
            }
            conn.execute_batch("COMMIT;")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }
        *self.cache.write().await = Some(docs);
        Ok(())
    }

    pub async fn get(&self, kind: &str, name: &str) -> Result<Value, AppError> {
        let docs = self.list_all().await?;
        docs.into_iter()
            .find(|d| kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(name))
            .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))
    }

    pub async fn list(&self, kind: Option<&str>) -> Result<Vec<Value>, AppError> {
        let docs = self.list_all().await?;
        match kind {
            Some(k) => {
                if !matches!(k, "table" | "view" | "cube" | "metric" | "segment") {
                    return Err(AppError::Validation(format!("unknown kind: {k}")));
                }
                Ok(docs
                    .into_iter()
                    .filter(|d| kind_of(d).ok() == Some(k))
                    .collect())
            }
            None => Ok(docs),
        }
    }

    pub async fn search_columns(
        &self,
        query: &str,
        mode: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        self.search_columns_for_target(query, None, mode, limit).await
    }

    pub async fn search_columns_for_target(
        &self,
        query: &str,
        target: Option<&str>,
        mode: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, AppError> {
        let embedder = self.embedder.as_deref();
        search::search_columns_filtered(&self.conn, query, target, mode, limit, embedder).await
    }

    async fn list_all(&self) -> Result<Vec<Value>, AppError> {
        if let Some(cached) = self.cache.read().await.as_ref() {
            return Ok(cached.clone());
        }
        let mut guard = self.cache.write().await;
        if let Some(cached) = guard.as_ref() {
            return Ok(cached.clone());
        }
        let conn = self.conn.lock().await;
        let docs = typed::list_all(&conn).await?;
        *guard = Some(docs.clone());
        Ok(docs)
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
        PRAGMA foreign_keys=OFF;
        CREATE TABLE meta_cube_facts_new (
          cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
          name TEXT NOT NULL,
          description TEXT NOT NULL DEFAULT '',
          type TEXT NOT NULL CHECK (type IN ('amount', 'fx', 'quantity')),
          column_ref TEXT,
          expression TEXT,
          encryption INTEGER NOT NULL,
          role_json TEXT NOT NULL DEFAULT '"all"',
          PRIMARY KEY (cube_name, name)
        ) STRICT;
        INSERT INTO meta_cube_facts_new (cube_name, name, description, type, column_ref, expression, encryption, role_json)
        SELECT cube_name, name, name,
               CASE WHEN fx != 0 THEN 'fx' WHEN amount != 0 THEN 'amount' ELSE 'quantity' END,
               column_ref, expression, encryption, role_json
        FROM meta_cube_facts;
        DROP TABLE meta_cube_facts;
        ALTER TABLE meta_cube_facts_new RENAME TO meta_cube_facts;
        PRAGMA foreign_keys=ON;
        "#,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Older catalogs lack MCP `target` on table/view/cube; add with default `dw`.
async fn migrate_add_target_columns(conn: &Connection) -> Result<(), AppError> {
    for table in ["meta_tables", "meta_views", "meta_cubes"] {
        if table_has_column(conn, table, "target").await? {
            continue;
        }
        // SQLite: ADD COLUMN with NOT NULL requires DEFAULT for existing rows.
        let sql = format!(
            "ALTER TABLE {table} ADD COLUMN target TEXT NOT NULL DEFAULT 'dw'"
        );
        conn.execute(&sql, ())
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

/// Older catalogs lack `role_json` on `meta_cubes`; add with default `"all"`.
async fn migrate_add_cube_role(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "meta_cubes", "role_json").await? {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE meta_cubes ADD COLUMN role_json TEXT NOT NULL DEFAULT '\"all\"'",
        (),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Older catalogs lack `target` on `column_index`; add with default `'dw'`.
async fn migrate_add_column_index_target(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "column_index", "target").await? {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE column_index ADD COLUMN target TEXT NOT NULL DEFAULT 'dw'",
        (),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Older catalogs lack `catalog_name` on `meta_view_sources` (alias vs catalog table).
async fn migrate_add_view_source_catalog(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "meta_view_sources", "catalog_name").await? {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE meta_view_sources ADD COLUMN catalog_name TEXT",
        (),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    // Legacy rows: source_name was the catalog key (no separate alias).
    conn.execute(
        "UPDATE meta_view_sources SET catalog_name = source_name WHERE catalog_name IS NULL",
        (),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Older catalogs lack required `description` on dim/fact; metric/segment may be NULL.
async fn migrate_add_member_descriptions(conn: &Connection) -> Result<(), AppError> {
    for table in ["meta_cube_dimensions", "meta_cube_facts"] {
        if table_has_column(conn, table, "description").await? {
            continue;
        }
        let sql = format!(
            "ALTER TABLE {table} ADD COLUMN description TEXT NOT NULL DEFAULT ''"
        );
        conn.execute(&sql, ())
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        conn.execute(
            &format!("UPDATE {table} SET description = name WHERE description = ''"),
            (),
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    for table in ["meta_metrics", "meta_segments"] {
        conn.execute(
            &format!(
                "UPDATE {table} SET description = name WHERE description IS NULL OR description = ''"
            ),
            (),
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
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
            PRAGMA foreign_keys=OFF;
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
            PRAGMA foreign_keys=ON;
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
