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

use crate::config::{Config, McpMode};
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
    pub mode: McpMode,
    pub default_role: Option<String>,
    #[allow(dead_code)] // read by rmcp #[tool_handler] generated code
    tool_router: rmcp::handler::server::router::tool::ToolRouter<Self>,
}

/// Cursor rejects boolean JSON Schema (`true`) for tool `inputSchema` properties.
fn json_object_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": true
    })
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetadataBodyArgs {
    /// Semantic registration document (JSON object).
    #[schemars(schema_with = "json_object_schema")]
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
    /// Semantic registration document (JSON object).
    #[schemars(schema_with = "json_object_schema")]
    pub body: Value,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MetadataListArgs {
    pub kind: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTablesArgs {
    /// Database name from `list_databases`. When set, only that database's tables are listed.
    pub database: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct DescribeTableArgs {
    /// Table name from `list_tables` or `search_columns`.
    pub table: String,
    /// Database name from `list_databases`. Use when the same table name could exist in more than one database.
    pub database: Option<String>,
    /// Your access role; only columns you may use are returned.
    #[serde(default)]
    pub caller_role: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SearchColumnsArgs {
    /// Business term or column/table keyword to search for.
    pub query: String,
    /// Limit search to one database from `list_databases`.
    pub database: Option<String>,
    /// Search mode: `hybrid` (default), `keyword`, or `vector`.
    pub mode: Option<String>,
    /// Max hits to return (default 20).
    pub limit: Option<u32>,
    /// Your access role; hits you cannot use are omitted.
    #[serde(default)]
    pub caller_role: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SampleColumnArgs {
    /// Database name from `list_databases`.
    pub database: String,
    /// Table name from `list_tables` / `search_columns` / `describe_table`.
    pub table: String,
    /// Dimension or fact name from `describe_table` / `search_columns` (not a metric or segment).
    pub name: String,
    /// Max sample values (server default if omitted).
    pub limit: Option<u32>,
    /// Your access role for row/column permissions.
    #[serde(default)]
    pub caller_role: Option<String>,
    /// Values for `:name` placeholders used in security filters.
    #[serde(default)]
    pub params: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ExecuteSqlArgs {
    /// Database name from `list_databases`.
    pub database: String,
    /// Analytical SELECT using table names and their dimensions/facts (ad-hoc aggregates OK). FROM must be a listed table name or subquery/CTE over them.
    pub sql: String,
    /// Your access role for row/column permissions.
    pub caller_role: Option<String>,
    /// Values for `:name` placeholders in filters.
    pub params: Option<std::collections::HashMap<String, String>>,
    /// If true, do not fetch rows; return only the compiled SQL for inspection.
    pub dry_run: Option<bool>,
}

impl SqlMcp {
    pub fn new(store: SemanticStore, targets: TargetRegistry, cfg: Config) -> Self {
        let mode = cfg.mode();
        let default_role = cfg.default_role();
        Self {
            store,
            targets,
            cfg: Arc::new(cfg),
            mode,
            default_role,
            tool_router: Self::tool_router(),
        }
    }

    pub fn with_mode(mut self, mode: McpMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_default_role(mut self, role: impl Into<String>) -> Self {
        self.default_role = Some(role.into());
        self
    }

    fn effective_role<'a>(&'a self, caller_role: Option<&'a str>) -> Option<&'a str> {
        caller_role.or(self.default_role.as_deref())
    }
}

#[tool_router]
impl SqlMcp {
    #[tool(
        description = "List databases you can query (name, description, engine). Call first when you do not know which database to use; pass the returned name as `database` to the other tools."
    )]
    async fn list_databases(&self) -> Result<CallToolResult, McpError> {
        let items: Vec<Value> = self
            .targets
            .list()
            .iter()
            .map(|t| {
                json!({
                    "name": t.id,
                    "description": t.description,
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
        if !self.mode.allows_metadata_mutation() {
            return tool_err(AppError::Forbidden(
                "metadata mutation is disabled (SQL_MCP_MODE=query)".into(),
            ));
        }
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
        if !self.mode.allows_metadata_mutation() {
            return tool_err(AppError::Forbidden(
                "metadata mutation is disabled (SQL_MCP_MODE=query)".into(),
            ));
        }
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
        if !self.mode.allows_metadata_mutation() {
            return tool_err(AppError::Forbidden(
                "metadata mutation is disabled (SQL_MCP_MODE=query)".into(),
            ));
        }
        match self.store.delete(&args.kind, &args.name).await {
            Ok(()) => ok_json(json!({ "deleted": true })),
            Err(e) => tool_err(e),
        }
    }

    #[tool(
        description = "List tables in a database (name and description only). Use to browse what you can query; then call describe_table for columns. Optional `database` filters to one database from list_databases."
    )]
    async fn list_tables(
        &self,
        Parameters(args): Parameters<ListTablesArgs>,
    ) -> Result<CallToolResult, McpError> {
        let docs = match self.store.list(Some("cube")).await {
            Ok(v) => v,
            Err(e) => return tool_err(e),
        };
        let filtered = match args.database.as_deref() {
            Some(t) => docs
                .into_iter()
                .filter(|d| d.get("target").and_then(|v| v.as_str()) == Some(t))
                .collect::<Vec<_>>(),
            None => docs,
        };
        let items: Vec<Value> = filtered
            .into_iter()
            .map(|d| {
                json!({
                    "database": d.get("target").and_then(|v| v.as_str()).unwrap_or(""),
                    "name": d.get("name").and_then(|v| v.as_str()).unwrap_or(""),
                    "description": d.get("description").and_then(|v| v.as_str()).unwrap_or(""),
                })
            })
            .collect();
        ok_json(json!({ "items": items }))
    }

    #[tool(
        description = "Describe one table: dimensions and facts (SQL columns), plus optional examples (metric/segment expression snippets to copy into ad-hoc SELECT—not column names). Call after list_tables or search_columns when you need column details."
    )]
    async fn describe_table(
        &self,
        Parameters(args): Parameters<DescribeTableArgs>,
    ) -> Result<CallToolResult, McpError> {
        let cube = match self.store.get("cube", &args.table).await {
            Ok(c) => c,
            Err(e) => return tool_err(e),
        };
        if let Some(want) = args.database.as_deref() {
            let got = cube.get("target").and_then(|v| v.as_str()).unwrap_or("");
            if got != want {
                return tool_err(AppError::Validation(format!(
                    "cube {} target {got} != requested database {want}",
                    args.table
                )));
            }
        }
        let role = self.effective_role(args.caller_role.as_deref());
        if let Err(e) = crate::compile::check_role_access(cube.get("role"), role) {
            return tool_err(e);
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
        let dimensions = project_dim_fact_members(cube.get("dimensions"), role, true);
        let facts = project_dim_fact_members(cube.get("facts"), role, false);
        let mut examples: Vec<Value> = Vec::new();
        for m in metrics
            .into_iter()
            .filter(|m| m.get("cube").and_then(|v| v.as_str()) == Some(cube_name))
            .filter(|m| crate::compile::check_role_access(m.get("role"), role).is_ok())
        {
            examples.push(project_metric_example(&m));
        }
        for s in segments
            .into_iter()
            .filter(|s| s.get("cube").and_then(|v| v.as_str()) == Some(cube_name))
            .filter(|s| crate::compile::check_role_access(s.get("role"), role).is_ok())
        {
            examples.push(project_segment_example(&s));
        }
        ok_json(json!({
            "database": cube.get("target").and_then(|v| v.as_str()).unwrap_or(""),
            "name": cube.get("name").and_then(|v| v.as_str()).unwrap_or(cube_name),
            "description": cube.get("description").and_then(|v| v.as_str()).unwrap_or(""),
            "dimensions": dimensions,
            "facts": facts,
            "examples": examples,
        }))
    }

    #[tool(
        description = "Search tables and columns by business keyword (dimensions and facts only). Use when you know a term but not the exact table or column name; prefer list_tables when browsing one database."
    )]
    async fn search_columns(
        &self,
        Parameters(args): Parameters<SearchColumnsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let mode = args.mode.as_deref().unwrap_or("hybrid");
        let limit = args.limit.map(|n| n as usize).unwrap_or(20);
        let role = self.effective_role(args.caller_role.as_deref());
        // Over-fetch then role-filter so `limit` is approximately respected after RBAC.
        let fetch = limit.saturating_mul(3).max(limit);
        let hits = match self
            .store
            .search_columns_for_target(&args.query, args.database.as_deref(), mode, fetch)
            .await
        {
            Ok(h) => h,
            Err(e) => return tool_err(e),
        };
        let docs = match self.store.list(None).await {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        let filtered: Vec<_> = hits
            .into_iter()
            .filter(|h| crate::search::hit_visible_for_role(h, &docs, role))
            .take(limit)
            .collect();
        ok_json(crate::search::hits_to_dw_json(&filtered))
    }

    #[tool(
        description = "Sample values for one dimension or fact to check codes or typical data before writing filters. Requires `database`, `table`, and `name` from list/search/describe. Code dimensions may return key/value pairs; not for metrics or segments."
    )]
    async fn sample_column(
        &self,
        Parameters(args): Parameters<SampleColumnArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = match self.targets.get(&args.database) {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let docs = match self.store.list(None).await {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        if let Err(e) = assert_cube_target(&docs, &args.table, &args.database) {
            return tool_err(e);
        }
        let limit = args
            .limit
            .map(|n| n as usize)
            .unwrap_or(self.cfg.sample_limit);
        let role = self.effective_role(args.caller_role.as_deref());
        let target = match crate::sample::resolve_sample_target_with_context(
            &docs,
            &args.table,
            &args.name,
            role,
            args.params.as_ref(),
        ) {
            Ok(t) => t,
            Err(e) => return tool_err(e),
        };
        let dialect = match crate::dialect::DialectProfile::for_engine(&db.info().engine) {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        let sql = match crate::sample::sample_sql(&target, limit, dialect) {
            Ok(s) => s,
            Err(e) => return tool_err(e),
        };
        let result = match tokio::time::timeout(self.cfg.timeout(), db.execute_select(&sql, limit)).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return tool_err(e),
            Err(_) => {
                return tool_err(AppError::Connection(anyhow::anyhow!(
                    "target query timed out after {}ms",
                    self.cfg.timeout_ms
                )));
            }
        };
        let encrypted = crate::mask::is_encrypted_member(&docs, &args.table, &args.name);
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
            "database": args.database,
            "table": args.table,
            "name": args.name,
            "as_code": target.from_as_code,
            "masked": encrypted,
            "values": values,
        }))
    }

    #[tool(
        description = "Run a read-only analytical SELECT and return columns, rows, row_count, and execution_time_ms. FROM must be a table from list_tables (or a subquery/CTE over those tables); select dimensions, facts, and ad-hoc aggregates/scalars. Copy metric/segment snippets from describe_table.examples into SQL—do not treat example names as columns. Do not write warehouse/physical SQL. Set dry_run=true only to inspect compiled SQL without fetching rows."
    )]
    async fn execute_sql(
        &self,
        Parameters(args): Parameters<ExecuteSqlArgs>,
    ) -> Result<CallToolResult, McpError> {
        let db = match self.targets.get(&args.database) {
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
            if let Err(e) = assert_cube_target(&docs, cube_name, &args.database) {
                return tool_err(e);
            }
        }
        let dialect = match crate::dialect::DialectProfile::for_engine(&db.info().engine) {
            Ok(d) => d,
            Err(e) => return tool_err(e),
        };
        let role = self.effective_role(args.caller_role.as_deref());
        let physical = match crate::pipeline::compile_semantic_sql_with_binds(
            &docs,
            &args.sql,
            role,
            args.params.as_ref(),
            dialect,
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
        let started = std::time::Instant::now();
        let result = match tokio::time::timeout(
            self.cfg.timeout(),
            db.execute_select(&physical, self.cfg.max_rows),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return tool_err(e),
            Err(_) => {
                return tool_err(AppError::Connection(anyhow::anyhow!(
                    "target query timed out after {}ms",
                    self.cfg.timeout_ms
                )));
            }
        };
        let execution_time_ms = started.elapsed().as_millis() as u64;
        let encrypted = crate::mask::encrypted_names_for_cubes(&docs, &cube_names);
        let rows = crate::mask::mask_result_rows(&result.columns, result.rows, &encrypted);
        let row_count = rows.len();
        let mut body = json!({
            "columns": result.columns,
            "rows": rows,
            "row_count": row_count,
            "execution_time_ms": execution_time_ms,
        });
        if row_count >= self.cfg.max_rows {
            body["truncated"] = json!(true);
        }
        ok_json(body)
    }
}

#[tool_handler(
    name = "sql-mcp",
    version = "0.1.0",
    instructions = "Query governed analytics tables: list_databases → list_tables/search_columns → describe_table/sample_column → execute_sql. Tables expose dimensions and facts as columns; metric/segment appear only as describe_table.examples (SQL snippets to copy)—not raw warehouse catalogs."
)]
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
            "Query governed analytics tables: list_databases → list_tables/search_columns → describe_table/sample_column → execute_sql. Tables expose dimensions and facts as columns; metric/segment appear only as describe_table.examples (SQL snippets to copy)—not raw warehouse catalogs."
                .to_string(),
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

fn role_visible(role_val: Option<&Value>, caller: Option<&str>) -> bool {
    crate::compile::check_role_access(role_val, caller).is_ok()
}

fn filter_as_expression(filter: Option<&Value>) -> Value {
    match filter {
        None | Some(Value::Null) => Value::Null,
        Some(Value::Object(o)) => o
            .get("expression")
            .cloned()
            .unwrap_or(Value::Null),
        Some(Value::String(s)) => json!(s),
        Some(other) => other.clone(),
    }
}

fn project_dim_fact_members(
    members: Option<&Value>,
    role: Option<&str>,
    include_as_code: bool,
) -> Vec<Value> {
    let Some(arr) = members.and_then(|v| v.as_array()) else {
        return vec![];
    };
    arr.iter()
        .filter(|m| role_visible(m.get("role"), role))
        .map(|m| {
            let mut out = json!({
                "name": m.get("name").cloned().unwrap_or(Value::Null),
                "description": m.get("description").cloned().unwrap_or(json!("")),
                "type": m.get("type").cloned().unwrap_or(Value::Null),
                "encryption": m.get("encryption").cloned().unwrap_or(json!(false)),
            });
            if include_as_code {
                if let Some(ac) = m.get("as_code") {
                    if !ac.is_null() {
                        out["as_code"] = ac.clone();
                    }
                }
            }
            out
        })
        .collect()
}

/// D26: metric/segment are example snippets, not SQL columns.
fn project_metric_example(m: &Value) -> Value {
    let mut out = json!({
        "kind": "metric",
        "name": m.get("name").cloned().unwrap_or(Value::Null),
        "description": m.get("description").cloned().unwrap_or(json!("")),
        "expression": m.get("expression").cloned().unwrap_or(Value::Null),
        "filter": filter_as_expression(m.get("filter")),
    });
    if let Some(a) = m.get("aggregation") {
        if !a.is_null() {
            out["aggregation"] = a.clone();
        }
    }
    if let Some(g) = m.get("group_by") {
        if !g.is_null() {
            out["group_by"] = g.clone();
        }
    }
    out
}

fn project_segment_example(s: &Value) -> Value {
    json!({
        "kind": "segment",
        "name": s.get("name").cloned().unwrap_or(Value::Null),
        "description": s.get("description").cloned().unwrap_or(json!("")),
        "filter": filter_as_expression(s.get("filter")),
    })
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
            "cube {cube_name} target {got} != requested database {target}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use schemars::schema_for;

    use crate::config::{Config, McpMode};
    use crate::target::{
        ColumnInfo, QueryResult, TableInfo, TargetDb, TargetInfo, TargetRegistry,
    };

    #[test]
    fn metadata_body_schema_is_object_not_boolean() {
        let create = schema_for!(MetadataBodyArgs);
        let update = schema_for!(MetadataUpdateArgs);
        let create_body = &create.as_value()["properties"]["body"];
        let update_body = &update.as_value()["properties"]["body"];
        assert_eq!(create_body["type"], "object", "{create_body}");
        assert_eq!(update_body["type"], "object", "{update_body}");
        assert!(!create_body.is_boolean(), "Cursor rejects boolean schemas");
        assert!(!update_body.is_boolean(), "Cursor rejects boolean schemas");
    }

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
    async fn list_databases_exposes_name_description_engine() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        let mut targets = TargetRegistry::empty();
        targets.insert_for_test(Arc::new(CountingBoomTarget {
            info: TargetInfo {
                id: "dw".into(),
                description: "Data Warehouse".into(),
                engine: "postgres".into(),
            },
            selects: AtomicUsize::new(0),
        }));
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let mcp = SqlMcp::new(store, targets, cfg);
        let body = tool_json(mcp.list_databases().await.unwrap());
        let items = body["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["name"], "dw");
        assert_eq!(items[0]["description"], "Data Warehouse");
        assert_eq!(items[0]["engine"], "postgres");
        assert!(items[0].get("id").is_none());
    }

    #[tokio::test]
    async fn list_tables_returns_cube_summaries() {
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
        let all = tool_json(
            mcp.list_tables(Parameters(ListTablesArgs { database: None }))
                .await
                .unwrap(),
        );
        let items = all["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["database"], "dw");
        assert_eq!(items[0]["name"], "sales");
        assert!(items[0]["description"].as_str().unwrap().len() > 0);
        assert!(items[0].get("dimensions").is_none());
        assert!(items[0].get("facts").is_none());
        assert!(items[0].get("kind").is_none());
        assert!(items[0].get("target").is_none());

        let filtered = tool_json(
            mcp.list_tables(Parameters(ListTablesArgs {
                database: Some("dw".into()),
            }))
            .await
            .unwrap(),
        );
        assert_eq!(filtered["items"].as_array().unwrap().len(), 1);
        let none = tool_json(
            mcp.list_tables(Parameters(ListTablesArgs {
                database: Some("nope".into()),
            }))
            .await
            .unwrap(),
        );
        assert!(none["items"].as_array().unwrap().is_empty());
    }

    struct FixedSelectTarget {
        info: TargetInfo,
        rows: Vec<Vec<Value>>,
    }

    #[async_trait]
    impl TargetDb for FixedSelectTarget {
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
            Ok(QueryResult {
                columns: vec!["v".into()],
                rows: self.rows.clone(),
            })
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

    #[tokio::test]
    async fn sample_column_dw_surface_hides_physical() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let fixed = Arc::new(FixedSelectTarget {
            info: TargetInfo {
                id: "dw".into(),
                description: "dw".into(),
                engine: "sqlite".into(),
            },
            rows: vec![
                vec![json!("KR")],
                vec![json!("US")],
            ],
        });
        let mut targets = TargetRegistry::empty();
        targets.insert_for_test(fixed);
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let mcp = SqlMcp::new(store, targets, cfg);
        let body = tool_json(
            mcp.sample_column(Parameters(SampleColumnArgs {
                database: "dw".into(),
                table: "sales".into(),
                name: "customer_id".into(),
                limit: Some(5),
                caller_role: Some("branch".into()),
                params: None,
            }))
            .await
            .unwrap(),
        );
        assert_eq!(body["database"], "dw");
        assert_eq!(body["table"], "sales");
        assert_eq!(body["name"], "customer_id");
        assert_eq!(body["as_code"], false);
        assert_eq!(body["masked"], false);
        assert_eq!(body["values"], json!(["KR", "US"]));
        assert!(body.get("resolved").is_none());
        assert!(body.get("sql").is_none());
        assert!(body.get("cube").is_none());
        assert!(body.get("member").is_none());
    }

    #[tokio::test]
    async fn describe_table_dw_surface_filters_role_and_hides_internals() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let mcp = SqlMcp::new(store, TargetRegistry::empty(), cfg);

        let branch = tool_json(
            mcp.describe_table(Parameters(DescribeTableArgs {
                table: "sales".into(),
                database: Some("dw".into()),
                caller_role: Some("branch".into()),
            }))
            .await
            .unwrap(),
        );
        assert_eq!(branch["database"], "dw");
        assert_eq!(branch["name"], "sales");
        assert!(branch.get("kind").is_none());
        assert!(branch.get("target").is_none());
        assert!(branch.get("view").is_none());
        assert!(branch.get("metrics").is_none());
        assert!(branch.get("segments").is_none());
        let examples = branch["examples"].as_array().unwrap();
        assert_eq!(examples.len(), 2);
        assert!(
            examples.iter().any(|e| e["kind"] == "metric" && e["name"] == "revenue"),
            "expected revenue metric example: {examples:?}"
        );
        let paid = examples
            .iter()
            .find(|e| e["kind"] == "segment" && e["name"] == "paid")
            .expect("paid segment example");
        assert_eq!(paid["filter"], "status_code = 'PAID'");
        let dim0 = &branch["dimensions"][0];
        assert!(dim0.get("role").is_none());
        assert!(dim0.get("column").is_none());
        let fact_names: Vec<&str> = branch["facts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f.get("name").and_then(|v| v.as_str()))
            .collect();
        assert!(
            !fact_names.contains(&"sale_amount"),
            "finance-only fact hidden for branch: {fact_names:?}"
        );
        let region = branch["dimensions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == "region_code")
            .expect("region_code");
        assert!(region.get("as_code").is_some());

        let finance = tool_json(
            mcp.describe_table(Parameters(DescribeTableArgs {
                table: "sales".into(),
                database: None,
                caller_role: Some("finance".into()),
            }))
            .await
            .unwrap(),
        );
        let finance_facts: Vec<&str> = finance["facts"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f.get("name").and_then(|v| v.as_str()))
            .collect();
        assert!(
            finance_facts.contains(&"sale_amount"),
            "finance sees sale_amount: {finance_facts:?}"
        );
        let ex0 = &finance["examples"][0];
        assert_eq!(ex0["kind"], "metric");
        assert!(ex0.get("cube").is_none());
        assert!(ex0.get("role").is_none());
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
    async fn execute_sql_response_has_counts_not_physical_sql() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let fixed = Arc::new(FixedSelectTarget {
            info: TargetInfo {
                id: "dw".into(),
                description: "dw".into(),
                engine: "sqlite".into(),
            },
            rows: vec![vec![json!(1)], vec![json!(2)]],
        });
        let mut targets = TargetRegistry::empty();
        targets.insert_for_test(fixed);
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let mcp = SqlMcp::new(store, targets, cfg);
        let mut params = std::collections::HashMap::new();
        params.insert("caller_region".into(), "KR".into());
        let body = tool_json(
            mcp.execute_sql(Parameters(ExecuteSqlArgs {
                database: "dw".into(),
                sql: "SELECT region_code, count(*) AS cnt FROM sales WHERE status_code = 'PAID' GROUP BY region_code".into(),
                caller_role: Some("branch".into()),
                params: Some(params),
                dry_run: None,
            }))
            .await
            .unwrap(),
        );
        assert_eq!(body["row_count"], 2);
        assert!(body["execution_time_ms"].as_u64().is_some());
        assert!(body.get("sql").is_none());
        assert!(body.get("dry_run").is_none());
        assert!(body.get("truncated").is_none());
        assert_eq!(body["rows"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn execute_sql_dry_run_skips_target_select() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let boom = Arc::new(CountingBoomTarget {
            info: TargetInfo {
                id: "dw".into(),
                description: "boom".into(),
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
                database: "dw".into(),
                sql: "SELECT region_code, count(*) AS cnt FROM sales WHERE status_code = 'PAID' GROUP BY region_code".into(),
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
        assert!(sql.to_ascii_lowercase().contains("count("));
        assert_eq!(boom.selects.load(Ordering::SeqCst), 0);

        let err = mcp
            .execute_sql(Parameters(ExecuteSqlArgs {
                database: "dw".into(),
                sql: "SELECT region_code, count(*) AS cnt FROM sales WHERE status_code = 'PAID' GROUP BY region_code".into(),
                caller_role: Some("branch".into()),
                params: None,
                dry_run: None,
            }))
            .await
            .unwrap();
        assert_eq!(err.is_error, Some(true));
        assert_eq!(boom.selects.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_mode_and_default_role_are_enforced() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        seed_examples(&store).await;
        let cfg = Config {
            listen: "127.0.0.1:0".into(),
            max_rows: 100,
            sample_limit: 10,
            timeout_ms: 1000,
            targets: vec![],
        };
        let boom = Arc::new(CountingBoomTarget {
            info: TargetInfo {
                id: "dw".into(),
                description: "boom".into(),
                engine: "sqlite".into(),
            },
            selects: AtomicUsize::new(0),
        });
        let mut targets = TargetRegistry::empty();
        targets.insert_for_test(boom);
        let mcp = SqlMcp::new(store, targets, cfg)
            .with_mode(McpMode::Query)
            .with_default_role("branch");

        // metadata_delete is forbidden in query mode
        let res = mcp
            .metadata_delete(Parameters(KindNameArgs {
                kind: "segment".into(),
                name: "paid".into(),
            }))
            .await
            .unwrap();
        assert_eq!(res.is_error, Some(true));

        // default_role ("branch") is applied when caller_role is None
        let mut params = std::collections::HashMap::new();
        params.insert("caller_region".into(), "KR".into());
        let dry = mcp
            .execute_sql(Parameters(ExecuteSqlArgs {
                database: "dw".into(),
                sql: "SELECT region_code, count(*) AS cnt FROM sales GROUP BY region_code".into(),
                caller_role: None,
                params: Some(params),
                dry_run: Some(true),
            }))
            .await
            .unwrap();
        let body = tool_json(dry);
        let sql = body["sql"].as_str().unwrap();
        assert!(
            sql.contains("'KR'"),
            "expected default_role 'branch' permission filter in compiled SQL: {sql}"
        );
    }
}
