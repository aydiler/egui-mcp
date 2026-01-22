//! MCP server binary for egui E2E testing.

mod bridge;
mod tools;

use rmcp::{
    model::{
        CallToolRequestParam, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParam, ServerCapabilities, ServerInfo, ToolsCapability,
    },
    service::{RequestContext, RoleServer},
    transport::stdio,
    ServerHandler, ServiceExt,
};
use tools::EguiMcpServer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

impl ServerHandler for EguiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: Default::default(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "egui-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "MCP server for E2E testing egui applications. \
                 First connect to an app with egui_connect, then use egui_snapshot \
                 to see the UI tree and interact with elements using their refs."
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _pagination: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_
    {
        async {
            Ok(ListToolsResult {
                tools: EguiMcpServer::tools(),
                next_cursor: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_
    {
        async move {
            let args = request
                .arguments
                .map(serde_json::Value::Object)
                .unwrap_or(serde_json::Value::Object(Default::default()));
            Ok(self.call_tool(&request.name, args).await)
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing to stderr (stdout is used for MCP communication)
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    tracing::info!("Starting egui-mcp server");

    let server = EguiMcpServer::new();
    let service = server.serve(stdio()).await?;

    tracing::info!("Server running, waiting for requests...");
    service.waiting().await?;

    Ok(())
}
