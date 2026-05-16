//! MCP tool implementations for egui testing.

use crate::bridge::BridgeClient;
use rmcp::{
    model::{CallToolResult, Content, Tool},
    ErrorData as McpError,
};
use schemars::{schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

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
    /// Launched app process (if any)
    pub launched_process: Arc<Mutex<Option<Child>>>,
}

impl EguiMcpServer {
    pub fn new() -> Self {
        Self {
            client: Arc::new(Mutex::new(BridgeClient::new())),
            launched_process: Arc::new(Mutex::new(None)),
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
                "egui_launch",
                "Launch an egui application with optional environment variables. Auto-detects X11 mode when DISPLAY is set (for virtual displays like Xvfb). Waits for MCP bridge, then auto-connects. NOTE: Performs pre-flight checks to verify (1) binary was compiled with MCP support (--features mcp), and (2) Xvfb is running on :99.",
                schema_to_json_object::<LaunchParams>(),
            ),
            Tool::new(
                "egui_kill",
                "Kill the launched egui application.",
                empty_schema(),
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
                "egui_key",
                "Send a keyboard event with optional modifiers (global — not targeted at a widget). \
                 Examples: {key: \"F\", modifiers: [\"ctrl\"]} for Ctrl+F, \
                 {key: \"Escape\"} for Esc, {key: \"F5\"} for F5. \
                 Supported keys: A-Z, 0-9, F1-F35, Enter, Escape, Tab, Space, Backspace, Delete, \
                 Home, End, PageUp, PageDown, Insert, ArrowLeft/Right/Up/Down, and common symbols. \
                 Modifiers (case-insensitive): ctrl, shift, alt, command. \
                 Default sends press+release; set press_only=true to hold the key.",
                schema_to_json_object::<KeyParams>(),
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
            Tool::new(
                "egui_scroll",
                "Scroll at a position. Use to test scroll isolation between panels.",
                schema_to_json_object::<ScrollParams>(),
            ),
            Tool::new(
                "egui_drag",
                "Drag the pointer from (x1, y1) to (x2, y2) in screen-space pixels. \
                 Use for text selection in selectable_label widgets, or any continuous-drag \
                 interaction. Coordinates are absolute (find them via egui_snapshot rects).",
                schema_to_json_object::<DragParams>(),
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
            "egui_launch" => {
                let params: LaunchParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };

                // === PRE-FLIGHT CHECK: Verify binary has MCP support ===
                // This prevents waiting 10s only to fail with a cryptic error
                if let Err(e) = check_mcp_support(&params.application_path) {
                    return error(e);
                }

                // Check if already launched
                {
                    let proc = self.launched_process.lock().await;
                    if proc.is_some() {
                        return error("An app is already launched. Use egui_kill first.");
                    }
                }

                let port = params.port.unwrap_or(9877);
                let host = params.host.clone().unwrap_or_else(|| "127.0.0.1".to_string());

                // === DISPLAY ENFORCEMENT: Ensure virtual display only ===
                // This prevents accidentally launching on real display (:0)
                const ENFORCED_DISPLAY: &str = ":99";

                // Check if user specified DISPLAY
                let mut enforced_env = params.env.clone().unwrap_or_default();
                if let Some(display) = enforced_env.get("DISPLAY") {
                    if display == ":0" || display.is_empty() {
                        return error(format!(
                            "❌ Real display (:0) is forbidden for E2E testing\n\
                             Use DISPLAY={} for isolated testing\n\
                             This prevents interfering with your desktop",
                            ENFORCED_DISPLAY
                        ));
                    }
                }

                // Force virtual display (override or set default)
                enforced_env.insert("DISPLAY".to_string(), ENFORCED_DISPLAY.to_string());

                // Force X11 backend for Wayland systems
                if !enforced_env.contains_key("WINIT_UNIX_BACKEND") {
                    enforced_env.insert("WINIT_UNIX_BACKEND".to_string(), "x11".to_string());
                }
                enforced_env.remove("WAYLAND_DISPLAY"); // Ensure Wayland doesn't interfere

                // === END ENFORCEMENT ===

                // === PRE-FLIGHT CHECK: Verify Xvfb is running ===
                // This prevents cryptic errors when socket exists but Xvfb died
                if let Err(e) = check_xvfb_running(ENFORCED_DISPLAY) {
                    return error(e);
                }

                // Build command
                let mut cmd = Command::new(&params.application_path);

                // Set working directory if provided (for resolving relative paths in args)
                if let Some(ref cwd) = params.cwd {
                    cmd.current_dir(cwd);
                }

                // Add arguments if provided
                if let Some(ref args) = params.args {
                    cmd.args(args);
                }

                // Set enforced environment variables
                for (key, value) in &enforced_env {
                    cmd.env(key, value);
                }

                // Explicitly remove WAYLAND_DISPLAY from child process
                // (enforced_env.remove() only removes from hashmap, not inherited env)
                cmd.env_remove("WAYLAND_DISPLAY");

                // Spawn the process
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                let child = match cmd.spawn() {
                    Ok(c) => c,
                    Err(e) => return error(format!("Failed to launch app: {}", e)),
                };

                let pid = child.id();

                // Store the process
                {
                    let mut proc = self.launched_process.lock().await;
                    *proc = Some(child);
                }

                // Wait for the MCP bridge to become available
                let timeout_secs = params.timeout.unwrap_or(10);
                let start = std::time::Instant::now();
                let mut connected = false;

                while start.elapsed().as_secs() < timeout_secs as u64 {
                    let client = self.client.lock().await;
                    if client.connect(&host, port).await.is_ok() {
                        connected = true;
                        break;
                    }
                    drop(client);
                    sleep(Duration::from_millis(200)).await;
                }

                if connected {
                    let env_info = params.env.as_ref().map_or(String::new(), |env| {
                        let vars: Vec<String> = env.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                        format!(" (env: {})", vars.join(", "))
                    });
                    success(format!(
                        "Launched {} (PID {}) and connected to {}:{}{}",
                        params.application_path, pid, host, port, env_info
                    ))
                } else {
                    // Kill the process since we couldn't connect
                    {
                        let mut proc = self.launched_process.lock().await;
                        if let Some(mut p) = proc.take() {
                            let _ = p.kill();
                        }
                    }
                    error(format!(
                        "Launched app but MCP bridge not available at {}:{} within {}s. App killed.",
                        host, port, timeout_secs
                    ))
                }
            }
            "egui_kill" => {
                let mut proc = self.launched_process.lock().await;
                if let Some(mut child) = proc.take() {
                    match child.kill() {
                        Ok(()) => {
                            // Also disconnect
                            let client = self.client.lock().await;
                            client.disconnect().await;
                            success("Killed launched app and disconnected")
                        }
                        Err(e) => error(format!("Failed to kill app: {}", e)),
                    }
                } else {
                    error("No launched app to kill")
                }
            }
            "egui_disconnect" => {
                let client = self.client.lock().await;
                client.disconnect().await;
                success("Disconnected")
            }
            "egui_status" => {
                let client = self.client.lock().await;
                let proc = self.launched_process.lock().await;
                let launched = proc.is_some();
                let connected = client.is_connected().await;
                
                let status = match (launched, connected) {
                    (true, true) => "App launched and connected",
                    (true, false) => "App launched but not connected",
                    (false, true) => "Connected (externally launched app)",
                    (false, false) => "Not connected",
                };
                success(status)
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
            "egui_key" => {
                let params: KeyParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let client = self.client.lock().await;
                match client
                    .send_key(&params.key, &params.modifiers, params.press_only)
                    .await
                {
                    Ok(()) => {
                        let mods_str = if params.modifiers.is_empty() {
                            String::new()
                        } else {
                            format!("{}+", params.modifiers.join("+"))
                        };
                        let suffix = if params.press_only { " (press_only)" } else { "" };
                        success(format!("Sent key {}{}{}", mods_str, params.key, suffix))
                    }
                    Err(e) => error(format!("Key failed: {}", e)),
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
            "egui_scroll" => {
                let params: ScrollParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let client = self.client.lock().await;
                match client.scroll(params.x, params.y, params.delta_x, params.delta_y).await {
                    Ok(()) => success(format!(
                        "Scrolled at ({}, {}) by delta ({}, {})",
                        params.x, params.y, params.delta_x, params.delta_y
                    )),
                    Err(e) => error(format!("Scroll failed: {}", e)),
                }
            }
            "egui_drag" => {
                let params: DragParams = match serde_json::from_value(args) {
                    Ok(p) => p,
                    Err(e) => return error(format!("Invalid params: {}", e)),
                };
                let client = self.client.lock().await;
                match client
                    .drag(params.x1, params.y1, params.x2, params.y2)
                    .await
                {
                    Ok(()) => success(format!(
                        "Dragged from ({}, {}) to ({}, {})",
                        params.x1, params.y1, params.x2, params.y2
                    )),
                    Err(e) => error(format!("Drag failed: {}", e)),
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

/// Check if a binary was compiled with MCP bridge support.
/// Uses `strings` command to look for known MCP bridge markers.
fn check_mcp_support(binary_path: &str) -> Result<(), String> {
    // Check if file exists first
    if !std::path::Path::new(binary_path).exists() {
        return Err(format!("Binary not found: {}", binary_path));
    }

    // Run `strings` on the binary to extract readable strings
    let output = Command::new("strings")
        .arg(binary_path)
        .output()
        .map_err(|e| format!("Failed to run 'strings' command: {}", e))?;

    if !output.status.success() {
        // If strings command fails, skip the check (don't block on tool availability)
        tracing::warn!(
            "Could not inspect binary with 'strings' command, skipping MCP support check"
        );
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Look for known MCP bridge markers
    // These strings are present in binaries compiled with egui-mcp-bridge
    let mcp_markers = [
        "MCP bridge listening on port",
        "egui_mcp_bridge",
        "Bridge server listening on port",
    ];

    let has_mcp = mcp_markers.iter().any(|marker| stdout.contains(marker));

    if !has_mcp {
        return Err(format!(
            "❌ Binary '{}' was NOT compiled with MCP bridge support.\n\n\
             The egui-mcp-bridge library is not linked into this binary.\n\n\
             To fix, rebuild with the 'mcp' feature enabled:\n\
             \n\
                 cargo build --features mcp\n\
             \n\
             Or if using a Makefile:\n\
             \n\
                 make dev   # (if configured for MCP builds)",
            binary_path
        ));
    }

    Ok(())
}

/// Check if Xvfb (or any X server) is running and accepting connections on the given display.
/// This prevents cryptic errors when the display socket exists but Xvfb isn't running.
fn check_xvfb_running(display: &str) -> Result<(), String> {
    // Try xdpyinfo first (most reliable)
    let output = Command::new("xdpyinfo")
        .env("DISPLAY", display)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(o) if o.status.success() => return Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            // Check for common error patterns
            if stderr.contains("unable to open display") {
                // Display socket might exist but Xvfb isn't running (stale state)
                let display_num = display.trim_start_matches(':');
                return Err(format!(
                    "❌ X11 display {} is not responding.\n\n\
                     Xvfb may have died leaving stale socket files.\n\n\
                     To fix, run:\n\
                     \n\
                         pkill -9 Xvfb 2>/dev/null\n\
                         rm -f /tmp/.X{}-lock /tmp/.X11-unix/X{}\n\
                         Xvfb {} -screen 0 1920x1080x24 &\n\
                         sleep 2\n\
                     \n\
                     Then retry egui_launch.",
                    display, display_num, display_num, display
                ));
            }
        }
        Err(_) => {
            // xdpyinfo not available, try alternative check
            tracing::warn!("xdpyinfo not available, skipping Xvfb check");
            return Ok(());
        }
    }

    // Fallback: check if X socket exists (less reliable but better than nothing)
    let display_num = display.trim_start_matches(':');
    let socket_path = format!("/tmp/.X11-unix/X{}", display_num);

    if !std::path::Path::new(&socket_path).exists() {
        return Err(format!(
            "❌ X11 display {} not found.\n\n\
             No X server is running on this display.\n\n\
             To start Xvfb, run:\n\
             \n\
                 Xvfb {} -screen 0 1920x1080x24 &\n\
                 sleep 2\n\
             \n\
             Then retry egui_launch.",
            display, display
        ));
    }

    Ok(())
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
pub struct LaunchParams {
    /// Path to the egui application binary
    pub application_path: String,
    /// Port the app's MCP bridge listens on (default: 9877)
    #[serde(default)]
    pub port: Option<u16>,
    /// Host to connect to (default: "127.0.0.1")
    #[serde(default)]
    pub host: Option<String>,
    /// Command-line arguments to pass to the application
    #[serde(default)]
    pub args: Option<Vec<String>>,
    /// Environment variables to set (e.g., {"DISPLAY": ":99"} for virtual X11)
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    /// Timeout in seconds to wait for MCP bridge (default: 10)
    #[serde(default)]
    pub timeout: Option<u32>,
    /// Working directory for the application (for resolving relative paths in args)
    #[serde(default)]
    pub cwd: Option<String>,
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
pub struct KeyParams {
    /// Key name (case-insensitive). Examples: "F", "Escape", "F5", "ArrowLeft", "Enter".
    pub key: String,
    /// Modifier names (case-insensitive). Subset of "ctrl", "shift", "alt", "command".
    #[serde(default)]
    pub modifiers: Vec<String>,
    /// If true, emit only the key-press event (don't auto-release). Default false.
    #[serde(default)]
    pub press_only: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct FillParams {
    /// Element reference from snapshot
    pub r#ref: String,
    /// Value to set
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ScrollParams {
    /// X position to scroll at
    pub x: f32,
    /// Y position to scroll at
    pub y: f32,
    /// Horizontal scroll delta (positive = right)
    #[serde(default)]
    pub delta_x: f32,
    /// Vertical scroll delta (positive = down, negative = up)
    pub delta_y: f32,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DragParams {
    /// Drag start X (absolute screen-space px)
    pub x1: f32,
    /// Drag start Y (absolute screen-space px)
    pub y1: f32,
    /// Drag end X (absolute screen-space px)
    pub x2: f32,
    /// Drag end Y (absolute screen-space px)
    pub y2: f32,
}
