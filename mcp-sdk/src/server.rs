use async_trait::async_trait;
use crate::error::MCPError;
use crate::request::MCPRequest;
use crate::response::MCPResponse;
use crate::tools::{InitializeResponse, ServerCapabilities, ServerInfo, Tool, ToolResponse};
use serde_json::Value;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn call_tool(&self, name: &str, args: &Value) -> Result<ToolResponse, MCPError>;

    async fn on_tool_called(&self, name: &str) {
        let _ = name;
    }

    async fn on_tool_completed(&self, name: &str, success: bool) {
        let _ = (name, success);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JsonRpcVersion {
    V1_0,
    V2_0,
}

pub struct ServerBuilder {
    capabilities: ServerCapabilities,
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            capabilities: ServerCapabilities {
                tools: Default::default(),
                prompts: Default::default(),
                resources: Default::default(),
            },
        }
    }

    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            "tools".into(),
            Value::Array(tools.into_iter().map(|t| serde_json::to_value(t).unwrap()).collect()),
        );
        self.capabilities.tools = map;
        self
    }

    pub fn build<H: ToolHandler>(self, handler: H) -> SystemMCPServer<H> {
        SystemMCPServer {
            handler,
            capabilities: self.capabilities,
        }
    }
}

pub struct SystemMCPServer<H: ToolHandler> {
    handler: H,
    capabilities: ServerCapabilities,
}

impl<H: ToolHandler> SystemMCPServer<H> {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    /// Detect JSON-RPC version from request
    fn detect_version(&self, req: &MCPRequest) -> JsonRpcVersion {
        match req.jsonrpc.as_deref() {
            Some("2.0") => JsonRpcVersion::V2_0,
            Some("1.0") | None => JsonRpcVersion::V1_0, // 1.0 or legacy
            Some(_) => JsonRpcVersion::V2_0, // Default to 2.0 for unknown versions
        }
    }

    fn list_tools(&self) -> Value {
        Value::Object(self.capabilities.tools.clone())
    }

    /// Handle MCP request with full JSON-RPC 1.0/2.0 support
    pub async fn handle(&self, req: MCPRequest) -> Option<MCPResponse> {
        let version = self.detect_version(&req);

        // Ignore notifications (requests without id)
        if req.id.is_none() {
            return None;
        }

        let result: Result<Value, MCPError> = match req.method.as_str() {
            "initialize" => {
                serde_json::to_value(InitializeResponse {
                    protocol_version: "2024-11-05".into(),
                    capabilities: self.capabilities.clone(),
                    server_info: ServerInfo {
                        name: "secure-system-mcp".into(),
                        version: "0.2.1".into(),
                    },
                }).map_err(MCPError::from)
            }

            "tools/list" => Ok(self.list_tools()),

            "tools/call" => {
                self.handle_tool_call(&req).await
            }

            "prompts/list" => Ok(serde_json::json!({"prompts": []})),
            "resources/list" => Ok(serde_json::json!({"resources": []})),

            other => Err(MCPError::MethodNotFound(other.into())),
        };

        Some(self.build_response(version, req.id.clone(), result))
    }

    /// Build response according to JSON-RPC version
    fn build_response(&self, version: JsonRpcVersion, id: Option<Value>, result: Result<Value, MCPError>) -> MCPResponse {
        match version {
            JsonRpcVersion::V1_0 => {
                // JSON-RPC 1.0: Always include both result and error fields
                match result {
                    Ok(res) => MCPResponse {
                        jsonrpc: None, // 1.0 doesn't have version field
                        id,
                        result: Some(res),
                        error: None, // 1.0 format: null error on success
                    },
                    Err(err) => MCPResponse {
                        jsonrpc: None,
                        id,
                        result: Some(Value::Null), // 1.0 format: null result on error
                        error: Some(err.to_json_rpc_error()),
                    },
                }
            }
            JsonRpcVersion::V2_0 => {
                // JSON-RPC 2.0: Only result OR error, never both
                match result {
                    Ok(res) => MCPResponse {
                        jsonrpc: Some("2.0".into()),
                        id,
                        result: Some(res),
                        error: None, // 2.0: omit error field on success
                    },
                    Err(err) => MCPResponse {
                        jsonrpc: Some("2.0".into()),
                        id,
                        result: None, // 2.0: omit result field on error
                        error: Some(err.to_json_rpc_error()),
                    },
                }
            }
        }
    }

    /// Handle tool call with observability hooks
    async fn handle_tool_call(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        match (req.params.as_ref(), req.params.as_ref().and_then(|p| p.get("name")).and_then(Value::as_str)) {
            (Some(params), Some(name)) => {
                let args = params.get("arguments").unwrap_or(&Value::Null);

                // Call observability hook
                self.handler.on_tool_called(name).await;

                // Execute tool
                let result = self.handler.call_tool(name, args).await;

                // Call completion hook
                let success = result.is_ok();
                self.handler.on_tool_completed(name, success).await;

                // Return result
                match result {
                    Ok(tool_response) => serde_json::to_value(tool_response).map_err(MCPError::from),
                    Err(e) => Err(e),
                }
            }
            (None, _) => Err(MCPError::MissingParameters),
            (_, None) => Err(MCPError::MissingToolName),
        }
    }
}
