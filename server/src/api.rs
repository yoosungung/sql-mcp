//! Authoring HTTP `/api` (D30). Not MCP tools.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::error::AppError;
use crate::semantic::SemanticStore;
use crate::target::TargetRegistry;

#[derive(Clone)]
pub struct ApiState {
    pub store: SemanticStore,
    pub targets: TargetRegistry,
    pub cfg: Arc<Config>,
}

#[derive(Debug, Deserialize)]
struct KindQuery {
    kind: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PhysicalQuery {
    schema: Option<String>,
    limit: Option<u32>,
}

type ApiError = (StatusCode, Json<Value>);

fn err(e: AppError) -> ApiError {
    let status = match &e {
        AppError::Validation(_) | AppError::NotSelect | AppError::SqlParse(_) => {
            StatusCode::BAD_REQUEST
        }
        AppError::NotFound(_) => StatusCode::NOT_FOUND,
        AppError::Forbidden(_) => StatusCode::FORBIDDEN,
        AppError::Timeout => StatusCode::GATEWAY_TIMEOUT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(e.to_json()))
}

pub fn router(state: ApiState) -> Router {
    Router::new()
        .route("/api/databases", get(list_databases))
        .route("/api/databases/{id}/tables", get(list_physical_tables))
        .route(
            "/api/databases/{id}/tables/{table}",
            get(describe_physical_table),
        )
        .route(
            "/api/databases/{id}/tables/{table}/columns/{column}/sample",
            get(sample_physical_column),
        )
        .route("/api/meta", get(meta_list).post(meta_create))
        .route(
            "/api/meta/{kind}/{name}",
            get(meta_get).put(meta_update).delete(meta_delete),
        )
        .with_state(state)
}

async fn list_databases(State(st): State<ApiState>) -> Json<Value> {
    Json(st.targets.list_json())
}

async fn list_physical_tables(
    State(st): State<ApiState>,
    Path(id): Path<String>,
    Query(q): Query<PhysicalQuery>,
) -> Result<Json<Value>, ApiError> {
    let db = st.targets.get(&id).map_err(err)?;
    let tables = db.list_tables(q.schema.as_deref()).await.map_err(err)?;
    Ok(Json(json!({
        "database": id,
        "items": tables.iter().map(|t| json!({
            "schema": t.schema,
            "name": t.name,
        })).collect::<Vec<_>>(),
    })))
}

async fn describe_physical_table(
    State(st): State<ApiState>,
    Path((id, table)): Path<(String, String)>,
    Query(q): Query<PhysicalQuery>,
) -> Result<Json<Value>, ApiError> {
    let db = st.targets.get(&id).map_err(err)?;
    let columns = db
        .describe_table(q.schema.as_deref(), &table)
        .await
        .map_err(err)?;
    Ok(Json(json!({
        "database": id,
        "schema": q.schema,
        "name": table,
        "columns": columns.iter().map(|c| json!({
            "name": c.name,
            "data_type": c.data_type,
            "nullable": c.nullable,
        })).collect::<Vec<_>>(),
    })))
}

async fn sample_physical_column(
    State(st): State<ApiState>,
    Path((id, table, column)): Path<(String, String, String)>,
    Query(q): Query<PhysicalQuery>,
) -> Result<Json<Value>, ApiError> {
    let db = st.targets.get(&id).map_err(err)?;
    let limit = q.limit.unwrap_or(st.cfg.sample_limit as u32) as usize;
    let values = db
        .sample_column(q.schema.as_deref(), &table, &column, limit)
        .await
        .map_err(err)?;
    Ok(Json(json!({
        "database": id,
        "schema": q.schema,
        "table": table,
        "column": column,
        "values": values,
    })))
}

async fn meta_list(
    State(st): State<ApiState>,
    Query(q): Query<KindQuery>,
) -> Result<Json<Value>, ApiError> {
    let items = st.store.list(q.kind.as_deref()).await.map_err(err)?;
    Ok(Json(json!({ "items": items })))
}

async fn meta_get(
    State(st): State<ApiState>,
    Path((kind, name)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(st.store.get(&kind, &name).await.map_err(err)?))
}

async fn meta_create(
    State(st): State<ApiState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(st.store.create(body).await.map_err(err)?))
}

async fn meta_update(
    State(st): State<ApiState>,
    Path((kind, name)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        st.store.update(&kind, &name, body).await.map_err(err)?,
    ))
}

async fn meta_delete(
    State(st): State<ApiState>,
    Path((kind, name)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    st.store.delete(&kind, &name).await.map_err(err)?;
    Ok(Json(json!({ "deleted": true })))
}
