use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct MCPRequest {
    /// JSON-RPC version string
    #[cfg(feature = "jsonrpc-1")]
    pub jsonrpc: Option<String>,
    /// JSON-RPC version string, required in strict mode (schema-draft)
    #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
    pub jsonrpc: String,
    
    /// Request ID
    #[cfg(feature = "schema-june-2025")]
    pub id: Option<Value>,
    /// Request ID - required in draft schema for requests, omitted for notifications
    #[cfg(all(feature = "schema-draft", not(feature = "schema-june-2025")))]
    pub id: Option<Value>,  // Still optional for notifications
    
    pub method: String,
    pub params: Option<Value>,
}

impl MCPRequest {
    /// Get the JSON-RPC version, handling both optional and required cases
    pub fn jsonrpc_version(&self) -> Option<&str> {
        #[cfg(feature = "jsonrpc-1")]
        {
            self.jsonrpc.as_deref()
        }
        #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
        {
            Some(&self.jsonrpc)
        }
    }
    
    /// Check if this is a JSON-RPC 2.0 request
    pub fn is_v2(&self) -> bool {
        self.jsonrpc_version() == Some("2.0")
    }
    
    /// Check if this is a JSON-RPC 1.0 request (no version field or version "1.0")
    pub fn is_v1(&self) -> bool {
        match self.jsonrpc_version() {
            None => true,
            Some("1.0") => true,
            _ => false,
        }
    }
    
    /// Check if this is a notification (no id field)
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}
