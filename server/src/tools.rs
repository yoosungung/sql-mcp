use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, ListResourcesResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerConfig,
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
    pub target: Option<String>,
    pub schema: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DescribeTableArgs {
    pub target: Option<String>,
    pub table: String,
    pub schema: Option<String>,
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
    pub table: String,
    pub column: String,
    pub schema: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteSqlArgs {
    pub target: String,
    pub sql: String,
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

    #[tool(description = "List tables from a target catalog, or semantic table docs if target omitted")]
    async fn list_tables(
        &self,
        Parameters(args): Parameters<ListTablesArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(target) = args.target.as_deref() {
            let db = match self.targets.get(target) {
                Ok(t) => t,
                Err(e) => return tool_err(e),
            };
            let tables = match db.list_tables(args.schema.as_deref()).await {
                Ok(t) => t,
                Err(e) => return tool_err(e),
            };
            let items: Vec<Value> = tables
                .into_iter()
                .map(|t| json!({"schema": t.schema, "name": t.name}))
                .collect();
            return ok_json(json!({ "items": items }));
        }
        let docs = match self.store.list(Some("table")).await {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        ok_json(json!({ "items": docs }))
    }

    #[tool(description = "Describe a table from a target, or a semantic table document")]
    async fn describe_table(
        &self,
        Parameters(args): Parameters<DescribeTableArgs>,
    ) -> Result<CallToolResult, McpError> {
        if let Some(target) = args.target.as_deref() {
            let db = match self.targets.get(target) {
                Ok(t) => t,
                Err(e) => return tool_err(e),
            };
            let cols = match db
                .describe_table(args.schema.as_deref(), &args.table)
                .await
            {
                Ok(c) => c,
                Err(e) => return tool_err(e),
            };
            let columns: Vec<Value> = cols
                .into_iter()
                .map(|c| {
                    json!({
                        "name": c.name,
                        "type": c.data_type,
                        "nullable": c.nullable,
                    })
                })
                .collect();
            return ok_json(json!({
                "table": args.table,
                "schema": args.schema,
                "columns": columns,
            }));
        }
        map_result(self.store.get("table", &args.table).await)
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

    #[tool(description = "Sample distinct-ish values from a target column")]
    async fn sample_column(
        &self,
        Parameters(args): Parameters<SampleColumnArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = match self.targets.get(&args.target) {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let limit = args
            .limit
            .map(|n| n as usize)
            .unwrap_or(self.cfg.sample_limit);
        let values = match db
            .sample_column(args.schema.as_deref(), &args.table, &args.column, limit)
            .await
        {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let docs = match self.store.list(None).await {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        let encrypted = crate::mask::is_encrypted_column(&docs, &args.column);
        let values = if encrypted {
            crate::mask::mask_sample_values(values)
        } else {
            values
        };
        let (is_code, language) = crate::code_detect::detect_code(&args.column, &values);
        let mut out = json!({
            "values": values,
            "is_code": is_code,
            "masked": encrypted,
        });
        if let Some(lang) = language {
            out["language"] = json!(lang);
        }
        ok_json(out)
    }

    #[tool(description = "Execute a single SELECT on a target database")]
    async fn execute_sql(
        &self,
        Parameters(args): Parameters<ExecuteSqlArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = match self.targets.get(&args.target) {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let result = match db.execute_select(&args.sql, self.cfg.max_rows).await {
            Ok(r) => r,
            Err(e) => return tool_err(e),
        };
        let docs = match self.store.list(None).await {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        let encrypted = crate::mask::encrypted_names(&docs);
        let rows = crate::mask::mask_result_rows(&result.columns, result.rows, &encrypted);
        ok_json(json!({
            "columns": result.columns,
            "rows": rows,
        }))
    }
}

#[tool_handler(name = "sql-mcp", version = "0.1.0", instructions = "Embedded libSQL semantic metadata + Postgres/SQLite target SELECT")]
impl ServerHandler for SqlMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::new("sql-mcp", "0.1.0"))
        .with_instructions(
            "Embedded libSQL semantic metadata + Postgres/SQLite target SELECT".to_string(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new(DOCUMENT_SCHEMA_URI, "document.schema.json")
                .with_description(
                    "JSON Schema for semantic registration documents (table/view/cube/metric/segment)",
                )
                .with_mime_type("application/schema+json")
                .with_size(DOCUMENT_SCHEMA_JSON.len() as u64),
        ]))
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
