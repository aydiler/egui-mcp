//! TCP client for connecting to the egui-mcp-bridge.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Client for communicating with the egui-mcp-bridge.
pub struct BridgeClient {
    stream: Mutex<Option<BridgeStream>>,
    request_id: AtomicI64,
}

struct BridgeStream {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Request {
    jsonrpc: String,
    id: i64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct Response {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: i64,
    result: Option<serde_json::Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Clone, Deserialize)]
struct RpcError {
    #[allow(dead_code)]
    code: i32,
    message: String,
}

/// Snapshot from the bridge.
#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotResponse {
    pub tree: String,
    pub node_count: usize,
}

/// Value response from the bridge.
#[derive(Debug, Clone, Deserialize)]
pub struct ValueResponse {
    pub value: Option<String>,
    pub role: String,
    pub name: Option<String>,
}

impl BridgeClient {
    pub fn new() -> Self {
        Self {
            stream: Mutex::new(None),
            request_id: AtomicI64::new(1),
        }
    }

    /// Connect to the bridge at the given host and port.
    pub async fn connect(&self, host: &str, port: u16) -> Result<(), String> {
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;

        let (reader, writer) = stream.into_split();
        let mut guard = self.stream.lock().await;
        *guard = Some(BridgeStream {
            reader: BufReader::new(reader),
            writer,
        });

        Ok(())
    }

    /// Disconnect from the bridge.
    pub async fn disconnect(&self) {
        let mut guard = self.stream.lock().await;
        *guard = None;
    }

    /// Check if connected.
    pub async fn is_connected(&self) -> bool {
        self.stream.lock().await.is_some()
    }

    /// Get the accessibility tree snapshot.
    pub async fn get_snapshot(&self) -> Result<SnapshotResponse, String> {
        let response = self.call("get_snapshot", None).await?;
        serde_json::from_value(response).map_err(|e| format!("Invalid response: {}", e))
    }

    /// Click on a node.
    pub async fn click(&self, node_id: u64) -> Result<(), String> {
        let params = serde_json::json!({ "node_id": node_id });
        self.call("click", Some(params)).await?;
        Ok(())
    }

    /// Focus a node.
    pub async fn focus(&self, node_id: u64) -> Result<(), String> {
        let params = serde_json::json!({ "node_id": node_id });
        self.call("focus", Some(params)).await?;
        Ok(())
    }

    /// Set value on a node.
    pub async fn set_value(&self, node_id: u64, value: &str) -> Result<(), String> {
        let params = serde_json::json!({ "node_id": node_id, "value": value });
        self.call("set_value", Some(params)).await?;
        Ok(())
    }

    /// Type text into a node.
    pub async fn type_text(&self, node_id: u64, text: &str) -> Result<(), String> {
        let params = serde_json::json!({ "node_id": node_id, "text": text });
        self.call("type_text", Some(params)).await?;
        Ok(())
    }

    /// Send a key event (with optional modifiers) globally. Not targeted at a
    /// specific widget — egui's `Input::key_pressed` handlers catch it.
    pub async fn send_key(
        &self,
        key: &str,
        modifiers: &[String],
        press_only: bool,
    ) -> Result<(), String> {
        let params = serde_json::json!({
            "key": key,
            "modifiers": modifiers,
            "press_only": press_only,
        });
        self.call("send_key", Some(params)).await?;
        Ok(())
    }

    /// Hover over a node.
    pub async fn hover(&self, node_id: u64) -> Result<(), String> {
        let params = serde_json::json!({ "node_id": node_id });
        self.call("hover", Some(params)).await?;
        Ok(())
    }

    /// Get value of a node.
    pub async fn get_value(&self, node_id: u64) -> Result<ValueResponse, String> {
        let params = serde_json::json!({ "node_id": node_id });
        let response = self.call("get_value", Some(params)).await?;
        serde_json::from_value(response).map_err(|e| format!("Invalid response: {}", e))
    }

    /// Scroll at a position.
    pub async fn scroll(&self, x: f32, y: f32, delta_x: f32, delta_y: f32) -> Result<(), String> {
        let params = serde_json::json!({
            "x": x,
            "y": y,
            "delta_x": delta_x,
            "delta_y": delta_y
        });
        self.call("scroll", Some(params)).await?;
        Ok(())
    }

    /// Drag the pointer from (x1, y1) to (x2, y2) in screen-space pixels.
    /// Useful for text selection and any continuous-drag interaction.
    pub async fn drag(&self, x1: f32, y1: f32, x2: f32, y2: f32) -> Result<(), String> {
        let params = serde_json::json!({
            "x1": x1,
            "y1": y1,
            "x2": x2,
            "y2": y2,
        });
        self.call("drag", Some(params)).await?;
        Ok(())
    }

    async fn call(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, String> {
        let mut guard = self.stream.lock().await;
        let stream = guard.as_mut().ok_or("Not connected")?;

        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = Request {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };

        let request_json = serde_json::to_string(&request).unwrap() + "\n";
        stream
            .writer
            .write_all(request_json.as_bytes())
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        let mut line = String::new();
        stream
            .reader
            .read_line(&mut line)
            .await
            .map_err(|e| format!("Read error: {}", e))?;

        let response: Response =
            serde_json::from_str(&line).map_err(|e| format!("Parse error: {}", e))?;

        if let Some(error) = response.error {
            return Err(error.message);
        }

        response.result.ok_or_else(|| "No result".into())
    }
}

impl Default for BridgeClient {
    fn default() -> Self {
        Self::new()
    }
}
