use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, CustomRequest, CustomResult, ExtensionCapabilities,
    ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
    ReadResourceResult, Resource, ResourceContents, ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::Config;
use crate::error::AppError;
use crate::semantic::schema::{DOCUMENT_SCHEMA_JSON, DOCUMENT_SCHEMA_URI};
use crate::semantic::SemanticStore;
use crate::skill_resources::{list_skill_resources, read_skill_resource};
use crate::skills_ext::{self, SKILLS_EXTENSION_ID};
use crate::target::TargetRegistry;

#[derive(Clone)]
pub struct SqlMcp {
    pub store: SemanticStore,
    pub targets: TargetRegistry,
    pub cfg: Arc<Config>,
    #[allow(dead_code)] // read by rmcp #[tool_handler] generated code
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetadataBodyArgs {
    pub body: Value,
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct KindNameArgs {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetadataUpdateArgs {
    pub kind: String,
    pub name: String,
    pub body: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetadataListArgs {
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTablesArgs {
    /// Optional MCP target id; when set, only cubes with that `target` are returned.
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DescribeTableArgs {
    /// Cube name (DW agent "table").
    pub table: String,
    /// Optional MCP target id; when set, must match cube.target.
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchColumnsArgs {
    pub query: String,
    pub target: Option<String>,
    pub mode: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SampleColumnArgs {
    pub target: String,
    /// Cube name (semantic).
    pub cube: String,
    /// Dimension or fact name on the cube.
    pub member: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteSqlArgs {
    pub target: String,
    /// Semantic SQL only (cube FROM). Compiled to physical SQL before execution.
    pub sql: String,
    /// Caller role for permission_filters / cube role checks.
    pub caller_role: Option<String>,
    /// Bind values for `:name` placeholders (used in compiled SQL and union date pruning).
    pub params: Option<std::collections::HashMap<String, String>>,
    /// When true, compile only and return physical SQL without querying the target.
    pub dry_run: Option<bool>,
}

impl SqlMcp {
    pub fn new(store: SemanticStore, targets: TargetRegistry, cfg: Config) -> Self {
        Self {
            store,
            targets,
            cfg: Arc::new(cfg),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl SqlMcp {
    #[tool(description = "List configured target databases (no secrets)")]
    async fn list_targets(&self) -> Result<CallToolResult, McpError> {
        let items: Vec<Value> = self
            .targets
            .list()
            .iter()
            .map(|t| {
                json!({
                    "id": t.id,
                    "name": t.name,
                    "engine": t.engine,
                })
            })
            .collect();
        ok_json(json!({ "items": items }))
    }

    #[tool(description = "Create a semantic registration document (JSON)")]
    async fn metadata_create(
        &self,
        Parameters(args): Parameters<MetadataBodyArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(kind) = args.kind.as_deref() {
            if args.body.get("kind").and_then(|v| v.as_str()) != Some(kind) {
                return tool_err(AppError::Validation(
                    "optional kind must match body.kind".into(),
                ));
            }
        }
        map_result(self.store.create(args.body).await)
    }

    #[tool(description = "Get one semantic document by kind and name")]
    async fn metadata_get(
        &self,
        Parameters(args): Parameters<KindNameArgs>,
    ) -> Result<CallToolResult, McpError> {
        map_result(self.store.get(&args.kind, &args.name).await)
    }

    #[tool(description = "List semantic documents, optionally filtered by kind")]
    async fn metadata_list(
        &self,
        Parameters(args): Parameters<MetadataListArgs>,
    ) -> Result<CallToolResult, McpError> {
        let items = match self.store.list(args.kind.as_deref()).await {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        ok_json(json!({ "items": items }))
    }

    #[tool(description = "Replace a semantic document (full body replace)")]
    async fn metadata_update(
        &self,
        Parameters(args): Parameters<MetadataUpdateArgs>,
    ) -> Result<CallToolResult, McpError> {
        map_result(
            self.store
                .update(&args.kind, &args.name, args.body)
                .await,
        )
    }

    #[tool(description = "Delete a semantic document")]
    async fn metadata_delete(
        &self,
        Parameters(args): Parameters<KindNameArgs>,
    ) -> Result<CallToolResult, McpError> {
        match self.store.delete(&args.kind, &args.name).await {
            Ok(()) => ok_json(json!({ "deleted": true })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(description = "List semantic cubes (DW agent tables), optionally filtered by target")]
    async fn list_tables(
        &self,
        Parameters(args): Parameters<ListTablesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let docs = match self.store.list(Some("cube")).await {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let items: Vec<Value> = match args.target.as_deref() {
            Some(t) => docs
                .into_iter()
                .filter(|d| d.get("target").and_then(|v| v.as_str()) == Some(t))
                .collect(),
            None => docs,
        };
        ok_json(json!({ "items": items }))
    }

    #[tool(
        description = "Describe a semantic cube (dimensions, facts) plus linked metrics and segments"
    )]
    async fn describe_table(
        &self,
        Parameters(args): Parameters<DescribeTableArgs>,
    ) -> Result<CallToolResult, McpError> {
        let cube = match self.store.get("cube", &args.table).await {
            Ok(c) => c,
            Err(e) => return tool_err(e),
        };
        if let Some(want) = args.target.as_deref() {
            let got = cube.get("target").and_then(|v| v.as_str()).unwrap_or("");
            if got != want {
                return tool_err(AppError::Validation(format!(
                    "cube {} target {got} != requested target {want}",
                    args.table
                )));
            }
        }
        let metrics = match self.store.list(Some("metric")).await {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let segments = match self.store.list(Some("segment")).await {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let cube_name = args.table.as_str();
        let metrics: Vec<Value> = metrics
            .into_iter()
            .filter(|m| m.get("cube").and_then(|v| v.as_str()) == Some(cube_name))
            .collect();
        let segments: Vec<Value> = segments
            .into_iter()
            .filter(|s| s.get("cube").and_then(|v| v.as_str()) == Some(cube_name))
            .collect();
        let mut out = cube;
        out["metrics"] = json!(metrics);
        out["segments"] = json!(segments);
        ok_json(out)
    }

    #[tool(description = "Search columns via keyword (BM25), vector, or hybrid (RRF)")]
    async fn search_columns(
        &self,
        Parameters(args): Parameters<SearchColumnsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mode = args.mode.as_deref().unwrap_or("hybrid");
        let limit = args.limit.map(|n| n as usize).unwrap_or(20);
        let hits = match self.store.search_columns(&args.query, mode, limit).await {
            Ok(h) => h,
            Err(e) => return tool_err(e),
        };
        // optional target filter is reserved for target-catalog search; semantic index has no target id
        let _ = args.target;
        ok_json(crate::search::hits_to_json(&hits))
    }

    #[tool(
        description = "Sample values for a cube dimension/fact. Uses as_code dictionary when registered."
    )]
    async fn sample_column(
        &self,
        Parameters(args): Parameters<SampleColumnArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = match self.targets.get(&args.target) {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let docs = match self.store.list(None).await {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        if let Err(e) = assert_cube_target(&docs, &args.cube, &args.target) {
            return tool_err(e);
        }
        let limit = args
            .limit
            .map(|n| n as usize)
            .unwrap_or(self.cfg.sample_limit);
        let target = match crate::sample::resolve_sample_target(&docs, &args.cube, &args.member)
        {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let sql = match crate::sample::sample_sql(&target, limit) {
            Ok(s) => s,
            Err(e) => return tool_err(e),
        };
        let result = match db.execute_select(&sql, limit).await {
            Ok(r) => r,
            Err(e) => return tool_err(e),
        };
        let encrypted = crate::mask::is_encrypted_column(&docs, &args.member);
        let values: Value = if target.from_as_code {
            let rows: Vec<Value> = result
                .rows
                .into_iter()
                .map(|r| {
                    json!({
                        "key": r.first().cloned().unwrap_or(Value::Null),
                        "value": r.get(1).cloned().unwrap_or(Value::Null),
                    })
                })
                .collect();
            if encrypted {
                json!(rows
                    .into_iter()
                    .map(|_| json!({"key": null, "value": null}))
                    .collect::<Vec<_>>())
            } else {
                json!(rows)
            }
        } else {
            let vals: Vec<Value> = result
                .rows
                .into_iter()
                .filter_map(|r| r.into_iter().next())
                .collect();
            json!(if encrypted {
                crate::mask::mask_sample_values(vals)
            } else {
                vals
            })
        };
        ok_json(json!({
            "values": values,
            "as_code": target.from_as_code,
            "masked": encrypted,
            "resolved": target.resolved,
            "sql": sql,
        }))
    }

    #[tool(
        description = "Compile semantic SQL (cube/metric/segment) to physical SELECT and run on a target. Free physical SQL is not accepted. Set dry_run=true to compile only (no target query)."
    )]
    async fn execute_sql(
        &self,
        Parameters(args): Parameters<ExecuteSqlArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = match self.targets.get(&args.target) {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let docs = match self.store.list(None).await {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        let cube_names = match crate::pipeline::cube_names_from_semantic_sql(&args.sql) {
            Ok(n) => n,
            Err(e) => return tool_err(e),
        };
        for cube_name in &cube_names {
            if let Err(e) = assert_cube_target(&docs, cube_name, &args.target) {
                return tool_err(e);
            }
        }
        let physical = match crate::pipeline::compile_semantic_sql_with_binds(
            &docs,
            &args.sql,
            args.caller_role.as_deref(),
            args.params.as_ref(),
        ) {
            Ok(s) => s,
            Err(e) => return tool_err(e),
        };
        if args.dry_run.unwrap_or(false) {
            return ok_json(json!({
                "sql": physical,
                "dry_run": true,
            }));
        }
        let result = match db.execute_select(&physical, self.cfg.max_rows).await {
            Ok(r) => r,
            Err(e) => return tool_err(e),
        };
        let encrypted = crate::mask::encrypted_names(&docs);
        let rows = crate::mask::mask_result_rows(&result.columns, result.rows, &encrypted);
        ok_json(json!({
            "columns": result.columns,
            "rows": rows,
            "sql": physical,
        }))
    }
}

#[tool_handler(name = "sql-mcp", version = "0.1.0", instructions = "Embedded libSQL semantic metadata + Postgres/SQLite target SELECT")]
impl ServerHandler for SqlMcp {
    fn get_info(&self) -> ServerConfig {
        let mut extensions = ExtensionCapabilities::new();
        extensions.insert(
            SKILLS_EXTENSION_ID.to_string(),
            skills_ext::skills_extension_settings(),
        );
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_extensions_with(extensions)
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new("sql-mcp", "0.1.0"))
        .with_instructions(
            "Embedded libSQL semantic metadata + Postgres/SQLite target SELECT".to_string(),
        )
    }

    async fn on_custom_request(
        &self,
        request: CustomRequest,
        _context: RequestContext<RoleServer>,
    ) -> Result<CustomResult, McpError> {
        match skills_ext::handle_skills_custom(request.method.as_str(), request.params.as_ref()) {
            Some(Ok(v)) => Ok(CustomResult::new(v)),
            Some(Err(msg)) => Err(McpError::invalid_params(msg, None)),
            None => Err(McpError::new(
                rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                request.method,
                None,
            )),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut items = vec![Resource::new(DOCUMENT_SCHEMA_URI, "document.schema.json")
            .with_description(
                "JSON Schema for semantic registration documents (table/view/cube/metric/segment)",
            )
            .with_mime_type("application/schema+json")
            .with_size(DOCUMENT_SCHEMA_JSON.len() as u64)];
        items.extend(list_skill_resources());
        Ok(ListResourcesResult::with_all_items(items))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        if request.uri.as_str() == DOCUMENT_SCHEMA_URI {
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(DOCUMENT_SCHEMA_JSON, DOCUMENT_SCHEMA_URI)
                    .with_mime_type("application/schema+json"),
            ])
            .into());
        }
        if let Some((mime, text)) = read_skill_resource(request.uri.as_str()) {
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, request.uri.as_str()).with_mime_type(mime),
            ])
            .into());
        }
        Err(McpError::resource_not_found(
            format!("unknown resource: {}", request.uri),
            Some(json!({ "uri": request.uri })),
        ))
    }
}

fn ok_json(value: Value) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![ContentBlock::text(
        value.to_string(),
    )]))
}

fn map_result(result: Result<Value, AppError>) -> Result<CallToolResult, McpError> {
    match result {
        Ok(v) => ok_json(v),
        Err(e) => tool_err(e),
    }
}

fn tool_err(err: AppError) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        err.to_json().to_string(),
    )]))
}

fn assert_cube_target(docs: &[Value], cube_name: &str, target: &str) -> Result<(), AppError> {
    let cube = docs
        .iter()
        .find(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("cube")
                && d.get("name").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .ok_or_else(|| AppError::NotFound(format!("cube/{cube_name}")))?;
    let got = cube
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation(format!("cube {cube_name}: target missing")))?;
    if got != target {
        return Err(AppError::Validation(format!(
            "cube {cube_name} target {got} != requested target {target}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use crate::config::Config;
    use crate::target::{
        ColumnInfo, QueryResult, TableInfo, TargetDb, TargetInfo, TargetRegistry,
    };

    async fn seed_examples(store: &SemanticStore) {
        for path in [
            include_str!("../../semantic/examples/tables/customer.json"),
            include_str!("../../semantic/examples/tables/region_codes.json"),
            include_str!("../../semantic/examples/tables/status_codes.json"),
            include_str!("../../semantic/examples/tables/orders.json"),
            include_str!("../../semantic/examples/tables/orders_month.json"),
            include_str!("../../semantic/examples/views/orders_customer.json"),
            include_str!("../../semantic/examples/cubes/sales.json"),
            include_str!("../../semantic/examples/metrics/revenue.json"),
            include_str!("../../semantic/examples/segments/paid.json"),
        ] {
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
    }

    struct CountingBoomTarget {
        info: TargetInfo,
        selects: AtomicUsize,
    }

    #[async_trait]
    impl TargetDb for CountingBoomTarget {
        fn info(&self) -> TargetInfo {
            self.info.clone()
        }

        async fn list_tables(&self, _schema: Option<&str>) -> Result<Vec<TableInfo>, AppError> {
            Ok(vec![])
        }

        async fn describe_table(
            &self,
            _schema: Option<&str>,
            _table: &str,
        ) -> Result<Vec<ColumnInfo>, AppError> {
            Ok(vec![])
        }

        async fn execute_select(
            &self,
            _sql: &str,
            _max_rows: usize,
        ) -> Result<QueryResult, AppError> {
            self.selects.fetch_add(1, Ordering::SeqCst);
            Err(AppError::Validation("boom: select should not run".into()))
        }

        async fn sample_column(
            &self,
            _schema: Option<&str>,
            _table: &str,
            _column: &str,
            _limit: usize,
        ) -> Result<Vec<Value>, AppError> {
            Ok(vec![])
        }
    }

    fn tool_json(result: CallToolResult) -> Value {
        assert_ne!(result.is_error, Some(true), "{result:?}");
        let text = match result.content.first() {
            Some(ContentBlock::Text(t)) => t.text.as_str(),
            other => panic!("expected text content, got {other:?}"),
        };
        serde_json::from_str(text).expect("tool json")
    }

    #[tokio::test]
    async fn list_tables_returns_cubes() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let targets = TargetRegistry::from_config(&cfg).await.unwrap();
        let mcp = SqlMcp::new(store, targets, cfg);
        let docs = mcp.store.list(Some("cube")).await.unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0]["name"], "sales");
        assert_eq!(docs[0]["kind"], "cube");
        let filtered: Vec<_> = docs
            .iter()
            .filter(|d| d.get("target").and_then(|v| v.as_str()) == Some("dw"))
            .collect();
        assert_eq!(filtered.len(), 1);
        let none: Vec<_> = docs
            .iter()
            .filter(|d| d.get("target").and_then(|v| v.as_str()) == Some("nope"))
            .collect();
        assert!(none.is_empty());
        let _ = mcp;
    }

    #[tokio::test]
    async fn describe_assembles_metrics_segments() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let cube = store.get("cube", "sales").await.unwrap();
        let metrics = store.list(Some("metric")).await.unwrap();
        let segments = store.list(Some("segment")).await.unwrap();
        let metrics: Vec<_> = metrics
            .into_iter()
            .filter(|m| m.get("cube").and_then(|v| v.as_str()) == Some("sales"))
            .collect();
        let segments: Vec<_> = segments
            .into_iter()
            .filter(|s| s.get("cube").and_then(|v| v.as_str()) == Some("sales"))
            .collect();
        assert!(!cube["dimensions"].as_array().unwrap().is_empty());
        assert!(!cube["facts"].as_array().unwrap().is_empty());
        assert_eq!(metrics.len(), 1);
        assert_eq!(segments.len(), 1);
    }

    #[test]
    fn assert_cube_target_checks() {
        let docs = vec![json!({
            "kind": "cube",
            "name": "sales",
            "target": "dw",
        })];
        assert!(assert_cube_target(&docs, "sales", "dw").is_ok());
        assert!(matches!(
            assert_cube_target(&docs, "sales", "other"),
            Err(AppError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn execute_sql_dry_run_skips_target_select() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let boom = Arc::new(CountingBoomTarget {
            info: TargetInfo {
                id: "dw".into(),
                name: "boom".into(),
                engine: "sqlite".into(),
            },
            selects: AtomicUsize::new(0),
        });
        let mut targets = TargetRegistry::empty();
        targets.insert_for_test(boom.clone());
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let mcp = SqlMcp::new(store, targets, cfg);
        let result = mcp
            .execute_sql(Parameters(ExecuteSqlArgs {
                target: "dw".into(),
                sql: "SELECT revenue FROM sales WHERE paid".into(),
                caller_role: Some("branch".into()),
                params: None,
                dry_run: Some(true),
            }))
            .await
            .unwrap();
        let body = tool_json(result);
        assert_eq!(body["dry_run"], true);
        assert!(body.get("columns").is_none());
        assert!(body.get("rows").is_none());
        let sql = body["sql"].as_str().expect("sql");
        assert!(!sql.is_empty());
        assert!(sql.to_ascii_lowercase().contains("sum("));
        assert_eq!(boom.selects.load(Ordering::SeqCst), 0);

        let err = mcp
            .execute_sql(Parameters(ExecuteSqlArgs {
                target: "dw".into(),
                sql: "SELECT revenue FROM sales WHERE paid".into(),
                caller_role: Some("branch".into()),
                params: None,
                dry_run: None,
            }))
            .await
            .unwrap();
        assert_eq!(err.is_error, Some(true));
        assert_eq!(boom.selects.load(Ordering::SeqCst), 1);
    }
}
