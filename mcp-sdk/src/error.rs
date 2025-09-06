use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MCPError {
    #[error("Invalid JSON-RPC version: {0}")]
    InvalidJsonRpcVersion(String),
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    #[error("Missing parameters")]
    MissingParameters,
    #[error("Missing tool name")]
    MissingToolName,
    #[error("Unknown tool: {0}")]
    UnknownTool(String),
    #[error("Unknown prompt: {0}")]
    UnknownPrompt(String),
    #[error("Unknown resource: {0}")]
    UnknownResource(String),
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),
    #[error("Command timeout")]
    CommandTimeout,
    #[error("Output too large")]
    OutputTooLarge,
    #[error("Stream error: {0}")]
    StreamError(String),
    #[error("Request was cancelled: {0}")]
    RequestCancelled(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl MCPError {
    pub fn to_json_rpc_error(&self) -> JsonRpcError {
        let (code, message) = match self {
            MCPError::InvalidJsonRpcVersion(_) => (-32600, self.to_string()),
            MCPError::MethodNotFound(_) => (-32601, self.to_string()),
            MCPError::MissingParameters | MCPError::MissingToolName => (-32602, self.to_string()),
            MCPError::UnknownPrompt(_) | MCPError::UnknownResource(_) | MCPError::ResourceNotFound(_) => (-32602, self.to_string()),
            MCPError::RequestCancelled(_) => (-32800, self.to_string()), // Custom cancellation code
            _ => (-32603, self.to_string()),
        };
        JsonRpcError { code, message, data: None }
    }
}
