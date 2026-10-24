use rmcp::service::ServiceExt;
use rmcp::transport::{ConfigureCommandExt, TokioChildProcess};
use tokio::process::Command;

#[tokio::test]
async fn stdio_initialize_and_list_tools() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("config.toml");
    std::fs::write(
        &cfg_path,
        r#"
listen = "127.0.0.1:0"
max_rows = 100
sample_limit = 10
timeout_ms = 5000
"#,
    )
    .unwrap();
    let semantic = dir.path().join("semantic.db");

    let transport = TokioChildProcess::new(Command::new(env!("CARGO_BIN_EXE_sql-mcp")).configure(
        |cmd| {
            cmd.arg("--stdio")
                .env("SQL_MCP_CONFIG", &cfg_path)
                .env("SQL_MCP_SEMANTIC_PATH", &semantic)
                .env("RUST_LOG", "error");
        },
    ))
    .expect("spawn sql-mcp --stdio");

    let client = ().serve(transport).await.expect("stdio initialize handshake");

    let tools = client
        .list_tools(Default::default())
        .await
        .expect("list_tools");
    let names: Vec<_> = tools.tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        names.iter().any(|n| *n == "list_databases"),
        "tools={names:?}"
    );

    client.cancel().await.expect("cancel");
}
