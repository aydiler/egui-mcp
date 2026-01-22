//! MCP tool implementations for egui testing.

use crate::bridge::BridgeClient;
use rmcp::{
    model::{CallToolResult, Content, Tool},
    ErrorData as McpError,
};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Convert a schemars schema to a JSON object for rmcp Tool.
fn schema_to_json_object<T: JsonSchema>() -> Arc<Map<String, Value>> {
    let schema = schema_for!(T);
    let value = serde_json::to_value(&schema).unwrap_or(Value::Object(Map::new()));
    match value {
        Value::Object(map) => Arc::new(map),
        _ => Arc::new(Map::new()),
    }
}

/// Create an empty schema (accepts any input).
fn empty_schema() -> Arc<Map<String, Value>> {
    let mut map = Map::new();
    map.insert("type".to_string(), Value::String("object".to_string()));
    Arc::new(map)
}

/// MCP server for egui E2E testing.
#[derive(Clone)]
pub struct EguiMcpServer {
    pub client: Arc<Mutex<BridgeClient>>,
}

impl EguiMcpServer {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(BridgeClient::new())),
        }
    }

    pub fn tools() -> Vec<Tool> {
        vec![
            Tool::new(
                "egui_connect",
                "Connect to egui app bridge. Must be called before other tools.",
                schema_to_json_object::<ConnectParams>(),
            ),
            Tool::new(
                "egui_disconnect",
                "Disconnect from the egui app.",
                empty_schema(),
            ),
            Tool::new(
                "egui_status",
                "Check if connected to an egui app.",
                empty_schema(),
            ),
            Tool::new(
                "egui_snapshot",
                "Get accessibility tree snapshot. Returns elements with refs like [ref=n3].",
                empty_schema(),
            ),
            Tool::new(
                "egui_click",
                "Click element by ref (e.g., 'n3').",
                schema_to_json_object::<RefParams>(),
            ),
            Tool::new(
                "egui_type",
                "Type text into input element. Focuses the element first.",
                schema_to_json_object::<TypeParams>(),
            ),
            Tool::new(
                "egui_fill",
                "Set value directly on element (for sliders, spinboxes).",
                schema_to_json_object::<FillParams>(),
            ),
            Tool::new(
                "egui_focus",
                "Focus element by ref.",
                schema_to_json_object::<RefParams>(),
            ),
            Tool::new(
                "egui_hover",
                "Hover over element by ref.",
                schema_to_json_object::<RefParams>(),
            ),
            Tool::new(
                "egui_get_value",
                "Get current value of element.",
                schema_to_json_object::<RefParams>(),
            ),
        ]
    }

    pub async fn call_tool(&self, name: &str, args: serde_json::Value) -> CallToolResult {
        match name {
            "egui_connect" => {
                let params: ConnectParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let client = self.client.lock().await;
                match client.connect(&params.host, params.port).await {
                    Ok(()) => success(format!(
                        "Connected to egui app at {}:{}",
                        params.host, params.port
                    )),
                    Err(e) => error(format!("Connection failed: {}", e)),
                }
            }
            "egui_disconnect" => {
                let client = self.client.lock().await;
                client.disconnect().await;
                success("Disconnected")
            }
            "egui_status" => {
                let client = self.client.lock().await;
                if client.is_connected().await {
                    success("Connected")
                } else {
                    success("Not connected")
                }
            }
            "egui_snapshot" => {
                let client = self.client.lock().await;
                match client.get_snapshot().await {
                    Ok(snapshot) => {
                        let msg = format!(
                            "Snapshot ({} nodes):\n\n{}",
                            snapshot.node_count, snapshot.tree
                        );
                        success(msg)
                    }
                    Err(e) => error(format!("Snapshot failed: {}", e)),
                }
            }
            "egui_click" => {
                let params: RefParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let node_id = match parse_ref(&params.r#ref) {
                    Ok(id) => id,
                    Err(_) => return error(format!("Invalid ref: {}", params.r#ref)),
                };
                let client = self.client.lock().await;
                match client.click(node_id).await {
                    Ok(()) => success(format!("Clicked {}", params.r#ref)),
                    Err(e) => error(format!("Click failed: {}", e)),
                }
            }
            "egui_type" => {
                let params: TypeParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let node_id = match parse_ref(&params.r#ref) {
                    Ok(id) => id,
                    Err(_) => return error(format!("Invalid ref: {}", params.r#ref)),
                };
                let client = self.client.lock().await;
                match client.type_text(node_id, &params.text).await {
                    Ok(()) => success(format!("Typed '{}' into {}", params.text, params.r#ref)),
                    Err(e) => error(format!("Type failed: {}", e)),
                }
            }
            "egui_fill" => {
                let params: FillParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let node_id = match parse_ref(&params.r#ref) {
                    Ok(id) => id,
                    Err(_) => return error(format!("Invalid ref: {}", params.r#ref)),
                };
                let client = self.client.lock().await;
                match client.set_value(node_id, &params.value).await {
                    Ok(()) => success(format!("Set {} to '{}'", params.r#ref, params.value)),
                    Err(e) => error(format!("Fill failed: {}", e)),
                }
            }
            "egui_focus" => {
                let params: RefParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let node_id = match parse_ref(&params.r#ref) {
                    Ok(id) => id,
                    Err(_) => return error(format!("Invalid ref: {}", params.r#ref)),
                };
                let client = self.client.lock().await;
                match client.focus(node_id).await {
                    Ok(()) => success(format!("Focused {}", params.r#ref)),
                    Err(e) => error(format!("Focus failed: {}", e)),
                }
            }
            "egui_hover" => {
                let params: RefParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let node_id = match parse_ref(&params.r#ref) {
                    Ok(id) => id,
                    Err(_) => return error(format!("Invalid ref: {}", params.r#ref)),
                };
                let client = self.client.lock().await;
                match client.hover(node_id).await {
                    Ok(()) => success(format!("Hovering over {}", params.r#ref)),
                    Err(e) => error(format!("Hover failed: {}", e)),
                }
            }
            "egui_get_value" => {
                let params: RefParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let node_id = match parse_ref(&params.r#ref) {
                    Ok(id) => id,
                    Err(_) => return error(format!("Invalid ref: {}", params.r#ref)),
                };
                let client = self.client.lock().await;
                match client.get_value(node_id).await {
                    Ok(resp) => {
                        let value_str = resp.value.unwrap_or_else(|| "(no value)".into());
                        let msg = format!(
                            "Element {}: role={}, name={}, value={}",
                            params.r#ref,
                            resp.role,
                            resp.name.unwrap_or_else(|| "(none)".into()),
                            value_str
                        );
                        success(msg)
                    }
                    Err(e) => error(format!("Get value failed: {}", e)),
                }
            }
            _ => error(format!("Unknown tool: {}", name)),
        }
    }
}

impl Default for EguiMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_ref(r: &str) -> Result<u64, McpError> {
    let num_str = r.strip_prefix('n').unwrap_or(r);
    num_str.parse().map_err(|_| {
        McpError::invalid_params(
            format!("Invalid ref '{}': expected format 'n<number>'", r),
            None,
        )
    })
}

fn success(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![Content::text(msg.into())])
}

fn error(msg: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![Content::text(msg.into())])
}

// Tool parameter types
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ConnectParams {
    /// Host address (e.g., "127.0.0.1")
    pub host: String,
    /// Port number (e.g., 9876)
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RefParams {
    /// Element reference from snapshot (e.g., "n3")
    pub r#ref: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct TypeParams {
    /// Element reference from snapshot
    pub r#ref: String,
    /// Text to type
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FillParams {
    /// Element reference from snapshot
    pub r#ref: String,
    /// Value to set
    pub value: String,
}
