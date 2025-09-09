// mcp-sdk/src/request.rs

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A progress token, used to associate progress notifications with the original request.
/// This now lives in `request.rs` as it's part of the incoming request structure.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum ProgressToken {
    Integer(i64),
    String(String),
}

/// The `_meta` field can be attached to any request or result.
#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RequestMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_token: Option<ProgressToken>,
}

/// A client request that expects a response.
#[derive(Debug, Deserialize)]
pub struct MCPRequest {
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
    #[serde(rename = "_meta")]
    pub meta: Option<RequestMeta>,
}

impl MCPRequest {
    pub fn jsonrpc_version(&self) -> Option<&str> {
        self.jsonrpc.as_deref()
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}