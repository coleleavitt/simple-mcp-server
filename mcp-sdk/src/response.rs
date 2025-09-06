use serde::Serialize;
use serde_json::Value;
use crate::error::JsonRpcError;

/// MCP Response structure supporting both JSON-RPC 1.0 and 2.0
#[derive(Debug, Serialize)]
pub struct MCPResponse {
    /// Only set for JSON-RPC 2.0 (omitted for 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<String>,

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
    /// Helper for request too large error (defaults to JSON-RPC 1.0 style)
    pub fn too_large() -> Self {
        MCPResponse {
            jsonrpc: None,
            id: None,
            result: Some(Value::Null), // 1.0 style: null result on error
            error: Some(JsonRpcError {
                code: -32700,
                message: "Request too large".into(),
                data: None
            }),
        }
    }

    /// Helper for parse error (defaults to JSON-RPC 1.0 style)
    pub fn parse_error() -> Self {
        MCPResponse {
            jsonrpc: None,
            id: None,
            result: Some(Value::Null), // 1.0 style: null result on error
            error: Some(JsonRpcError {
                code: -32700,
                message: "Parse error".into(),
                data: None
            }),
        }
    }

    /// Create a JSON-RPC 2.0 specific success response
    pub fn v2_success(id: Option<Value>, result: Value) -> Self {
        MCPResponse {
            jsonrpc: Some("2.0".into()),
            id,
            result: Some(result),
            error: None, // 2.0: omit error field on success
        }
    }

    /// Create a JSON-RPC 2.0 specific error response
    pub fn v2_error(id: Option<Value>, error: JsonRpcError) -> Self {
        MCPResponse {
            jsonrpc: Some("2.0".into()),
            id,
            result: None, // 2.0: omit result field on error
            error: Some(error),
        }
    }

    /// Create a JSON-RPC 1.0 specific success response
    pub fn v1_success(id: Option<Value>, result: Value) -> Self {
        MCPResponse {
            jsonrpc: None, // 1.0: no version field
            id,
            result: Some(result),
            error: None, // 1.0: null error on success
        }
    }

    /// Create a JSON-RPC 1.0 specific error response
    pub fn v1_error(id: Option<Value>, error: JsonRpcError) -> Self {
        MCPResponse {
            jsonrpc: None, // 1.0: no version field
            id,
            result: Some(Value::Null), // 1.0: null result on error
            error: Some(error),
        }
    }

    /// Create notification response (both versions - should be None)
    pub fn notification() -> Option<Self> {
        None // Notifications don't get responses
    }

    /// Check if this is a JSON-RPC 2.0 response
    pub fn is_v2(&self) -> bool {
        self.jsonrpc.as_deref() == Some("2.0")
    }

    /// Check if this is a JSON-RPC 1.0 response
    pub fn is_v1(&self) -> bool {
        self.jsonrpc.is_none()
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
    fn test_v2_success_response() {
        let resp = MCPResponse::v2_success(Some(json!(1)), json!("test"));
        assert!(resp.is_v2());
        assert!(resp.is_success());
        assert_eq!(resp.jsonrpc, Some("2.0".into()));
        assert_eq!(resp.result, Some(json!("test")));
        assert!(resp.error.is_none());
    }

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
        assert_eq!(resp.jsonrpc, Some("2.0".into()));
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_v1_success_response() {
        let resp = MCPResponse::v1_success(Some(json!(1)), json!("test"));
        assert!(resp.is_v1());
        assert!(resp.is_success());
        assert!(resp.jsonrpc.is_none());
        assert_eq!(resp.result, Some(json!("test")));
        assert!(resp.error.is_none());
    }

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
        assert!(resp.jsonrpc.is_none());
        assert_eq!(resp.result, Some(Value::Null)); // 1.0 style: null result on error
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_parse_error_helper() {
        let resp = MCPResponse::parse_error();
        assert!(resp.is_v1()); // Defaults to 1.0 style
        assert!(resp.is_error());
        assert_eq!(resp.result, Some(Value::Null));
        assert!(resp.error.is_some());
        if let Some(error) = &resp.error {
            assert_eq!(error.code, -32700);
        }
    }

    #[test]
    fn test_too_large_helper() {
        let resp = MCPResponse::too_large();
        assert!(resp.is_v1()); // Defaults to 1.0 style
        assert!(resp.is_error());
        assert_eq!(resp.result, Some(Value::Null));
        assert!(resp.error.is_some());
        if let Some(error) = &resp.error {
            assert_eq!(error.code, -32700);
        }
    }
}
