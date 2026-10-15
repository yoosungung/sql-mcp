use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sql_mcp::config::Config;
use sql_mcp::{SemanticStore, SqlMcp, TargetRegistry};
use tokio::time::sleep;

async fn spawn_server() -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let store = SemanticStore::open(":memory:").await.unwrap();
    let cfg = Config {
        listen: addr.to_string(),
        max_rows: 100,
        sample_limit: 10,
        timeout_ms: 5_000,
        targets: vec![],
    };
    let cfg_arc = Arc::new(cfg.clone());
    let factory = {
        let store = store.clone();
        let cfg = cfg_arc.clone();
        Arc::new(move || {
            Ok(SqlMcp::new(
                store.clone(),
                TargetRegistry::empty(),
                (*cfg).clone(),
            ))
        }) as Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>
    };

    let handle = tokio::spawn(async move {
        let _ = sql_mcp::http::serve_listener(listener, factory, &cfg).await;
    });
    // wait until accept is ready
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    (format!("http://{addr}/mcp"), handle)
}

async fn post_rpc(url: &str, body: serde_json::Value) -> (u16, String) {
    let client = reqwest::Client::new();
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body)
        .send()
        .await
        .expect("send");
    let status = resp.status().as_u16();
    let text = resp.text().await.expect("text");
    (status, text)
}

#[tokio::test]
async fn health_endpoint_ok() {
    let (mcp_url, handle) = spawn_server().await;
    let base = mcp_url.trim_end_matches("/mcp");
    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{base}/health"))
        .send()
        .await
        .expect("health");
    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["ok"], true);
    handle.abort();
}

#[tokio::test]
async fn initialize_and_metadata_roundtrip() {
    let (url, handle) = spawn_server().await;

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2026-07-28",
                "capabilities": {},
                "clientInfo": {"name": "smoke", "version": "0.0.1"}
            }
        }),
    )
    .await;
    assert_eq!(status, 200, "initialize status body={body}");
    let init: serde_json::Value = serde_json::from_str(&body).expect(&format!("init json: {body}"));
    assert_eq!(init["jsonrpc"], "2.0");
    assert!(init["result"].is_object(), "{init}");

    let (status, _) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await;
    assert!(
        status == 200 || status == 202,
        "initialized notification status={status}"
    );

    let customer: serde_json::Value =
        serde_json::from_str(include_str!("../../semantic/examples/tables/customer.json"))
            .unwrap();

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "metadata_create",
                "arguments": {"body": customer}
            }
        }),
    )
    .await;
    assert_eq!(status, 200, "create status body={body}");
    let create: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("create json: {body}"));
    assert_ne!(create["result"]["isError"], true, "create failed: {create}");

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "metadata_list",
                "arguments": {"kind": "table"}
            }
        }),
    )
    .await;
    assert_eq!(status, 200, "list status body={body}");
    let listed: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("list json: {body}"));
    let text = listed["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    let payload: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(payload["items"].as_array().unwrap().len(), 1);

    handle.abort();
}

#[tokio::test]
#[ignore]
async fn postgres_execute_sql_ignored_without_env() {
    let url = std::env::var("SQL_MCP_TARGET_DW_URL").expect("set SQL_MCP_TARGET_DW_URL");
    assert!(!url.is_empty());
}
