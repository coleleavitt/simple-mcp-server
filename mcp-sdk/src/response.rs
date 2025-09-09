use serde::Serialize;
use serde_json::Value;
use crate::error::JsonRpcError;

/// MCP Response structure supporting multiple JSON-RPC versions and schema variations
#[derive(Debug, Serialize)]
pub struct MCPResponse {
    /// JSON-RPC version string - optional for 1.0, required for 2.0
    #[cfg(feature = "jsonrpc-1")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<String>,
    /// JSON-RPC version string, always "2.0" in strict mode
    #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
    pub jsonrpc: String,

    /// Request ID (null for notifications)
    pub id: Option<Value>,

    /// Response result (success case)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Response error (error case)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl MCPResponse {
    /// Helper for request too large error
    pub fn too_large() -> Self {
        MCPResponse {
            #[cfg(feature = "jsonrpc-1")]
            jsonrpc: None,
            #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
            jsonrpc: "2.0".into(),
            id: None,
            #[cfg(feature = "jsonrpc-1")]
            result: Some(Value::Null), // JSON-RPC 1.0 style
            #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
            result: None, // JSON-RPC 2.0 style
            error: Some(JsonRpcError {
                code: -32700,
                message: "Request too large".into(),
                data: None
            }),
        }
    }

    /// Helper for parse error
    pub fn parse_error() -> Self {
        MCPResponse {
            #[cfg(feature = "jsonrpc-1")]
            jsonrpc: None,
            #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
            jsonrpc: "2.0".into(),
            id: None,
            #[cfg(feature = "jsonrpc-1")]
            result: Some(Value::Null), // JSON-RPC 1.0 style
            #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
            result: None, // JSON-RPC 2.0 style
            error: Some(JsonRpcError {
                code: -32700,
                message: "Parse error".into(),
                data: None
            }),
        }
    }

    /// Create a JSON-RPC 1.0 success response
    #[cfg(feature = "jsonrpc-1")]
    pub fn v1_success(id: Option<Value>, result: Value) -> Self {
        MCPResponse {
            jsonrpc: None,
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create a JSON-RPC 1.0 error response
    #[cfg(feature = "jsonrpc-1")]
    pub fn v1_error(id: Option<Value>, error: JsonRpcError) -> Self {
        MCPResponse {
            jsonrpc: None,
            id,
            result: Some(Value::Null), // 1.0 style: null result on error
            error: Some(error),
        }
    }

    /// Create a JSON-RPC 2.0 success response
    #[cfg(feature = "jsonrpc-2")]
    pub fn v2_success(id: Option<Value>, result: Value) -> Self {
        MCPResponse {
            #[cfg(feature = "jsonrpc-1")]
            jsonrpc: Some("2.0".into()),
            #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create a JSON-RPC 2.0 error response
    #[cfg(feature = "jsonrpc-2")]
    pub fn v2_error(id: Option<Value>, error: JsonRpcError) -> Self {
        MCPResponse {
            #[cfg(feature = "jsonrpc-1")]
            jsonrpc: Some("2.0".into()),
            #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }

    /// Create a success response using the appropriate version
    pub fn success(id: Option<Value>, result: Value) -> Self {
        #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
        {
            Self::v2_success(id, result)
        }
        #[cfg(feature = "jsonrpc-1")]
        {
            Self::v1_success(id, result)
        }
    }

    /// Create an error response using the appropriate version
    pub fn error(id: Option<Value>, error: JsonRpcError) -> Self {
        #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
        {
            Self::v2_error(id, error)
        }
        #[cfg(feature = "jsonrpc-1")]
        {
            Self::v1_error(id, error)
        }
    }

    /// Create notification response (should be None)
    pub fn notification() -> Option<Self> {
        None // Notifications don't get responses
    }

    /// Check if this is a JSON-RPC 2.0 response
    pub fn is_v2(&self) -> bool {
        #[cfg(feature = "jsonrpc-1")]
        {
            self.jsonrpc.as_deref() == Some("2.0")
        }
        #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
        {
            true // Always 2.0 in strict mode
        }
    }

    /// Check if this is a JSON-RPC 1.0 response
    pub fn is_v1(&self) -> bool {
        #[cfg(feature = "jsonrpc-1")]
        {
            self.jsonrpc.is_none() || self.jsonrpc.as_deref() == Some("1.0")
        }
        #[cfg(all(feature = "jsonrpc-2", not(feature = "jsonrpc-1")))]
        {
            false // Never 1.0 in strict mode
        }
    }

    /// Check if this response indicates success
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }

    /// Check if this response indicates an error
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_success_response() {
        let resp = MCPResponse::success(Some(json!(1)), json!("test"));
        assert!(resp.is_success());
        assert_eq!(resp.result, Some(json!("test")));
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_error_response() {
        let error = JsonRpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        };
        let resp = MCPResponse::error(Some(json!(1)), error);
        assert!(resp.is_error());
        assert!(resp.error.is_some());
    }

    #[cfg(feature = "jsonrpc-2")]
    #[test]
    fn test_v2_success_response() {
        let resp = MCPResponse::v2_success(Some(json!(1)), json!("test"));
        assert!(resp.is_v2());
        assert!(resp.is_success());
        assert_eq!(resp.result, Some(json!("test")));
        assert!(resp.error.is_none());
    }

    #[cfg(feature = "jsonrpc-2")]
    #[test]
    fn test_v2_error_response() {
        let error = JsonRpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        };
        let resp = MCPResponse::v2_error(Some(json!(1)), error);
        assert!(resp.is_v2());
        assert!(resp.is_error());
        assert!(resp.error.is_some());
    }

    #[cfg(feature = "jsonrpc-1")]
    #[test]
    fn test_v1_success_response() {
        let resp = MCPResponse::v1_success(Some(json!(1)), json!("test"));
        assert!(resp.is_v1());
        assert!(resp.is_success());
        assert_eq!(resp.result, Some(json!("test")));
        assert!(resp.error.is_none());
    }

    #[cfg(feature = "jsonrpc-1")]
    #[test]
    fn test_v1_error_response() {
        let error = JsonRpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        };
        let resp = MCPResponse::v1_error(Some(json!(1)), error);
        assert!(resp.is_v1());
        assert!(resp.is_error());
        assert_eq!(resp.result, Some(Value::Null)); // 1.0 style: null result on error
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_parse_error_helper() {
        let resp = MCPResponse::parse_error();
        assert!(resp.is_error());
        assert!(resp.error.is_some());
        if let Some(error) = &resp.error {
            assert_eq!(error.code, -32700);
        }
    }

    #[test]
    fn test_too_large_helper() {
        let resp = MCPResponse::too_large();
        assert!(resp.is_error());
        assert!(resp.error.is_some());
        if let Some(error) = &resp.error {
            assert_eq!(error.code, -32700);
        }
    }
}
