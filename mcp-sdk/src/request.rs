use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct MCPRequest {
    /// JSON-RPC version string, present for v2.0, absent for v1.0
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}
