use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::tools::SqlMcp;

pub async fn serve(
    addr: SocketAddr,
    factory: Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>,
    cfg: &Config,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve_listener(listener, factory, cfg).await
}

pub async fn serve_listener(
    listener: tokio::net::TcpListener,
    factory: Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>,
    cfg: &Config,
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

    let router = Router::new().nest_service("/mcp", service);
    tracing::info!(%addr, "sql-mcp listening (stateless JSON HTTP)");
    let _ = cfg;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}
