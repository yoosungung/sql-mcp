use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use sql_mcp::cli;
use sql_mcp::embed;
use sql_mcp::{Config, SemanticStore, SqlMcp, TargetRegistry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

async fn run() -> Result<(), ExitCode> {
    let args: Vec<String> = std::env::args().collect();

    if cli::wants_help(args.iter().cloned()) {
        print!("{}", cli::HELP);
        return Ok(());
    }
    if let Some(flag) = cli::first_unknown_flag(args.iter().cloned()) {
        eprintln!("sql-mcp: unknown option {flag}\n");
        eprint!("{}", cli::HELP);
        return Err(ExitCode::from(2));
    }
    if let Some(arg) = cli::first_unexpected_arg(args.iter().cloned()) {
        eprintln!("sql-mcp: unexpected argument {arg}\n");
        eprint!("{}", cli::HELP);
        return Err(ExitCode::from(2));
    }

    // stderr only — stdout is the MCP stdio channel when --stdio is used.
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sql_mcp=debug".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true),
        )
        .init();

    let config_path = Config::config_path();
    let cfg = Config::load(&config_path).map_err(|e| startup_fail(&e))?;

    let semantic_path = Config::semantic_path();
    let mut store = SemanticStore::open(&semantic_path)
        .await
        .map_err(|e| startup_fail(&e))?;
    if let Some(embedder) = embed::from_env() {
        store = store.with_embedder(embedder);
    } else {
        tracing::info!("no embedding API configured; search_columns uses BM25 only (D14)");
    }
    let targets = TargetRegistry::from_config(&cfg).await.unwrap_or_else(|e| {
        tracing::warn!(
            error = %e,
            "starting with empty target registry (check [[targets]].url_env and DSN env vars; see --help)"
        );
        TargetRegistry::empty()
    });

    if sql_mcp::stdio::want_stdio(args) {
        let server = SqlMcp::new(store, targets, cfg);
        return sql_mcp::stdio::serve(server)
            .await
            .map_err(|e| fatal(format!("stdio serve failed: {e:#}")));
    }

    let addr: SocketAddr = cfg.listen.parse().map_err(|e| {
        fatal(format!(
            "invalid listen address {:?}: {e}\n  hint: set listen in config.toml or SQL_MCP_LISTEN (e.g. 127.0.0.1:8080); see --help",
            cfg.listen
        ))
    })?;
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

    sql_mcp::http::serve(addr, factory, &cfg)
        .await
        .map_err(|e| {
            fatal(format!(
                "cannot bind HTTP listen address {addr}: {e}\n  hint: free the port, set SQL_MCP_LISTEN, or use --stdio for local MCP; see --help"
            ))
        })
}

fn startup_fail(err: &sql_mcp::error::AppError) -> ExitCode {
    let msg = err.to_json()["message"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| err.to_string());
    fatal(msg)
}

fn fatal(msg: String) -> ExitCode {
    eprintln!("sql-mcp: {msg}");
    ExitCode::FAILURE
}
