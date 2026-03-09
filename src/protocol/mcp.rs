use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{
        Annotated, CallToolResult, Content, Implementation, ListResourcesResult,
        PaginatedRequestParams, RawResource, ReadResourceRequestParams, ReadResourceResult,
        ResourceContents, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};

use crate::domain::{BatchRetrievalRequest, IndexRunMode, IndexRunStatus, SymbolKind};
use crate::{ApplicationContext, TokenizorError};

const RUN_STATUS_URI_PREFIX: &str = "tokenizor://runs/";
const RUN_STATUS_URI_SUFFIX: &str = "/status";
const VALID_KIND_FILTERS: &str = "function, method, class, struct, enum, interface, module, constant, variable, type, trait, impl, other";

#[derive(Clone)]
pub struct TokenizorServer {
    tool_router: ToolRouter<Self>,
    application: ApplicationContext,
}

#[tool_router]
impl TokenizorServer {
    pub fn new(application: ApplicationContext) -> Self {
        Self {
            tool_router: Self::tool_router(),
            application,
        }
    }

    #[tool(
        description = "Report runtime health for the MCP server, SpacetimeDB control plane, and local byte-exact CAS."
    )]
    fn health(&self) -> Result<CallToolResult, McpError> {
        let report = self.application.health_report().map_err(to_mcp_error)?;
        let payload = serde_json::to_string(&report).map_err(|error| {
            McpError::internal_error(format!("failed to serialize health report: {error}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(payload)]))
    }

    #[tool(
        description = "Start an indexing run for a repository. Returns the run ID immediately without blocking on the full indexing pipeline. Parameters: repo_id (string, required), repo_root (string, required — absolute path to repository), mode (string, optional: full|incremental|repair|verify, defaults to full)."
    )]
    fn index_folder(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let repo_id = params
            .get("repo_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: repo_id", None))?
            .to_string();

        let repo_root = params
            .get("repo_root")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpError::invalid_params("missing required parameter: repo_root", None)
            })?;
        let repo_root = std::path::PathBuf::from(repo_root);

        let mode_str = params.get("mode").and_then(|v| v.as_str());
        let run_mode = match mode_str {
            Some("full") | None => IndexRunMode::Full,
            Some("incremental") => IndexRunMode::Incremental,
            Some("repair") => IndexRunMode::Repair,
            Some("verify") => IndexRunMode::Verify,
            Some(other) => {
                return Err(McpError::invalid_params(
                    format!(
                        "unknown indexing mode: `{other}`. Valid modes: full, incremental, repair, verify"
                    ),
                    None,
                ));
            }
        };

        let (run, _progress) = self
            .application
            .launch_indexing(&repo_id, run_mode, repo_root)
            .map_err(to_mcp_error)?;

        let payload = serde_json::to_string(&run).map_err(|error| {
            McpError::internal_error(format!("failed to serialize index run: {error}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(payload)]))
    }

    #[tool(
        description = "Inspect the status and health of an indexing run. Returns lifecycle state, health classification, progress (if active), file outcome summary, and action required (if intervention is needed). Parameters: run_id (string, required)."
    )]
    fn get_index_run(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let run_id = params
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: run_id", None))?;

        let report = self
            .application
            .run_manager()
            .inspect_run(run_id)
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string_pretty(&report).map_err(|e| {
            McpError::internal_error(format!("failed to serialize run status report: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "List indexing runs, optionally filtered by repository or status. Returns status and health for each run. Parameters: repo_id (string, optional), status (string, optional: queued|running|succeeded|failed|cancelled|interrupted|aborted)."
    )]
    fn list_index_runs(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let repo_id = params.get("repo_id").and_then(|v| v.as_str());

        let status_filter = if let Some(status_str) = params.get("status").and_then(|v| v.as_str())
        {
            let parsed = match status_str {
                "queued" => IndexRunStatus::Queued,
                "running" => IndexRunStatus::Running,
                "succeeded" => IndexRunStatus::Succeeded,
                "failed" => IndexRunStatus::Failed,
                "cancelled" => IndexRunStatus::Cancelled,
                "interrupted" => IndexRunStatus::Interrupted,
                "aborted" => IndexRunStatus::Aborted,
                other => {
                    return Err(McpError::invalid_params(
                        format!(
                            "unknown status: `{other}`. Valid statuses: queued, running, succeeded, failed, cancelled, interrupted, aborted"
                        ),
                        None,
                    ));
                }
            };
            Some(parsed)
        } else {
            None
        };

        let reports = self
            .application
            .run_manager()
            .list_runs_with_health(repo_id, status_filter.as_ref())
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string_pretty(&reports).map_err(|e| {
            McpError::internal_error(format!("failed to serialize run status reports: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Cancel an active indexing run. Returns the updated run status report. If the run is already terminal, returns the current status without modification. Parameters: run_id (string, required)."
    )]
    fn cancel_index_run(
        &self,
        params: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, McpError> {
        let run_id = params
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: run_id", None))?;

        let report = self
            .application
            .run_manager()
            .cancel_run(run_id)
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string_pretty(&report).map_err(|e| {
            McpError::internal_error(format!("failed to serialize run status report: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Create a checkpoint for an active indexing run. Persists current progress so interrupted work can later resume. Returns the checkpoint details. Fails if the run is not active or has no committed work yet. Parameters: run_id (string, required)."
    )]
    fn checkpoint_now(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let run_id = params
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: run_id", None))?;

        let checkpoint = self
            .application
            .run_manager()
            .checkpoint_run(run_id)
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string(&checkpoint).map_err(|e| {
            McpError::internal_error(format!("failed to serialize checkpoint: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Attempt to resume an interrupted indexing run from its last durable checkpoint. Returns a structured outcome indicating whether the run resumed or why resume was rejected, including the next safe action. Non-blocking: on success it returns immediately with the managed run reference. Parameters: run_id (string, required), repo_root (string, required — absolute path to repository)."
    )]
    fn resume_index_run(
        &self,
        params: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, McpError> {
        let run_id = params
            .get("run_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: run_id", None))?;

        let repo_root = params
            .get("repo_root")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpError::invalid_params("missing required parameter: repo_root", None)
            })?;
        let repo_root = std::path::PathBuf::from(repo_root);

        let outcome = self
            .application
            .resume_index_run(run_id, repo_root)
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string_pretty(&outcome).map_err(|e| {
            McpError::internal_error(format!("failed to serialize resume run outcome: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Re-index a previously indexed repository. Creates a new indexing run with mode 'reindex', linking to the prior completed run for traceability. Prior state remains inspectable. Behaves idempotently on replay. Parameters: repo_id (string, required), repo_root (string, required — absolute path to repository), workspace_id (string, optional), reason (string, optional description of why re-indexing)."
    )]
    fn reindex_repository(
        &self,
        params: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, McpError> {
        let repo_id = params
            .get("repo_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: repo_id", None))?;

        let repo_root = params
            .get("repo_root")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                McpError::invalid_params("missing required parameter: repo_root", None)
            })?;
        let repo_root = std::path::PathBuf::from(repo_root);

        let workspace_id = params.get("workspace_id").and_then(|v| v.as_str());
        let reason = params.get("reason").and_then(|v| v.as_str());

        let run = self
            .application
            .reindex_repository(repo_id, workspace_id, reason, repo_root)
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string_pretty(&run).map_err(|e| {
            McpError::internal_error(format!("failed to serialize reindex run: {e}"), None)
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Invalidate indexed state for a repository so it is no longer treated as trusted. Use when indexed state should not be served to retrieval flows. Returns the invalidation result with guidance for recovery (re-index or repair). Parameters: repo_id (string, required), workspace_id (string, optional), reason (string, optional description of why invalidation is needed)."
    )]
    fn invalidate_indexed_state(
        &self,
        params: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, McpError> {
        let repo_id = params
            .get("repo_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| McpError::invalid_params("missing required parameter: repo_id", None))?;

        let workspace_id = params.get("workspace_id").and_then(|v| v.as_str());
        let reason = params.get("reason").and_then(|v| v.as_str());

        let result = self
            .application
            .invalidate_repository(repo_id, workspace_id, reason)
            .map_err(to_mcp_error)?;

        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(
                format!("failed to serialize invalidation result: {e}"),
                None,
            )
        })?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Search indexed repository content by text. Returns matching code locations with line context, scoped to the specified repository. Results include provenance metadata (run_id, committed_at_unix_ms) for staleness assessment. Parameters: repo_id (string, required), query (string, required — non-empty search text)."
    )]
    fn search_text(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let p = parse_search_text_params(&params)?;
        let result = self
            .application
            .search_text(&p.repo_id, &p.query)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("failed to serialize search results: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Search indexed repository symbols by name. Returns matching symbol metadata (name, kind, file path, line range, depth) with coverage transparency. Uses case-insensitive substring matching. Parameters: repo_id (string, required), query (string, required — non-empty search text), kind_filter (string, optional: function|method|class|struct|enum|interface|module|constant|variable|type|trait|impl|other)."
    )]
    fn search_symbols(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let p = parse_search_symbols_params(&params)?;
        let result = self
            .application
            .search_symbols(&p.repo_id, &p.query, p.kind_filter)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(
                format!("failed to serialize symbol search results: {e}"),
                None,
            )
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Retrieve the structural outline (symbol tree) for a specific file in an indexed repository. Returns symbol metadata including name, kind, line ranges, depth, and document order. Distinguishes files with no symbols from files with unsupported languages. Parameters: repo_id (string, required), relative_path (string, required — file path relative to repository root)."
    )]
    fn get_file_outline(
        &self,
        params: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, McpError> {
        let p = parse_file_outline_params(&params)?;
        let result = self
            .application
            .get_file_outline(&p.repo_id, &p.relative_path)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("failed to serialize file outline: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Retrieve the structural overview of all files in an indexed repository. Returns file-level metadata (path, language, byte size, symbol count, status) with coverage statistics distinguishing files with symbols, without symbols, quarantined, and failed. Parameters: repo_id (string, required)."
    )]
    fn get_repo_outline(
        &self,
        params: rmcp::model::JsonObject,
    ) -> Result<CallToolResult, McpError> {
        let p = parse_repo_outline_params(&params)?;
        let result = self
            .application
            .get_repo_outline(&p.repo_id)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("failed to serialize repo outline: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Retrieve verified source code for a specific symbol from an indexed repository. Returns the exact source text with byte-exact verification against stored content. Verification ensures blob integrity (content hash match), span validity, and raw source fidelity. Parameters: repo_id (string, required), relative_path (string, required — file path relative to repository root), symbol_name (string, required — exact symbol name to retrieve), kind_filter (string, optional: function|method|class|struct|enum|interface|module|constant|variable|type|trait|impl|other)."
    )]
    fn get_symbol(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let p = parse_get_symbol_params(&params)?;
        let result = self
            .application
            .get_symbol(&p.repo_id, &p.relative_path, &p.symbol_name, p.kind_filter)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("failed to serialize get_symbol result: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(
        description = "Retrieve verified source code for multiple symbols or raw code slices from an indexed repository in a single request. Each item is verified independently — one failure does not affect others. Returns per-item outcomes with trust and provenance metadata. Parameters: repo_id (string, required), targets (array, preferred — ordered items with request_type=symbol or request_type=code_slice), or legacy symbols (array of symbol requests). Maximum 50 items per request."
    )]
    fn get_symbols(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
        let p = parse_get_symbols_params(&params)?;
        let result = self
            .application
            .get_symbols(&p.repo_id, &p.requests)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&result).map_err(|e| {
            McpError::internal_error(format!("failed to serialize get_symbols result: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(json)]))
    }
}

#[tool_handler]
impl ServerHandler for TokenizorServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::from_build_env())
        .with_instructions(
            "tokenizor_agentic_mcp is a Rust-native MCP server for code indexing and trusted retrieval. Retrieval tools (search_text, search_symbols, get_file_outline, get_repo_outline, get_symbol, get_symbols) provide verified code discovery with explicit trust and provenance metadata. get_symbol performs byte-exact verification against stored content before serving trusted source. Use get_symbols to retrieve multiple symbols or raw code slices in a single request for efficiency. Each item is verified independently — mixed outcomes are reported explicitly, including missing items. Prefer the targets parameter: an ordered array of objects with request_type=symbol or request_type=code_slice. Symbol targets use relative_path (required), symbol_name (required), and kind_filter (optional). Code-slice targets use relative_path (required) and byte_range ([start, end], required). The legacy symbols parameter remains accepted for symbol-only batches. Maximum 50 items per request. Request-level gating applies to the entire batch. Blocked or quarantined results include a next_action field indicating the recommended resolution (reindex, repair, wait, resolve_context). Repositories in quarantined state reject all retrieval requests with actionable guidance. Indexing tools manage durable run lifecycle. All retrieval tools require a repo_id parameter identifying the target repository.",
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let run_ids = self.application.run_manager().list_recent_run_ids(10);
        let resources = run_ids
            .iter()
            .map(|id| {
                Annotated::new(
                    RawResource {
                        uri: format!("{}{}{}", RUN_STATUS_URI_PREFIX, id, RUN_STATUS_URI_SUFFIX),
                        name: format!("Run {} Status", id),
                        title: None,
                        description: Some(format!("Status and health for indexing run {}", id)),
                        mime_type: Some("application/json".to_string()),
                        size: None,
                        icons: None,
                        meta: None,
                    },
                    None,
                )
            })
            .collect();
        Ok(ListResourcesResult::with_all_items(resources))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let run_id = parse_run_id_from_uri(&request.uri)?;
        let report = self
            .application
            .run_manager()
            .inspect_run(&run_id)
            .map_err(to_mcp_error)?;
        let json = serde_json::to_string_pretty(&report).map_err(|e| {
            McpError::internal_error(format!("failed to serialize run status report: {e}"), None)
        })?;
        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            json,
            request.uri,
        )]))
    }
}

fn to_mcp_error(error: TokenizorError) -> McpError {
    match error {
        TokenizorError::Config(message) | TokenizorError::InvalidArgument(message) => {
            McpError::invalid_params(message, None)
        }
        TokenizorError::ConflictingReplay(message) => McpError::invalid_params(
            format!(
                "conflicting replay: {message} — retry with identical inputs or use a new idempotency key"
            ),
            None,
        ),
        TokenizorError::InvalidOperation(message) => {
            McpError::invalid_params(format!("invalid operation: {message}"), None)
        }
        TokenizorError::NotFound(message) => McpError::invalid_params(message, None),
        TokenizorError::Integrity(message) => {
            McpError::internal_error(format!("integrity violation: {message}"), None)
        }
        TokenizorError::Storage(message) => McpError::internal_error(message, None),
        TokenizorError::ControlPlane(message) => McpError::internal_error(message, None),
        TokenizorError::Io { path, source } => {
            McpError::internal_error(format!("i/o error at `{}`: {source}", path.display()), None)
        }
        TokenizorError::Serialization(message) => McpError::internal_error(message, None),
        TokenizorError::RequestGated { gate_error } => {
            McpError::invalid_params(format!("request gated: {gate_error}"), None)
        }
    }
}

fn parse_run_id_from_uri(uri: &str) -> Result<String, McpError> {
    let stripped = uri
        .strip_prefix(RUN_STATUS_URI_PREFIX)
        .and_then(|s| s.strip_suffix(RUN_STATUS_URI_SUFFIX))
        .ok_or_else(|| {
            McpError::invalid_params(
                format!(
                    "invalid resource URI: expected {}{{run_id}}{}",
                    RUN_STATUS_URI_PREFIX, RUN_STATUS_URI_SUFFIX
                ),
                None,
            )
        })?;
    if stripped.is_empty() {
        return Err(McpError::invalid_params(
            "invalid resource URI: run_id is empty",
            None,
        ));
    }
    Ok(stripped.to_string())
}

#[derive(Debug)]
struct SearchTextParams {
    repo_id: String,
    query: String,
}

fn required_non_empty_string_param(
    params: &rmcp::model::JsonObject,
    key: &str,
) -> Result<String, McpError> {
    let value = params.get(key).ok_or_else(|| {
        McpError::invalid_params(format!("missing required parameter: {key}"), None)
    })?;
    let value = value.as_str().ok_or_else(|| {
        McpError::invalid_params(
            format!("invalid parameter `{key}`: expected non-empty string"),
            None,
        )
    })?;
    if value.trim().is_empty() {
        return Err(McpError::invalid_params(
            format!("invalid parameter `{key}`: expected non-empty string"),
            None,
        ));
    }
    Ok(value.to_string())
}

fn invalid_kind_filter_type_error() -> McpError {
    McpError::invalid_params(
        format!(
            "invalid parameter `kind_filter`: expected string. Valid kinds: {VALID_KIND_FILTERS}"
        ),
        None,
    )
}

fn unknown_kind_filter_error(value: &str) -> McpError {
    McpError::invalid_params(
        format!("unknown kind_filter: `{value}`. Valid kinds: {VALID_KIND_FILTERS}"),
        None,
    )
}

fn parse_search_text_params(
    params: &rmcp::model::JsonObject,
) -> Result<SearchTextParams, McpError> {
    let repo_id = required_non_empty_string_param(params, "repo_id")?;
    let query = required_non_empty_string_param(params, "query")?;
    Ok(SearchTextParams { repo_id, query })
}

#[derive(Debug)]
struct SearchSymbolsParams {
    repo_id: String,
    query: String,
    kind_filter: Option<SymbolKind>,
}

fn parse_search_symbols_params(
    params: &rmcp::model::JsonObject,
) -> Result<SearchSymbolsParams, McpError> {
    let repo_id = required_non_empty_string_param(params, "repo_id")?;
    let query = required_non_empty_string_param(params, "query")?;
    let kind_filter = parse_kind_filter(params)?;
    Ok(SearchSymbolsParams {
        repo_id,
        query,
        kind_filter,
    })
}

fn parse_kind_filter(params: &rmcp::model::JsonObject) -> Result<Option<SymbolKind>, McpError> {
    parse_kind_filter_value(params.get("kind_filter"))
}

#[derive(Debug)]
struct FileOutlineParams {
    repo_id: String,
    relative_path: String,
}

fn parse_file_outline_params(
    params: &rmcp::model::JsonObject,
) -> Result<FileOutlineParams, McpError> {
    let repo_id = required_non_empty_string_param(params, "repo_id")?;
    let relative_path = required_non_empty_string_param(params, "relative_path")?;
    Ok(FileOutlineParams {
        repo_id,
        relative_path,
    })
}

#[derive(Debug)]
struct RepoOutlineParams {
    repo_id: String,
}

fn parse_repo_outline_params(
    params: &rmcp::model::JsonObject,
) -> Result<RepoOutlineParams, McpError> {
    let repo_id = required_non_empty_string_param(params, "repo_id")?;
    Ok(RepoOutlineParams { repo_id })
}

#[derive(Debug)]
struct GetSymbolParams {
    repo_id: String,
    relative_path: String,
    symbol_name: String,
    kind_filter: Option<SymbolKind>,
}

fn parse_get_symbol_params(params: &rmcp::model::JsonObject) -> Result<GetSymbolParams, McpError> {
    let repo_id = required_non_empty_string_param(params, "repo_id")?;
    let relative_path = required_non_empty_string_param(params, "relative_path")?;
    let symbol_name = required_non_empty_string_param(params, "symbol_name")?;
    let kind_filter = parse_kind_filter(params)?;
    Ok(GetSymbolParams {
        repo_id,
        relative_path,
        symbol_name,
        kind_filter,
    })
}

const MAX_BATCH_SIZE: usize = 50;

#[derive(Debug)]
struct GetSymbolsParams {
    repo_id: String,
    requests: Vec<BatchRetrievalRequest>,
}

fn parse_get_symbols_params(
    params: &rmcp::model::JsonObject,
) -> Result<GetSymbolsParams, McpError> {
    let repo_id = required_non_empty_string_param(params, "repo_id")?;

    let requests = if let Some(targets_value) = params.get("targets") {
        let targets_array = targets_value.as_array().ok_or_else(|| {
            McpError::invalid_params(
                "invalid parameter `targets`: expected array of retrieval request objects",
                None,
            )
        })?;
        parse_batch_targets(targets_array)?
    } else if let Some(symbols_value) = params.get("symbols") {
        let symbols_array = symbols_value.as_array().ok_or_else(|| {
            McpError::invalid_params(
                "invalid parameter `symbols`: expected array of symbol request objects",
                None,
            )
        })?;
        parse_legacy_symbol_targets(symbols_array)?
    } else {
        return Err(McpError::invalid_params(
            "missing required parameter: targets or symbols",
            None,
        ));
    };

    Ok(GetSymbolsParams { repo_id, requests })
}

fn parse_kind_filter_value(
    kind_value: Option<&serde_json::Value>,
) -> Result<Option<SymbolKind>, McpError> {
    let Some(value) = kind_value else {
        return Ok(None);
    };
    let kind_str = value.as_str().ok_or_else(invalid_kind_filter_type_error)?;
    let kind = match kind_str {
        "function" => SymbolKind::Function,
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        "struct" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "interface" => SymbolKind::Interface,
        "module" => SymbolKind::Module,
        "constant" => SymbolKind::Constant,
        "variable" => SymbolKind::Variable,
        "type" => SymbolKind::Type,
        "trait" => SymbolKind::Trait,
        "impl" => SymbolKind::Impl,
        "other" => SymbolKind::Other,
        other => return Err(unknown_kind_filter_error(other)),
    };
    Ok(Some(kind))
}

fn parse_batch_targets(
    targets_array: &[serde_json::Value],
) -> Result<Vec<BatchRetrievalRequest>, McpError> {
    validate_batch_size(targets_array.len())?;

    let mut requests = Vec::with_capacity(targets_array.len());
    for (i, item) in targets_array.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| {
            McpError::invalid_params(
                format!("targets[{i}]: expected object with request_type"),
                None,
            )
        })?;
        let request_type = obj
            .get("request_type")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "targets[{i}]: missing or invalid `request_type` (expected `symbol` or `code_slice`)"
                    ),
                    None,
                )
            })?;
        let relative_path = required_batch_string_field(obj, "targets", i, "relative_path")?;

        match request_type {
            "symbol" => {
                let symbol_name = required_batch_string_field(obj, "targets", i, "symbol_name")?;
                let kind_filter = parse_kind_filter_value(obj.get("kind_filter")).map_err(|err| {
                    McpError::invalid_params(
                        format!(
                            "targets[{i}]: invalid `kind_filter`: {}. Valid kinds: {VALID_KIND_FILTERS}",
                            err.message
                        ),
                        None,
                    )
                })?;
                requests.push(BatchRetrievalRequest::Symbol {
                    relative_path,
                    symbol_name,
                    kind_filter,
                });
            }
            "code_slice" => {
                let byte_range = required_byte_range_field(obj, "targets", i)?;
                requests.push(BatchRetrievalRequest::CodeSlice {
                    relative_path,
                    byte_range,
                });
            }
            other => {
                return Err(McpError::invalid_params(
                    format!(
                        "targets[{i}]: unknown `request_type` `{other}` (expected `symbol` or `code_slice`)"
                    ),
                    None,
                ));
            }
        }
    }

    Ok(requests)
}

fn parse_legacy_symbol_targets(
    symbols_array: &[serde_json::Value],
) -> Result<Vec<BatchRetrievalRequest>, McpError> {
    validate_batch_size(symbols_array.len())?;

    let mut requests = Vec::with_capacity(symbols_array.len());
    for (i, item) in symbols_array.iter().enumerate() {
        let obj = item.as_object().ok_or_else(|| {
            McpError::invalid_params(
                format!("symbols[{i}]: expected object with relative_path and symbol_name"),
                None,
            )
        })?;

        let relative_path = required_batch_string_field(obj, "symbols", i, "relative_path")?;
        let symbol_name = required_batch_string_field(obj, "symbols", i, "symbol_name")?;
        let kind_filter = parse_kind_filter_value(obj.get("kind_filter")).map_err(|err| {
            McpError::invalid_params(
                format!(
                    "symbols[{i}]: invalid `kind_filter`: {}. Valid kinds: {VALID_KIND_FILTERS}",
                    err.message
                ),
                None,
            )
        })?;

        requests.push(BatchRetrievalRequest::Symbol {
            relative_path,
            symbol_name,
            kind_filter,
        });
    }

    Ok(requests)
}

fn required_batch_string_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    collection: &str,
    index: usize,
    field: &str,
) -> Result<String, McpError> {
    obj.get(field)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .ok_or_else(|| {
            McpError::invalid_params(
                format!("{collection}[{index}]: missing or empty `{field}`"),
                None,
            )
        })
}

fn required_byte_range_field(
    obj: &serde_json::Map<String, serde_json::Value>,
    collection: &str,
    index: usize,
) -> Result<(u32, u32), McpError> {
    let byte_range = obj.get("byte_range").ok_or_else(|| {
        McpError::invalid_params(
            format!("{collection}[{index}]: missing required `byte_range`"),
            None,
        )
    })?;
    let range_array = byte_range.as_array().ok_or_else(|| {
        McpError::invalid_params(
            format!("{collection}[{index}]: invalid `byte_range`: expected [start, end]"),
            None,
        )
    })?;
    if range_array.len() != 2 {
        return Err(McpError::invalid_params(
            format!("{collection}[{index}]: invalid `byte_range`: expected exactly 2 integers"),
            None,
        ));
    }

    let start = range_array[0].as_u64().ok_or_else(|| {
        McpError::invalid_params(
            format!("{collection}[{index}]: invalid `byte_range[0]`: expected unsigned integer"),
            None,
        )
    })?;
    let end = range_array[1].as_u64().ok_or_else(|| {
        McpError::invalid_params(
            format!("{collection}[{index}]: invalid `byte_range[1]`: expected unsigned integer"),
            None,
        )
    })?;

    let start = u32::try_from(start).map_err(|_| {
        McpError::invalid_params(
            format!("{collection}[{index}]: invalid `byte_range[0]`: exceeds u32"),
            None,
        )
    })?;
    let end = u32::try_from(end).map_err(|_| {
        McpError::invalid_params(
            format!("{collection}[{index}]: invalid `byte_range[1]`: exceeds u32"),
            None,
        )
    })?;

    Ok((start, end))
}

fn validate_batch_size(size: usize) -> Result<(), McpError> {
    if size > MAX_BATCH_SIZE {
        return Err(McpError::invalid_params(
            format!("batch size {size} exceeds maximum of {MAX_BATCH_SIZE} items per request"),
            None,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_run_id_from_uri_valid_uuid() {
        let uri = "tokenizor://runs/550e8400-e29b-41d4-a716-446655440000/status";
        let result = parse_run_id_from_uri(uri).unwrap();
        assert_eq!(result, "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn test_parse_run_id_from_uri_simple_id() {
        let uri = "tokenizor://runs/run-123/status";
        let result = parse_run_id_from_uri(uri).unwrap();
        assert_eq!(result, "run-123");
    }

    #[test]
    fn test_parse_run_id_from_uri_missing_prefix() {
        let uri = "invalid://runs/abc/status";
        let result = parse_run_id_from_uri(uri);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_run_id_from_uri_missing_suffix() {
        let uri = "tokenizor://runs/abc";
        let result = parse_run_id_from_uri(uri);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_run_id_from_uri_empty_run_id() {
        let uri = "tokenizor://runs//status";
        let result = parse_run_id_from_uri(uri);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_run_id_from_uri_completely_invalid() {
        let result = parse_run_id_from_uri("garbage");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_run_id_from_uri_empty_string() {
        let result = parse_run_id_from_uri("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_run_id_from_uri_only_prefix() {
        let result = parse_run_id_from_uri("tokenizor://runs/");
        assert!(result.is_err());
    }

    #[test]
    fn test_run_status_uri_round_trip() {
        let run_id = "test-run-42";
        let uri = format!(
            "{}{}{}",
            RUN_STATUS_URI_PREFIX, run_id, RUN_STATUS_URI_SUFFIX
        );
        let parsed = parse_run_id_from_uri(&uri).unwrap();
        assert_eq!(parsed, run_id);
    }

    // --- search_text parameter validation (Task 3.1) ---

    fn json_object(pairs: &[(&str, &str)]) -> rmcp::model::JsonObject {
        let mut map = serde_json::Map::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
        map
    }

    fn json_object_values(pairs: &[(&str, serde_json::Value)]) -> rmcp::model::JsonObject {
        let mut map = serde_json::Map::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v.clone());
        }
        map
    }

    #[test]
    fn test_search_text_tool_rejects_missing_repo_id() {
        let params = json_object(&[("query", "hello")]);
        let err = parse_search_text_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: repo_id")
        );
    }

    #[test]
    fn test_search_text_tool_rejects_missing_query() {
        let params = json_object(&[("repo_id", "repo-1")]);
        let err = parse_search_text_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: query")
        );
    }

    #[test]
    fn test_search_text_tool_rejects_empty_repo_id() {
        let params = json_object(&[("repo_id", ""), ("query", "hello")]);
        let err = parse_search_text_params(&params).unwrap_err();
        assert!(err.to_string().contains("invalid parameter `repo_id`"));
    }

    #[test]
    fn test_search_text_tool_rejects_empty_query() {
        let params = json_object(&[("repo_id", "repo-1"), ("query", "")]);
        let err = parse_search_text_params(&params).unwrap_err();
        assert!(err.to_string().contains("invalid parameter `query`"));
    }

    // --- search_symbols parameter validation (Task 3.1) ---

    #[test]
    fn test_search_symbols_tool_rejects_missing_repo_id() {
        let params = json_object(&[("query", "hello")]);
        let err = parse_search_symbols_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: repo_id")
        );
    }

    #[test]
    fn test_search_symbols_tool_rejects_missing_query() {
        let params = json_object(&[("repo_id", "repo-1")]);
        let err = parse_search_symbols_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: query")
        );
    }

    #[test]
    fn test_search_symbols_tool_rejects_invalid_kind_filter() {
        let params = json_object(&[
            ("repo_id", "repo-1"),
            ("query", "test"),
            ("kind_filter", "bogus"),
        ]);
        let err = parse_search_symbols_params(&params).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown kind_filter: `bogus`"));
        assert!(msg.contains("function"));
        assert!(msg.contains("method"));
        assert!(msg.contains("class"));
        assert!(msg.contains("struct"));
        assert!(msg.contains("enum"));
        assert!(msg.contains("interface"));
        assert!(msg.contains("module"));
        assert!(msg.contains("constant"));
        assert!(msg.contains("variable"));
        assert!(msg.contains("type"));
        assert!(msg.contains("trait"));
        assert!(msg.contains("impl"));
        assert!(msg.contains("other"));
    }

    #[test]
    fn test_search_symbols_tool_accepts_valid_kind_filter() {
        let params = json_object(&[
            ("repo_id", "repo-1"),
            ("query", "test"),
            ("kind_filter", "function"),
        ]);
        let result = parse_search_symbols_params(&params).unwrap();
        assert_eq!(result.kind_filter, Some(SymbolKind::Function));
    }

    #[test]
    fn test_search_symbols_tool_accepts_missing_kind_filter() {
        let params = json_object(&[("repo_id", "repo-1"), ("query", "test")]);
        let result = parse_search_symbols_params(&params).unwrap();
        assert_eq!(result.kind_filter, None);
    }

    #[test]
    fn test_search_symbols_tool_rejects_non_string_kind_filter() {
        let params = json_object_values(&[
            ("repo_id", serde_json::Value::String("repo-1".to_string())),
            ("query", serde_json::Value::String("test".to_string())),
            ("kind_filter", serde_json::json!(123)),
        ]);
        let err = parse_search_symbols_params(&params).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid parameter `kind_filter`"));
        assert!(msg.contains("Valid kinds"));
    }

    // --- get_file_outline parameter validation (Task 3.1) ---

    #[test]
    fn test_get_file_outline_tool_rejects_missing_repo_id() {
        let params = json_object(&[("relative_path", "src/main.rs")]);
        let err = parse_file_outline_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: repo_id")
        );
    }

    #[test]
    fn test_get_file_outline_tool_rejects_missing_relative_path() {
        let params = json_object(&[("repo_id", "repo-1")]);
        let err = parse_file_outline_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: relative_path")
        );
    }

    #[test]
    fn test_get_file_outline_tool_rejects_empty_relative_path() {
        let params = json_object(&[("repo_id", "repo-1"), ("relative_path", "")]);
        let err = parse_file_outline_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid parameter `relative_path`")
        );
    }

    // --- get_repo_outline parameter validation (Task 3.1) ---

    #[test]
    fn test_get_repo_outline_tool_rejects_missing_repo_id() {
        let params = json_object(&[]);
        let err = parse_repo_outline_params(&params).unwrap_err();
        assert!(
            err.to_string()
                .contains("missing required parameter: repo_id")
        );
    }

    #[test]
    fn test_get_repo_outline_tool_rejects_empty_repo_id() {
        let params = json_object(&[("repo_id", "")]);
        let err = parse_repo_outline_params(&params).unwrap_err();
        assert!(err.to_string().contains("invalid parameter `repo_id`"));
    }

    // --- kind_filter parsing tests (Task 3.2) ---

    #[test]
    fn test_parse_kind_filter_all_13_variants() {
        let cases = [
            ("function", SymbolKind::Function),
            ("method", SymbolKind::Method),
            ("class", SymbolKind::Class),
            ("struct", SymbolKind::Struct),
            ("enum", SymbolKind::Enum),
            ("interface", SymbolKind::Interface),
            ("module", SymbolKind::Module),
            ("constant", SymbolKind::Constant),
            ("variable", SymbolKind::Variable),
            ("type", SymbolKind::Type),
            ("trait", SymbolKind::Trait),
            ("impl", SymbolKind::Impl),
            ("other", SymbolKind::Other),
        ];
        assert_eq!(cases.len(), 13);
        for (input, expected) in cases {
            let params = json_object(&[("kind_filter", input)]);
            let result = parse_kind_filter(&params).unwrap();
            assert_eq!(result, Some(expected), "failed for kind_filter: {input}");
        }
    }

    #[test]
    fn test_parse_kind_filter_rejects_unknown_value() {
        let params = json_object(&[("kind_filter", "unknown_kind")]);
        let err = parse_kind_filter(&params).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown kind_filter: `unknown_kind`"));
        assert!(msg.contains("function"));
        assert!(msg.contains("other"));
    }

    #[test]
    fn test_parse_kind_filter_rejects_non_string_value() {
        let params = json_object_values(&[("kind_filter", serde_json::json!(true))]);
        let err = parse_kind_filter(&params).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("invalid parameter `kind_filter`"));
        assert!(msg.contains("function"));
        assert!(msg.contains("other"));
    }

    #[test]
    fn test_parse_kind_filter_none_for_absent() {
        let params = json_object(&[]);
        let result = parse_kind_filter(&params).unwrap();
        assert_eq!(result, None);
    }

    // --- get_symbol parameter validation (Story 3.5) ---

    #[test]
    fn test_get_symbol_tool_rejects_missing_repo_id() {
        let params = json_object(&[("relative_path", "src/main.rs"), ("symbol_name", "main")]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert!(err.message.contains("missing required parameter: repo_id"));
    }

    #[test]
    fn test_get_symbol_tool_rejects_missing_relative_path() {
        let params = json_object(&[("repo_id", "repo-1"), ("symbol_name", "main")]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert!(
            err.message
                .contains("missing required parameter: relative_path")
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_missing_symbol_name() {
        let params = json_object(&[("repo_id", "repo-1"), ("relative_path", "src/main.rs")]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert!(
            err.message
                .contains("missing required parameter: symbol_name")
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_empty_repo_id() {
        let params = json_object(&[
            ("repo_id", ""),
            ("relative_path", "src/main.rs"),
            ("symbol_name", "main"),
        ]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert_eq!(
            err.message,
            "invalid parameter `repo_id`: expected non-empty string"
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_non_string_repo_id() {
        let params = json_object_values(&[
            ("repo_id", serde_json::json!(123)),
            (
                "relative_path",
                serde_json::Value::String("src/main.rs".to_string()),
            ),
            ("symbol_name", serde_json::Value::String("main".to_string())),
        ]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert_eq!(
            err.message,
            "invalid parameter `repo_id`: expected non-empty string"
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_empty_relative_path() {
        let params = json_object(&[
            ("repo_id", "repo-1"),
            ("relative_path", ""),
            ("symbol_name", "main"),
        ]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert_eq!(
            err.message,
            "invalid parameter `relative_path`: expected non-empty string"
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_non_string_relative_path() {
        let params = json_object_values(&[
            ("repo_id", serde_json::Value::String("repo-1".to_string())),
            ("relative_path", serde_json::json!(123)),
            ("symbol_name", serde_json::Value::String("main".to_string())),
        ]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert_eq!(
            err.message,
            "invalid parameter `relative_path`: expected non-empty string"
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_empty_symbol_name() {
        let params = json_object(&[
            ("repo_id", "repo-1"),
            ("relative_path", "src/main.rs"),
            ("symbol_name", ""),
        ]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert_eq!(
            err.message,
            "invalid parameter `symbol_name`: expected non-empty string"
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_non_string_symbol_name() {
        let params = json_object_values(&[
            ("repo_id", serde_json::Value::String("repo-1".to_string())),
            (
                "relative_path",
                serde_json::Value::String("src/main.rs".to_string()),
            ),
            ("symbol_name", serde_json::json!(123)),
        ]);
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert_eq!(
            err.message,
            "invalid parameter `symbol_name`: expected non-empty string"
        );
    }

    #[test]
    fn test_get_symbol_tool_rejects_invalid_kind_filter() {
        let mut params = json_object(&[
            ("repo_id", "repo-1"),
            ("relative_path", "src/main.rs"),
            ("symbol_name", "main"),
        ]);
        params.insert(
            "kind_filter".to_string(),
            serde_json::Value::String("invalid_kind".to_string()),
        );
        let err = parse_get_symbol_params(&params).unwrap_err();
        assert!(err.message.contains("unknown kind_filter"));
    }

    #[test]
    fn test_get_symbol_tool_accepts_valid_kind_filter() {
        let mut params = json_object(&[
            ("repo_id", "repo-1"),
            ("relative_path", "src/main.rs"),
            ("symbol_name", "main"),
        ]);
        params.insert(
            "kind_filter".to_string(),
            serde_json::Value::String("function".to_string()),
        );
        let result = parse_get_symbol_params(&params).unwrap();
        assert_eq!(result.kind_filter, Some(SymbolKind::Function));
    }

    #[test]
    fn test_get_symbol_tool_accepts_missing_kind_filter() {
        let params = json_object(&[
            ("repo_id", "repo-1"),
            ("relative_path", "src/main.rs"),
            ("symbol_name", "main"),
        ]);
        let result = parse_get_symbol_params(&params).unwrap();
        assert_eq!(result.kind_filter, None);
    }

    #[test]
    fn test_get_symbols_tool_accepts_legacy_symbols_array() {
        let params = json_object_values(&[
            ("repo_id", serde_json::json!("repo-1")),
            (
                "symbols",
                serde_json::json!([
                    {
                        "relative_path": "src/main.rs",
                        "symbol_name": "main",
                        "kind_filter": "function"
                    }
                ]),
            ),
        ]);

        let result = parse_get_symbols_params(&params).unwrap();
        assert_eq!(result.repo_id, "repo-1");
        assert_eq!(
            result.requests,
            vec![BatchRetrievalRequest::Symbol {
                relative_path: "src/main.rs".to_string(),
                symbol_name: "main".to_string(),
                kind_filter: Some(SymbolKind::Function),
            }]
        );
    }

    #[test]
    fn test_get_symbols_tool_accepts_targets_with_code_slice() {
        let params = json_object_values(&[
            ("repo_id", serde_json::json!("repo-1")),
            (
                "targets",
                serde_json::json!([
                    {
                        "request_type": "symbol",
                        "relative_path": "src/main.rs",
                        "symbol_name": "main"
                    },
                    {
                        "request_type": "code_slice",
                        "relative_path": "src/main.rs",
                        "byte_range": [0, 12]
                    }
                ]),
            ),
        ]);

        let result = parse_get_symbols_params(&params).unwrap();
        assert_eq!(result.repo_id, "repo-1");
        assert_eq!(
            result.requests,
            vec![
                BatchRetrievalRequest::Symbol {
                    relative_path: "src/main.rs".to_string(),
                    symbol_name: "main".to_string(),
                    kind_filter: None,
                },
                BatchRetrievalRequest::CodeSlice {
                    relative_path: "src/main.rs".to_string(),
                    byte_range: (0, 12),
                },
            ]
        );
    }

    #[test]
    fn test_get_symbols_tool_rejects_missing_targets_and_symbols() {
        let params = json_object(&[("repo_id", "repo-1")]);
        let err = parse_get_symbols_params(&params).unwrap_err();
        assert!(
            err.message
                .contains("missing required parameter: targets or symbols")
        );
    }

    #[test]
    fn test_get_symbols_tool_rejects_invalid_code_slice_byte_range() {
        let params = json_object_values(&[
            ("repo_id", serde_json::json!("repo-1")),
            (
                "targets",
                serde_json::json!([
                    {
                        "request_type": "code_slice",
                        "relative_path": "src/main.rs",
                        "byte_range": [0]
                    }
                ]),
            ),
        ]);

        let err = parse_get_symbols_params(&params).unwrap_err();
        assert!(err.message.contains("invalid `byte_range`"));
    }
}
