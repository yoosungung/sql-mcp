use std::net::SocketAddr;
use std::sync::Arc;

use std::sync::Arc as StdArc;

use sql_mcp::embed::HashEmbedder;
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
    // D14: OpenAI-호환 API는 후속 배선. 기본은 결정적 HashEmbedder(오프라인·테스트 동일).
    let store = SemanticStore::open(Config::semantic_path())
        .await?
        .with_embedder(StdArc::new(HashEmbedder::new(64)));
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
