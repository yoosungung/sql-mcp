use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::extract::{Path as AxumPath, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use axum::Router;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::api::{self, ApiState};
use crate::config::Config;
use crate::semantic::SemanticStore;
use crate::target::TargetRegistry;
use crate::tools::SqlMcp;

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

pub fn default_ui_dir() -> PathBuf {
    let cwd = PathBuf::from("semantic-ui/dist");
    if cwd.join("index.html").is_file() {
        return cwd;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../semantic-ui/dist")
}

pub async fn serve(
    addr: SocketAddr,
    factory: Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>,
    store: SemanticStore,
    targets: TargetRegistry,
    cfg: Arc<Config>,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve_listener(listener, factory, store, targets, cfg, &default_ui_dir()).await
}

pub async fn serve_listener(
    listener: tokio::net::TcpListener,
    factory: Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>,
    store: SemanticStore,
    targets: TargetRegistry,
    cfg: Arc<Config>,
    ui_dir: &Path,
) -> anyhow::Result<()> {
    let addr = listener.local_addr()?;
    let ct = CancellationToken::new();
    let service = StreamableHttpService::new(
        {
            let factory = factory.clone();
            move || factory()
        },
        NeverSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_sse_keep_alive(None)
            .with_cancellation_token(ct.child_token())
            .with_allowed_hosts(vec![
                "127.0.0.1".into(),
                "localhost".into(),
                format!("127.0.0.1:{}", addr.port()),
                format!("localhost:{}", addr.port()),
            ]),
    );

    let api = api::router(ApiState {
        store,
        targets,
        cfg,
    });
    let ui = Router::new()
        .route("/ui", get(ui_index))
        .route("/ui/", get(ui_index))
        .route("/ui/{*path}", get(ui_asset))
        .with_state(ui_dir.to_path_buf());
    let router = Router::new()
        .route("/health", get(health))
        .merge(api)
        .merge(ui)
        .nest_service("/mcp", service);
    tracing::info!(%addr, "sql-mcp listening (stateless JSON HTTP)");
    fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
        let mut out = root.to_path_buf();
        for c in Path::new(rel).components() {
            match c {
                Component::Normal(s) => out.push(s),
                _ => return None,
            }
        }
        Some(out)
    }

    fn content_type(path: &Path) -> &'static str {
        match path.extension().and_then(|e| e.to_str()) {
            Some("js") => "text/javascript",
            Some("css") => "text/css",
            Some("svg") => "image/svg+xml",
            Some("json") => "application/json",
            Some("png") => "image/png",
            Some("woff2") => "font/woff2",
            _ => "text/html; charset=utf-8",
        }
    }

    fn ui_file(root: &Path, rel: &str) -> Response {
        let Some(path) = safe_join(root, rel) else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        match std::fs::read(&path) {
            Ok(bytes) => ([(header::CONTENT_TYPE, content_type(&path))], bytes).into_response(),
            Err(_) if path.extension().is_none() => ui_file(root, "index.html"),
            Err(_) => StatusCode::NOT_FOUND.into_response(),
        }
    }

    async fn ui_index(State(root): State<PathBuf>) -> Response {
        ui_file(&root, "index.html")
    }

    async fn ui_asset(State(root): State<PathBuf>, AxumPath(path): AxumPath<String>) -> Response {
        ui_file(&root, &path)
    }

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}
