use std::net::SocketAddr;
use std::sync::Arc;

use sql_mcp::embed;
use sql_mcp::{Config, SemanticStore, SqlMcp, TargetRegistry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sql_mcp=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = Config::load(Config::config_path())?;
    let mut store = SemanticStore::open(Config::semantic_path()).await?;
    if let Some(embedder) = embed::from_env() {
        store = store.with_embedder(embedder);
    } else {
        tracing::info!("no embedding API configured; search_columns uses BM25 only (D14)");
    }
    let targets = TargetRegistry::from_config(&cfg).await.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "starting with empty target registry");
        TargetRegistry::empty()
    });

    let addr: SocketAddr = cfg.listen.parse()?;
    let cfg = Arc::new(cfg);
    let factory = {
        let store = store.clone();
        let targets = targets.clone();
        let cfg = cfg.clone();
        Arc::new(move || {
            Ok(SqlMcp::new(
                store.clone(),
                targets.clone(),
                (*cfg).clone(),
            ))
        }) as Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>
    };

    sql_mcp::http::serve(addr, factory, &cfg).await
}
