use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sql_mcp::config::Config;
use sql_mcp::target::{SqliteTarget, TargetInfo, TargetRegistry};
use sql_mcp::{SemanticStore, SqlMcp};
use tokio::time::sleep;

async fn spawn_server() -> (String, tokio::task::JoinHandle<()>) {
    spawn_server_with(TargetRegistry::empty()).await
}

async fn spawn_server_with(targets: TargetRegistry) -> (String, tokio::task::JoinHandle<()>) {
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
    let cfg_arc = Arc::new(cfg);
    let factory = {
        let store = store.clone();
        let targets = targets.clone();
        let cfg = cfg_arc.clone();
        Arc::new(move || Ok(SqlMcp::new(store.clone(), targets.clone(), (*cfg).clone())))
            as Arc<dyn Fn() -> Result<SqlMcp, std::io::Error> + Send + Sync>
    };

    let handle = tokio::spawn(async move {
        let _ = sql_mcp::http::serve_listener(
            listener,
            factory,
            store,
            targets,
            cfg_arc,
            &std::env::temp_dir(),
        )
        .await;
    });
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
async fn initialize_lists_query_tools_without_metadata() {
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

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }),
    )
    .await;
    assert_eq!(status, 200, "tools/list status body={body}");
    let listed: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("tools json: {body}"));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"list_databases"), "{names:?}");
    assert!(names.contains(&"execute_sql"), "{names:?}");
    assert!(
        names.iter().all(|n| !n.starts_with("metadata_")),
        "metadata tools must not be on MCP: {names:?}"
    );

    handle.abort();
}

#[tokio::test]
async fn api_meta_roundtrip() {
    let (mcp_url, handle) = spawn_server().await;
    let base = mcp_url.trim_end_matches("/mcp");
    let client = reqwest::Client::new();
    let customer: serde_json::Value =
        serde_json::from_str(include_str!("../../semantic/examples/tables/customer.json")).unwrap();

    let created = client
        .post(format!("{base}/api/meta"))
        .json(&customer)
        .send()
        .await
        .unwrap();
    assert_eq!(
        created.status().as_u16(),
        200,
        "{}",
        created.text().await.unwrap()
    );
    let listed = client
        .get(format!("{base}/api/meta?kind=table"))
        .send()
        .await
        .unwrap();
    assert_eq!(listed.status().as_u16(), 200);
    let payload: serde_json::Value = listed.json().await.unwrap();
    assert_eq!(payload["items"].as_array().unwrap().len(), 1);

    let got = client
        .get(format!("{base}/api/meta/table/customer"))
        .send()
        .await
        .unwrap();
    assert_eq!(got.status().as_u16(), 200);
    let doc: serde_json::Value = got.json().await.unwrap();
    assert_eq!(doc["name"], "customer");

    handle.abort();
}

#[tokio::test]
async fn api_physical_tables_and_sample() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("birdish.sqlite");
    let path_str = path.to_string_lossy().into_owned();
    {
        let db = libsql::Builder::new_local(&path_str).build().await.unwrap();
        let conn = db.connect().unwrap();
        conn.execute(
            r#"CREATE TABLE hero (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"#,
            (),
        )
        .await
        .unwrap();
        conn.execute(r#"INSERT INTO hero (id, name) VALUES (1, 'A')"#, ())
            .await
            .unwrap();
    }
    let info = TargetInfo {
        id: "t".into(),
        description: "test".into(),
        engine: "sqlite".into(),
    };
    let target = SqliteTarget::connect(info, &format!("sqlite:{path_str}?mode=ro"))
        .await
        .unwrap();
    let mut targets = TargetRegistry::empty();
    targets.insert_for_test(Arc::new(target));

    let (mcp_url, handle) = spawn_server_with(targets).await;
    let base = mcp_url.trim_end_matches("/mcp");
    let client = reqwest::Client::new();

    let dbs = client
        .get(format!("{base}/api/databases"))
        .send()
        .await
        .unwrap();
    assert_eq!(dbs.status().as_u16(), 200);
    let dbs: serde_json::Value = dbs.json().await.unwrap();
    assert_eq!(dbs["items"][0]["name"], "t");

    let tables = client
        .get(format!("{base}/api/databases/t/tables"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        tables.status().as_u16(),
        200,
        "{}",
        tables.text().await.unwrap()
    );
    let tables: serde_json::Value = tables.json().await.unwrap();
    assert_eq!(tables["items"][0]["name"], "hero");

    let cols = client
        .get(format!("{base}/api/databases/t/tables/hero"))
        .send()
        .await
        .unwrap();
    assert_eq!(cols.status().as_u16(), 200);
    let cols: serde_json::Value = cols.json().await.unwrap();
    assert!(
        cols["columns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["name"] == "name"),
        "{cols}"
    );

    let sample = client
        .get(format!(
            "{base}/api/databases/t/tables/hero/columns/name/sample"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(sample.status().as_u16(), 200);
    let sample: serde_json::Value = sample.json().await.unwrap();
    assert_eq!(sample["values"][0], "A");

    handle.abort();
}

#[tokio::test]
async fn ui_route_serves_index_and_asset() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("index.html"), b"<title>semantic-ui</title>").unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/app.js"), b"console.log(1)").unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let store = sql_mcp::SemanticStore::open(":memory:").await.unwrap();
    let cfg = sql_mcp::config::Config {
        listen: addr.to_string(),
        max_rows: 100,
        sample_limit: 10,
        timeout_ms: 5_000,
        targets: vec![],
    };
    let cfg_arc = Arc::new(cfg);
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
    let ui = dir.path().to_path_buf();
    let handle = tokio::spawn(async move {
        let _ = sql_mcp::http::serve_listener(
            listener,
            factory,
            store,
            TargetRegistry::empty(),
            cfg_arc,
            &ui,
        )
        .await;
    });
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(addr).await.is_ok() {
            break;
        }
        sleep(Duration::from_millis(20)).await;
    }
    let client = reqwest::Client::new();
    let index = client
        .get(format!("http://{addr}/ui/"))
        .send()
        .await
        .unwrap();
    assert_eq!(index.status().as_u16(), 200);
    let body = index.text().await.unwrap();
    assert!(body.contains("semantic-ui"), "{body}");
    let asset = client
        .get(format!("http://{addr}/ui/assets/app.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status().as_u16(), 200);
    assert!(asset.text().await.unwrap().contains("console.log"));
    let health = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status().as_u16(), 200);
    handle.abort();
}

#[tokio::test]
async fn document_schema_and_skill_resources_list_and_read() {
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
    assert!(
        init["result"]["capabilities"]["resources"].is_object(),
        "resources capability missing: {init}"
    );
    assert!(
        init["result"]["capabilities"]["extensions"]["io.modelcontextprotocol/skills"].is_object(),
        "skills extension missing: {init}"
    );

    let _ = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )
    .await;

    let schema_uri = "https://sql-mcp.local/semantic/document.schema.json";

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "resources/list",
            "params": {}
        }),
    )
    .await;
    assert_eq!(status, 200, "resources/list status body={body}");
    let listed: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("list json: {body}"));
    let resources = listed["result"]["resources"]
        .as_array()
        .expect("resources array");
    assert!(
        resources.len() > 1,
        "expected schema + skill resources, got {resources:?}"
    );
    let schema_res = resources
        .iter()
        .find(|r| r["uri"] == schema_uri)
        .expect("schema resource");
    assert_eq!(schema_res["name"], "document.schema.json");
    assert_eq!(schema_res["mimeType"], "application/schema+json");

    let skill_uri = "https://sql-mcp.local/skills/semantic-register/SKILL.md";
    let skill_res = resources
        .iter()
        .find(|r| r["uri"] == skill_uri)
        .expect("semantic-register SKILL.md resource");
    assert_eq!(skill_res["name"], "semantic-register/SKILL.md");
    assert_eq!(skill_res["mimeType"], "text/markdown");
    assert!(
        resources
            .iter()
            .any(|r| { r["uri"] == "https://sql-mcp.local/skills/semantic-table-view/SKILL.md" }),
        "missing table-view skill: {resources:?}"
    );
    assert!(
        resources.iter().any(|r| {
            r["uri"] == "https://sql-mcp.local/skills/semantic-cube-metric-segment/SKILL.md"
        }),
        "missing cube-metric-segment skill: {resources:?}"
    );
    assert!(
        resources
            .iter()
            .any(|r| { r["uri"] == "https://sql-mcp.local/skills/semantic-from-qa/SKILL.md" }),
        "missing from-qa skill: {resources:?}"
    );
    assert!(
        resources
            .iter()
            .any(|r| { r["uri"] == "https://sql-mcp.local/skills/semantic-from-eval/SKILL.md" }),
        "missing from-eval skill: {resources:?}"
    );

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "resources/read",
            "params": {"uri": schema_uri}
        }),
    )
    .await;
    assert_eq!(status, 200, "resources/read status body={body}");
    let read: serde_json::Value = serde_json::from_str(&body).expect(&format!("read json: {body}"));
    let text = read["result"]["contents"][0]["text"]
        .as_str()
        .expect("schema text");
    let schema: serde_json::Value = serde_json::from_str(text).expect("schema json");
    assert_eq!(schema["$id"], schema_uri);
    assert!(schema["$defs"]["table"].is_object(), "{schema}");

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/read",
            "params": {"uri": skill_uri}
        }),
    )
    .await;
    assert_eq!(status, 200, "skill resources/read status body={body}");
    let skill_read: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("skill read json: {body}"));
    let skill_text = skill_read["result"]["contents"][0]["text"]
        .as_str()
        .expect("skill text");
    assert!(
        skill_text.contains("name: semantic-register"),
        "unexpected skill body: {skill_text}"
    );
    assert_eq!(
        skill_read["result"]["contents"][0]["mimeType"],
        "text/markdown"
    );

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "skills/list",
            "params": {}
        }),
    )
    .await;
    assert_eq!(status, 200, "skills/list status body={body}");
    let skills_list: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("skills/list json: {body}"));
    assert!(
        skills_list.get("error").is_none(),
        "skills/list error: {skills_list}"
    );
    let skills = skills_list["result"]["skills"]
        .as_array()
        .expect("skills array");
    assert_eq!(skills.len(), 5, "{skills_list}");
    assert_eq!(skills_list["result"]["resultType"], "complete");

    let (status, body) = post_rpc(
        &url,
        json!({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "skills/get",
            "params": {"uri": skill_uri}
        }),
    )
    .await;
    assert_eq!(status, 200, "skills/get status body={body}");
    let skill_get: serde_json::Value =
        serde_json::from_str(&body).expect(&format!("skills/get json: {body}"));
    assert!(
        skill_get.get("error").is_none(),
        "skills/get error: {skill_get}"
    );
    assert_eq!(
        skill_get["result"]["skill"]["frontmatter"]["name"],
        "semantic-register"
    );

    handle.abort();
}

#[tokio::test]
#[ignore]
async fn postgres_execute_sql_ignored_without_env() {
    let url = std::env::var("SQL_MCP_TARGET_DW_URL").expect("set SQL_MCP_TARGET_DW_URL");
    assert!(!url.is_empty());
}

#[tokio::test]
#[ignore]
async fn mysql_dsn_smoke_ignored_without_env() {
    let url = std::env::var("SQL_MCP_TARGET_MYSQL_URL").expect("set SQL_MCP_TARGET_MYSQL_URL");
    assert!(!url.is_empty());
    assert!(
        url.starts_with("mysql://") || url.starts_with("mariadb://"),
        "expected mysql:// DSN"
    );
}

#[tokio::test]
#[ignore]
async fn oracle_dsn_smoke_ignored_without_env() {
    let url = std::env::var("SQL_MCP_TARGET_ORACLE_URL").expect("set SQL_MCP_TARGET_ORACLE_URL");
    assert!(!url.is_empty());
    assert!(
        url.contains('@') || url.starts_with("oracle://"),
        "expected user/pass@connect or oracle:// DSN"
    );
}
