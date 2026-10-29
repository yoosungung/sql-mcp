use std::path::Path;
use std::sync::Arc;

use libsql::{Builder, Connection};
use serde_json::{json, Value};
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
        let db = Builder::new_local(&path_str).build().await.map_err(|e| {
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
        migrate_dim_type_allow_id(&conn).await?;
        migrate_add_target_columns(&conn).await?;
        migrate_add_cube_role(&conn).await?;
        migrate_add_column_index_target(&conn).await?;
        migrate_add_member_descriptions(&conn).await?;
        migrate_add_view_source_catalog(&conn).await?;
        migrate_add_fact_additive(&conn).await?;
        migrate_add_view_join_relationship(&conn).await?;
        migrate_add_cube_grain(&conn).await?;
        migrate_add_cube_prefer(&conn).await?;
        migrate_surrogate_view_cube_ids(&conn).await?;
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
        let body = typed::normalize_join_hub_first(&body)?;
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
        let body = typed::normalize_join_hub_first(&body)?;
        if kind_of(&body)? != kind {
            return Err(AppError::Validation(
                "body.kind must match path kind".into(),
            ));
        }
        let new_name = name_of(&body)?;
        if kind == "table" && new_name != name {
            return Err(AppError::Validation(
                "body.kind/name must match path kind/name".into(),
            ));
        }
        let mut docs = self.list_all().await?;
        let pos = docs
            .iter()
            .position(|d| kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(name))
            .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))?;
        if new_name != name
            && docs.iter().any(|d| {
                kind_of(d).ok() == Some(kind) && name_of(d).ok() == Some(new_name)
            })
        {
            return Err(AppError::Validation(format!(
                "{kind}/{new_name} already exists"
            )));
        }
        rewrite_json_name_refs(&mut docs, kind, name, new_name);
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
                typed::replace_document(&conn, kind, name, &body).await?;
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
        self.search_columns_for_target(query, None, mode, limit)
            .await
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

/// JSON cache stays name-based. Storage FKs use ids, so rename does not
/// rewrite child rows — only the in-memory name pointers used for validate/GET.
fn rewrite_json_name_refs(docs: &mut [Value], kind: &str, old: &str, new: &str) {
    if old == new {
        return;
    }
    match kind {
        "view" => {
            for d in docs {
                if d.get("kind").and_then(|v| v.as_str()) != Some("cube") {
                    continue;
                }
                if d.get("view").and_then(|v| v.as_str()) == Some(old) {
                    d["view"] = json!(new);
                }
                if let Some(choices) = d.get_mut("choice").and_then(|v| v.as_array_mut()) {
                    for c in choices {
                        if c.get("view").and_then(|v| v.as_str()) == Some(old) {
                            c["view"] = json!(new);
                        }
                    }
                }
            }
        }
        "cube" => {
            for d in docs {
                let k = d.get("kind").and_then(|v| v.as_str());
                if k != Some("metric") && k != Some("segment") {
                    continue;
                }
                if d.get("cube").and_then(|v| v.as_str()) == Some(old) {
                    d["cube"] = json!(new);
                }
            }
        }
        _ => {}
    }
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

/// Older catalogs omit `id` from dimension type CHECK; rebuild (idempotent).
async fn migrate_dim_type_allow_id(conn: &Connection) -> Result<(), AppError> {
    let sql = {
        let mut rows = conn
            .query(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'meta_cube_dimensions'",
                (),
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let Some(row) = rows
            .next()
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        else {
            return Ok(());
        };
        let sql: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
        drop(rows);
        sql
    };
    if sql.contains("'id'") {
        conn.execute("DROP TABLE IF EXISTS meta_cube_dimensions_new", ())
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        return Ok(());
    }
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys=OFF;
        DROP TABLE IF EXISTS meta_cube_dimensions_new;
        CREATE TABLE meta_cube_dimensions_new (
          cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
          name TEXT NOT NULL,
          description TEXT NOT NULL,
          type TEXT CHECK (type IN ('id', 'code', 'date', 'time', 'datetime', 'string')),
          column_ref TEXT,
          expression TEXT,
          encryption INTEGER NOT NULL,
          as_code_json TEXT,
          role_json TEXT NOT NULL DEFAULT '"all"',
          PRIMARY KEY (cube_name, name)
        ) STRICT;
        INSERT INTO meta_cube_dimensions_new (cube_name, name, description, type, column_ref, expression, encryption, as_code_json, role_json)
        SELECT cube_name, name, description, type, column_ref, expression, encryption, as_code_json, role_json
        FROM meta_cube_dimensions;
        DROP TABLE meta_cube_dimensions;
        ALTER TABLE meta_cube_dimensions_new RENAME TO meta_cube_dimensions;
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
        let sql = format!("ALTER TABLE {table} ADD COLUMN target TEXT NOT NULL DEFAULT 'dw'");
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
async fn migrate_add_view_join_relationship(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "meta_view_joins", "relationship").await? {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE meta_view_joins ADD COLUMN relationship TEXT",
        (),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn migrate_add_cube_grain(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "meta_cubes", "grain_json").await? {
        return Ok(());
    }
    conn.execute("ALTER TABLE meta_cubes ADD COLUMN grain_json TEXT", ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn migrate_add_cube_prefer(conn: &Connection) -> Result<(), AppError> {
    if !table_has_column(conn, "meta_cubes", "prefer").await? {
        conn.execute("ALTER TABLE meta_cubes ADD COLUMN prefer TEXT", ())
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

async fn migrate_add_fact_additive(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "meta_cube_facts", "additive").await? {
        return Ok(());
    }
    conn.execute(
        "ALTER TABLE meta_cube_facts ADD COLUMN additive INTEGER NOT NULL DEFAULT 1",
        (),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn migrate_add_member_descriptions(conn: &Connection) -> Result<(), AppError> {
    for table in ["meta_cube_dimensions", "meta_cube_facts"] {
        if table_has_column(conn, table, "description").await? {
            continue;
        }
        let sql = format!("ALTER TABLE {table} ADD COLUMN description TEXT NOT NULL DEFAULT ''");
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

/// Pre-id catalogs used `name` as view/cube PK and name FKs on cubes/metrics.
/// Rebuild to `id` PK + UNIQUE `name`; JSON still exchanges names.
async fn migrate_surrogate_view_cube_ids(conn: &Connection) -> Result<(), AppError> {
    if table_has_column(conn, "meta_views", "id").await? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys=OFF;

        CREATE TABLE meta_views_bak AS SELECT * FROM meta_views;
        CREATE TABLE meta_view_sources_bak AS SELECT * FROM meta_view_sources;
        CREATE TABLE meta_view_joins_bak AS SELECT * FROM meta_view_joins;
        CREATE TABLE meta_view_columns_bak AS SELECT * FROM meta_view_columns;
        CREATE TABLE meta_cubes_bak AS SELECT * FROM meta_cubes;
        CREATE TABLE meta_cube_choices_bak AS SELECT * FROM meta_cube_choices;
        CREATE TABLE meta_cube_dimensions_bak AS SELECT * FROM meta_cube_dimensions;
        CREATE TABLE meta_cube_facts_bak AS SELECT * FROM meta_cube_facts;
        CREATE TABLE meta_metrics_bak AS SELECT * FROM meta_metrics;
        CREATE TABLE meta_segments_bak AS SELECT * FROM meta_segments;

        DROP TABLE meta_metrics;
        DROP TABLE meta_segments;
        DROP TABLE meta_cube_choices;
        DROP TABLE meta_cube_dimensions;
        DROP TABLE meta_cube_facts;
        DROP TABLE meta_cubes;
        DROP TABLE meta_view_columns;
        DROP TABLE meta_view_joins;
        DROP TABLE meta_view_sources;
        DROP TABLE meta_views;

        CREATE TABLE meta_views (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          name TEXT NOT NULL UNIQUE,
          description TEXT,
          target TEXT NOT NULL,
          combine TEXT NOT NULL CHECK (combine IN ('join', 'union', 'minus'))
        ) STRICT;
        INSERT INTO meta_views (name, description, target, combine)
        SELECT name, description, target, combine FROM meta_views_bak;

        CREATE TABLE meta_view_sources (
          view_id INTEGER NOT NULL REFERENCES meta_views(id) ON DELETE CASCADE,
          kind TEXT NOT NULL CHECK (kind IN ('table')),
          source_name TEXT NOT NULL,
          catalog_name TEXT,
          summary INTEGER NOT NULL DEFAULT 0
        ) STRICT;
        INSERT INTO meta_view_sources (view_id, kind, source_name, catalog_name, summary)
        SELECT v.id, s.kind, s.source_name, s.catalog_name, s.summary
        FROM meta_view_sources_bak s JOIN meta_views v ON v.name = s.view_name;

        CREATE TABLE meta_view_joins (
          view_id INTEGER NOT NULL REFERENCES meta_views(id) ON DELETE CASCADE,
          left_source TEXT NOT NULL,
          right_source TEXT NOT NULL,
          join_type TEXT NOT NULL CHECK (join_type IN ('inner', 'left', 'right', 'full')),
          on_conditions_json TEXT NOT NULL,
          relationship TEXT CHECK (relationship IS NULL OR relationship IN ('many_to_one', 'one_to_one', 'one_to_many'))
        ) STRICT;
        INSERT INTO meta_view_joins (view_id, left_source, right_source, join_type, on_conditions_json, relationship)
        SELECT v.id, j.left_source, j.right_source, j.join_type, j.on_conditions_json, j.relationship
        FROM meta_view_joins_bak j JOIN meta_views v ON v.name = j.view_name;

        CREATE TABLE meta_view_columns (
          view_id INTEGER NOT NULL REFERENCES meta_views(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          mappings_json TEXT NOT NULL,
          PRIMARY KEY (view_id, name)
        ) STRICT;
        INSERT INTO meta_view_columns (view_id, name, mappings_json)
        SELECT v.id, c.name, c.mappings_json
        FROM meta_view_columns_bak c JOIN meta_views v ON v.name = c.view_name;

        CREATE TABLE meta_cubes (
          id INTEGER PRIMARY KEY AUTOINCREMENT,
          name TEXT NOT NULL UNIQUE,
          target TEXT NOT NULL,
          view_id INTEGER NOT NULL REFERENCES meta_views(id),
          description TEXT NOT NULL,
          role_json TEXT NOT NULL DEFAULT '"all"',
          grain_json TEXT,
          prefer TEXT CHECK (prefer IS NULL OR prefer = 'summary')
        ) STRICT;
        INSERT INTO meta_cubes (name, target, view_id, description, role_json, grain_json, prefer)
        SELECT c.name, c.target, v.id, c.description, c.role_json, c.grain_json, c.prefer
        FROM meta_cubes_bak c JOIN meta_views v ON v.name = c.view_name;

        CREATE TABLE meta_cube_choices (
          cube_id INTEGER NOT NULL REFERENCES meta_cubes(id) ON DELETE CASCADE,
          pos INTEGER NOT NULL,
          view_id INTEGER NOT NULL REFERENCES meta_views(id),
          summary INTEGER NOT NULL DEFAULT 1,
          PRIMARY KEY (cube_id, pos)
        ) STRICT;
        INSERT INTO meta_cube_choices (cube_id, pos, view_id, summary)
        SELECT cu.id, ch.pos, v.id, ch.summary
        FROM meta_cube_choices_bak ch
        JOIN meta_cubes cu ON cu.name = ch.cube_name
        JOIN meta_views v ON v.name = ch.view_name;

        CREATE TABLE meta_cube_dimensions (
          cube_id INTEGER NOT NULL REFERENCES meta_cubes(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          description TEXT NOT NULL,
          type TEXT CHECK (type IN ('id', 'code', 'date', 'time', 'datetime', 'string')),
          column_ref TEXT,
          expression TEXT,
          encryption INTEGER NOT NULL,
          as_code_json TEXT,
          role_json TEXT NOT NULL DEFAULT '"all"',
          PRIMARY KEY (cube_id, name)
        ) STRICT;
        INSERT INTO meta_cube_dimensions (cube_id, name, description, type, column_ref, expression, encryption, as_code_json, role_json)
        SELECT cu.id, d.name, d.description, d.type, d.column_ref, d.expression, d.encryption, d.as_code_json, d.role_json
        FROM meta_cube_dimensions_bak d JOIN meta_cubes cu ON cu.name = d.cube_name;

        CREATE TABLE meta_cube_facts (
          cube_id INTEGER NOT NULL REFERENCES meta_cubes(id) ON DELETE CASCADE,
          name TEXT NOT NULL,
          description TEXT NOT NULL,
          type TEXT NOT NULL CHECK (type IN ('amount', 'fx', 'quantity')),
          column_ref TEXT,
          expression TEXT,
          encryption INTEGER NOT NULL,
          role_json TEXT NOT NULL DEFAULT '"all"',
          additive INTEGER NOT NULL DEFAULT 1,
          PRIMARY KEY (cube_id, name)
        ) STRICT;
        INSERT INTO meta_cube_facts (cube_id, name, description, type, column_ref, expression, encryption, role_json, additive)
        SELECT cu.id, f.name, f.description, f.type, f.column_ref, f.expression, f.encryption, f.role_json, f.additive
        FROM meta_cube_facts_bak f JOIN meta_cubes cu ON cu.name = f.cube_name;

        CREATE TABLE meta_metrics (
          name TEXT PRIMARY KEY,
          cube_id INTEGER NOT NULL REFERENCES meta_cubes(id),
          description TEXT NOT NULL,
          expression TEXT NOT NULL,
          aggregation TEXT CHECK (aggregation IN ('group', 'rollup', 'cube', 'grouping_sets')),
          group_by_json TEXT,
          filter_expression TEXT,
          role_json TEXT NOT NULL DEFAULT '"all"'
        ) STRICT;
        INSERT INTO meta_metrics (name, cube_id, description, expression, aggregation, group_by_json, filter_expression, role_json)
        SELECT m.name, cu.id, m.description, m.expression, m.aggregation, m.group_by_json, m.filter_expression, m.role_json
        FROM meta_metrics_bak m JOIN meta_cubes cu ON cu.name = m.cube_name;

        CREATE TABLE meta_segments (
          name TEXT PRIMARY KEY,
          cube_id INTEGER NOT NULL REFERENCES meta_cubes(id),
          description TEXT NOT NULL,
          filter_expression TEXT NOT NULL,
          role_json TEXT NOT NULL DEFAULT '"all"'
        ) STRICT;
        INSERT INTO meta_segments (name, cube_id, description, filter_expression, role_json)
        SELECT s.name, cu.id, s.description, s.filter_expression, s.role_json
        FROM meta_segments_bak s JOIN meta_cubes cu ON cu.name = s.cube_name;

        DROP TABLE meta_views_bak;
        DROP TABLE meta_view_sources_bak;
        DROP TABLE meta_view_joins_bak;
        DROP TABLE meta_view_columns_bak;
        DROP TABLE meta_cubes_bak;
        DROP TABLE meta_cube_choices_bak;
        DROP TABLE meta_cube_dimensions_bak;
        DROP TABLE meta_cube_facts_bak;
        DROP TABLE meta_metrics_bak;
        DROP TABLE meta_segments_bak;

        PRAGMA foreign_keys=ON;
        "#,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
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
        let customer: Value = serde_json::from_str(include_str!(
            "../../../semantic/examples/tables/customer.json"
        ))
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
        let orders: Value = serde_json::from_str(include_str!(
            "../../../semantic/examples/tables/orders.json"
        ))
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
        assert!(hits
            .iter()
            .any(|h| h.field_name == "region_code" && h.field_role == "dimension"));
    }

    #[tokio::test]
    async fn update_view_rejects_duplicate_column_as_validation() {
        let store = mem().await;
        for path in [
            include_str!("../../../semantic/examples/tables/customer.json"),
            include_str!("../../../semantic/examples/tables/region_codes.json"),
            include_str!("../../../semantic/examples/tables/status_codes.json"),
            include_str!("../../../semantic/examples/tables/orders.json"),
            include_str!("../../../semantic/examples/tables/orders_month.json"),
            include_str!("../../../semantic/examples/views/orders_customer.json"),
        ] {
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
        let mut view = store.get("view", "orders_customer").await.unwrap();
        let col = view["columns"][0].clone();
        view["columns"].as_array_mut().unwrap().push(col);
        let err = store
            .update("view", "orders_customer", view)
            .await
            .unwrap_err();
        match err {
            AppError::Validation(msg) => {
                assert!(msg.contains("duplicate column"), "{msg}");
            }
            other => panic!("expected Validation, got {other}"),
        }
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

    #[tokio::test]
    async fn cube_choice_roundtrip() {
        let store = mem().await;
        for path in [
            include_str!("../../../semantic/examples/tables/customer.json"),
            include_str!("../../../semantic/examples/tables/region_codes.json"),
            include_str!("../../../semantic/examples/tables/status_codes.json"),
            include_str!("../../../semantic/examples/tables/orders.json"),
            include_str!("../../../semantic/examples/tables/orders_month.json"),
            include_str!("../../../semantic/examples/views/orders_customer.json"),
            include_str!("../../../semantic/examples/cubes/sales.json"),
        ] {
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
        let month_view = json!({
            "kind": "view",
            "name": "orders_month_only",
            "target": "dw",
            "combine": "join",
            "sources": [{"kind": "table", "table": "orders_month"}],
            "joins": [],
            "columns": [
                {"name": "region_code", "mappings": [{"source": "orders_month", "column": "region_code"}]},
                {"name": "sale_amount", "mappings": [{"source": "orders_month", "column": "sale_amount"}]}
            ]
        });
        store.create(month_view).await.unwrap();
        let mut sales = store.get("cube", "sales").await.unwrap();
        sales["choice"] = json!([{ "view": "orders_month_only", "summary": true }]);
        sales["prefer"] = json!("summary");
        store.update("cube", "sales", sales).await.unwrap();
        let got = store.get("cube", "sales").await.unwrap();
        assert_eq!(got["prefer"], "summary");
        assert_eq!(got["choice"][0]["view"], "orders_month_only");
        assert_eq!(got["choice"][0]["summary"], true);
    }

    #[tokio::test]
    async fn create_view_normalizes_hub_to_sources_zero() {
        let store = mem().await;
        for doc in [
            json!({
                "kind": "table", "name": "superhero", "target": "dw", "db": "dw", "schema": "main", "table": "superhero",
                "columns": [{"name": "id", "type": "INT", "nullable": false}, {"name": "publisher_id", "type": "INT", "nullable": true}, {"name": "eye_colour_id", "type": "INT", "nullable": true}],
                "pk": ["id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
            }),
            json!({
                "kind": "table", "name": "publisher", "target": "dw", "db": "dw", "schema": "main", "table": "publisher",
                "columns": [{"name": "id", "type": "INT", "nullable": false}, {"name": "publisher_name", "type": "TEXT", "nullable": true}],
                "pk": ["id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
            }),
            json!({
                "kind": "table", "name": "colour", "target": "dw", "db": "dw", "schema": "main", "table": "colour",
                "columns": [{"name": "id", "type": "INT", "nullable": false}],
                "pk": ["id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
            }),
            json!({
                "kind": "table", "name": "hero_power", "target": "dw", "db": "dw", "schema": "main", "table": "hero_power",
                "columns": [{"name": "hero_id", "type": "INT", "nullable": false}, {"name": "power_id", "type": "INT", "nullable": false}],
                "pk": ["hero_id", "power_id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
            }),
            json!({
                "kind": "table", "name": "superpower", "target": "dw", "db": "dw", "schema": "main", "table": "superpower",
                "columns": [{"name": "id", "type": "INT", "nullable": false}],
                "pk": ["id"], "fk": [], "permission_filters": [], "scope_filter": null, "default_filter": null
            }),
        ] {
            store.create(doc).await.unwrap();
        }
        store
            .create(json!({
                "kind": "view", "name": "join_hero", "target": "dw", "combine": "join",
                "sources": [
                    {"kind": "table", "table": "superpower"},
                    {"kind": "table", "table": "hero_power"},
                    {"kind": "table", "table": "publisher"},
                    {"kind": "table", "table": "colour"},
                    {"kind": "table", "table": "superhero"}
                ],
                "joins": [
                    {"left": "superhero", "right": "publisher", "type": "inner",
                     "on": [{"left_column": "publisher_id", "right_column": "id"}]},
                    {"left": "superhero", "right": "colour", "type": "inner",
                     "on": [{"left_column": "eye_colour_id", "right_column": "id"}]},
                    {"left": "superhero", "right": "hero_power", "type": "inner",
                     "on": [{"left_column": "id", "right_column": "hero_id"}]},
                    {"left": "hero_power", "right": "superpower", "type": "inner",
                     "on": [{"left_column": "power_id", "right_column": "id"}]}
                ],
                "columns": [
                    {"name": "publisher_name", "mappings": [{"source": "publisher", "column": "publisher_name"}]},
                    {"name": "hero_id", "mappings": [
                        {"source": "hero_power", "column": "hero_id"},
                        {"source": "superhero", "column": "id"}
                    ]},
                    {"name": "power_id", "mappings": [
                        {"source": "superpower", "column": "id"},
                        {"source": "hero_power", "column": "power_id"}
                    ]}
                ]
            }))
            .await
            .unwrap();
        let got = store.get("view", "join_hero").await.unwrap();
        let names: Vec<&str> = got["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["table"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["superhero", "hero_power", "publisher", "colour", "superpower"],
            "save must order hub, depth-1, then depth-2: {}",
            got["sources"]
        );
        let hero_id = got["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "hero_id")
            .unwrap();
        assert_eq!(
            hero_id["mappings"][0]["source"].as_str(),
            Some("superhero"),
            "save must put hub first on shared hero_id: {}",
            hero_id["mappings"]
        );
        let power_id = got["columns"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "power_id")
            .unwrap();
        assert_eq!(
            [
                power_id["mappings"][0]["source"].as_str(),
                power_id["mappings"][1]["source"].as_str()
            ],
            [Some("hero_power"), Some("superpower")],
            "save must order power_id mappings by hop depth: {}",
            power_id["mappings"]
        );
    }

    async fn seed_sales_stack(store: &SemanticStore) {
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
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn put_view_keeps_attached_cube() {
        let store = mem().await;
        seed_sales_stack(&store).await;
        let mut view = store.get("view", "orders_customer").await.unwrap();
        view["description"] = json!("updated view");
        store
            .update("view", "orders_customer", view)
            .await
            .unwrap();
        let cube = store.get("cube", "sales").await.unwrap();
        assert_eq!(cube["view"], "orders_customer");
        assert!(cube.get("id").is_none());
        let metric = store.get("metric", "revenue").await.unwrap();
        assert_eq!(metric["cube"], "sales");
        assert!(metric.get("id").is_none());
    }

    #[tokio::test]
    async fn rename_view_keeps_cube_link_by_id() {
        let store = mem().await;
        seed_sales_stack(&store).await;
        let mut view = store.get("view", "orders_customer").await.unwrap();
        view["name"] = json!("orders_v2");
        store
            .update("view", "orders_customer", view)
            .await
            .unwrap();
        assert!(matches!(
            store.get("view", "orders_customer").await,
            Err(AppError::NotFound(_))
        ));
        let cube = store.get("cube", "sales").await.unwrap();
        assert_eq!(cube["view"], "orders_v2");
        let metric = store.get("metric", "revenue").await.unwrap();
        assert_eq!(metric["cube"], "sales");
    }

    #[tokio::test]
    async fn rename_cube_keeps_metric_and_segment_link_by_id() {
        let store = mem().await;
        seed_sales_stack(&store).await;
        let mut cube = store.get("cube", "sales").await.unwrap();
        cube["name"] = json!("sales_v2");
        store.update("cube", "sales", cube).await.unwrap();
        assert!(matches!(
            store.get("cube", "sales").await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(store.get("metric", "revenue").await.unwrap()["cube"], "sales_v2");
        assert_eq!(store.get("segment", "paid").await.unwrap()["cube"], "sales_v2");
    }

    #[tokio::test]
    async fn migrate_name_fks_to_surrogate_ids() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.db");
        {
            let db = Builder::new_local(&path).build().await.unwrap();
            let conn = db.connect().unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE meta_tables (
                  name TEXT PRIMARY KEY, description TEXT, target TEXT NOT NULL,
                  db TEXT NOT NULL, schema TEXT NOT NULL, table_name TEXT NOT NULL,
                  pk_json TEXT, fk_json TEXT, scope_filter TEXT, default_filter TEXT,
                  role_json TEXT NOT NULL DEFAULT '"all"'
                ) STRICT;
                CREATE TABLE meta_table_columns (
                  table_name TEXT NOT NULL REFERENCES meta_tables(name) ON DELETE CASCADE,
                  name TEXT NOT NULL, type TEXT NOT NULL, nullable INTEGER NOT NULL,
                  PRIMARY KEY (table_name, name)
                ) STRICT;
                CREATE TABLE meta_table_permission_filters (
                  table_name TEXT NOT NULL REFERENCES meta_tables(name) ON DELETE CASCADE,
                  role TEXT NOT NULL, expression TEXT NOT NULL
                ) STRICT;
                CREATE TABLE meta_views (
                  name TEXT PRIMARY KEY, description TEXT, target TEXT NOT NULL,
                  combine TEXT NOT NULL
                ) STRICT;
                CREATE TABLE meta_view_sources (
                  view_name TEXT NOT NULL REFERENCES meta_views(name) ON DELETE CASCADE,
                  kind TEXT NOT NULL, source_name TEXT NOT NULL, catalog_name TEXT,
                  summary INTEGER NOT NULL DEFAULT 0
                ) STRICT;
                CREATE TABLE meta_view_joins (
                  view_name TEXT NOT NULL REFERENCES meta_views(name) ON DELETE CASCADE,
                  left_source TEXT NOT NULL, right_source TEXT NOT NULL,
                  join_type TEXT NOT NULL, on_conditions_json TEXT NOT NULL, relationship TEXT
                ) STRICT;
                CREATE TABLE meta_view_columns (
                  view_name TEXT NOT NULL REFERENCES meta_views(name) ON DELETE CASCADE,
                  name TEXT NOT NULL, mappings_json TEXT NOT NULL,
                  PRIMARY KEY (view_name, name)
                ) STRICT;
                CREATE TABLE meta_cubes (
                  name TEXT PRIMARY KEY, target TEXT NOT NULL,
                  view_name TEXT NOT NULL REFERENCES meta_views(name),
                  description TEXT NOT NULL, role_json TEXT NOT NULL DEFAULT '"all"',
                  grain_json TEXT, prefer TEXT
                ) STRICT;
                CREATE TABLE meta_cube_choices (
                  cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
                  pos INTEGER NOT NULL,
                  view_name TEXT NOT NULL REFERENCES meta_views(name),
                  summary INTEGER NOT NULL DEFAULT 1,
                  PRIMARY KEY (cube_name, pos)
                ) STRICT;
                CREATE TABLE meta_cube_dimensions (
                  cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
                  name TEXT NOT NULL, description TEXT NOT NULL,
                  type TEXT CHECK (type IN ('id', 'code', 'date', 'time', 'datetime', 'string')),
                  column_ref TEXT, expression TEXT, encryption INTEGER NOT NULL,
                  as_code_json TEXT, role_json TEXT NOT NULL DEFAULT '"all"',
                  PRIMARY KEY (cube_name, name)
                ) STRICT;
                CREATE TABLE meta_cube_facts (
                  cube_name TEXT NOT NULL REFERENCES meta_cubes(name) ON DELETE CASCADE,
                  name TEXT NOT NULL, description TEXT NOT NULL,
                  type TEXT NOT NULL, column_ref TEXT, expression TEXT,
                  encryption INTEGER NOT NULL, role_json TEXT NOT NULL DEFAULT '"all"',
                  additive INTEGER NOT NULL DEFAULT 1,
                  PRIMARY KEY (cube_name, name)
                ) STRICT;
                CREATE TABLE meta_metrics (
                  name TEXT PRIMARY KEY,
                  cube_name TEXT NOT NULL REFERENCES meta_cubes(name),
                  description TEXT NOT NULL, expression TEXT NOT NULL,
                  aggregation TEXT, group_by_json TEXT, filter_expression TEXT,
                  role_json TEXT NOT NULL DEFAULT '"all"'
                ) STRICT;
                CREATE TABLE meta_segments (
                  name TEXT PRIMARY KEY,
                  cube_name TEXT NOT NULL REFERENCES meta_cubes(name),
                  description TEXT NOT NULL, filter_expression TEXT NOT NULL,
                  role_json TEXT NOT NULL DEFAULT '"all"'
                ) STRICT;
                CREATE TABLE column_index (
                  id INTEGER PRIMARY KEY AUTOINCREMENT, target TEXT NOT NULL DEFAULT 'dw',
                  doc_kind TEXT NOT NULL, doc_name TEXT NOT NULL, field_name TEXT NOT NULL,
                  field_role TEXT NOT NULL, text TEXT NOT NULL, embedding BLOB
                );
                INSERT INTO meta_views (name, target, combine) VALUES ('orders_customer', 'dw', 'join');
                INSERT INTO meta_view_sources (view_name, kind, source_name, catalog_name, summary)
                VALUES ('orders_customer', 'table', 'orders', 'orders', 0);
                INSERT INTO meta_view_columns (view_name, name, mappings_json)
                VALUES ('orders_customer', 'sale_amount', '[{"source":"orders","column":"sale_amount"}]');
                INSERT INTO meta_cubes (name, target, view_name, description)
                VALUES ('sales', 'dw', 'orders_customer', 'sales cube');
                INSERT INTO meta_cube_facts (cube_name, name, description, type, column_ref, encryption)
                VALUES ('sales', 'sale_amount', 'amount', 'amount', 'sale_amount', 0);
                INSERT INTO meta_metrics (name, cube_name, description, expression)
                VALUES ('revenue', 'sales', 'rev', 'sum(sale_amount)');
                "#,
            )
            .await
            .unwrap();
        }
        let store = SemanticStore::open(&path)
            .await
            .unwrap()
            .with_embedder(Arc::new(HashEmbedder::new(16)));
        let cube = store.get("cube", "sales").await.unwrap();
        assert_eq!(cube["view"], "orders_customer");
        assert!(cube.get("id").is_none());
        let metric = store.get("metric", "revenue").await.unwrap();
        assert_eq!(metric["cube"], "sales");
        let conn = store.conn.lock().await;
        assert!(table_has_column(&conn, "meta_views", "id").await.unwrap());
        assert!(table_has_column(&conn, "meta_cubes", "id").await.unwrap());
        assert!(table_has_column(&conn, "meta_cubes", "view_id").await.unwrap());
        assert!(table_has_column(&conn, "meta_metrics", "cube_id").await.unwrap());
    }
}
