//! JSON-RPC 2.0 protocol types for bridge communication.

use serde::{Deserialize, Serialize};

/// JSON-RPC 2.0 request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn success(id: RequestId, result: impl Serialize) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(serde_json::to_value(result).unwrap_or(serde_json::Value::Null)),
            error: None,
        }
    }

    pub fn error(id: RequestId, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Request ID (can be number or string).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

/// JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

// Application error codes
pub const NODE_NOT_FOUND: i32 = -32000;
pub const ACTION_FAILED: i32 = -32001;

/// Request parameters for various methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum BridgeMethod {
    #[serde(rename = "get_snapshot")]
    GetSnapshot,

    #[serde(rename = "click")]
    Click { node_id: u64 },

    #[serde(rename = "focus")]
    Focus { node_id: u64 },

    #[serde(rename = "set_value")]
    SetValue { node_id: u64, value: String },

    #[serde(rename = "type_text")]
    TypeText { node_id: u64, text: String },

    #[serde(rename = "hover")]
    Hover { node_id: u64 },

    #[serde(rename = "get_value")]
    GetValue { node_id: u64 },
}

/// Click parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClickParams {
    pub node_id: u64,
}

/// Focus parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusParams {
    pub node_id: u64,
}

/// Set value parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetValueParams {
    pub node_id: u64,
    pub value: String,
}

/// Type text parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeTextParams {
    pub node_id: u64,
    pub text: String,
}

/// Hover parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverParams {
    pub node_id: u64,
}

/// Get value parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetValueParams {
    pub node_id: u64,
}

/// Snapshot response containing the serialized tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResponse {
    pub tree: String,
    pub node_count: usize,
}

/// Simple success response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Value response for get_value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueResponse {
    pub value: Option<String>,
    pub role: String,
    pub name: Option<String>,
}
