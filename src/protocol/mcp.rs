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

use crate::domain::{IndexRunMode, IndexRunStatus};
use crate::{ApplicationContext, TokenizorError};

const RUN_STATUS_URI_PREFIX: &str = "tokenizor://runs/";
const RUN_STATUS_URI_SUFFIX: &str = "/status";

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
                    format!("unknown indexing mode: `{other}`. Valid modes: full, incremental, repair, verify"),
                    None,
                ));
            }
        };

        let (run, _progress) = self
            .application
            .launch_indexing(&repo_id, run_mode, repo_root)
            .map_err(to_mcp_error)?;

        let payload = serde_json::to_string(&run).map_err(|error| {
            McpError::internal_error(
                format!("failed to serialize index run: {error}"),
                None,
            )
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
    fn cancel_index_run(&self, params: rmcp::model::JsonObject) -> Result<CallToolResult, McpError> {
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
            "tokenizor_agentic_mcp is a Rust-native MCP server for indexing and retrieval. This foundation slice exposes deployment-aware health while the durable SpacetimeDB control plane and local byte-exact CAS are brought online.",
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
                        description: Some(format!(
                            "Status and health for indexing run {}",
                            id
                        )),
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
        TokenizorError::InvalidOperation(message) => {
            McpError::invalid_params(format!("invalid operation: {message}"), None)
        }
        TokenizorError::NotFound(message) => McpError::invalid_params(message, None),
        TokenizorError::Integrity(message) => {
            McpError::internal_error(format!("integrity violation: {message}"), None)
        }
        TokenizorError::Storage(message) => McpError::internal_error(message, None),
        TokenizorError::ControlPlane(message) => McpError::internal_error(message, None),
        TokenizorError::Io { path, source } => McpError::internal_error(
            format!("i/o error at `{}`: {source}", path.display()),
            None,
        ),
        TokenizorError::Serialization(message) => McpError::internal_error(message, None),
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
        let uri = format!("{}{}{}", RUN_STATUS_URI_PREFIX, run_id, RUN_STATUS_URI_SUFFIX);
        let parsed = parse_run_id_from_uri(&uri).unwrap();
        assert_eq!(parsed, run_id);
    }
}
