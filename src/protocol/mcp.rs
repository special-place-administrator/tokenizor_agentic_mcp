use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{CallToolResult, Content, Implementation, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router,
};

use crate::{ApplicationContext, TokenizorError};

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
}

#[tool_handler]
impl ServerHandler for TokenizorServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::from_build_env())
            .with_instructions(
                "tokenizor_agentic_mcp is a Rust-native MCP server for indexing and retrieval. This foundation slice exposes deployment-aware health while the durable SpacetimeDB control plane and local byte-exact CAS are brought online.",
            )
    }
}

fn to_mcp_error(error: TokenizorError) -> McpError {
    match error {
        TokenizorError::Config(message) | TokenizorError::InvalidArgument(message) => {
            McpError::invalid_params(message, None)
        }
        other => McpError::internal_error(other.to_string(), None),
    }
}
