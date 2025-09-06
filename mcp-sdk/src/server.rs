use async_trait::async_trait;
use tokio_stream::Stream;
use crate::error::MCPError;
use crate::request::MCPRequest;
use crate::response::MCPResponse;
use crate::tools::{
    InitializeResponse, ServerCapabilities, ServerInfo, Tool, ToolResponse,
    Prompt, PromptResponse, Resource, ResourceContent, StreamChunk
};
use serde_json::Value;
use std::pin::Pin;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    // Tool methods
    async fn call_tool(&self, name: &str, args: &Value) -> Result<ToolResponse, MCPError>;

    // Prompt methods
    async fn list_prompts(&self) -> Result<Vec<Prompt>, MCPError> {
        Ok(vec![]) // Default: no prompts
    }

    async fn get_prompt(&self, name: &str, args: &Value) -> Result<PromptResponse, MCPError> {
        let _ = (name, args);
        Err(MCPError::UnknownPrompt(name.into()))
    }

    // Resource methods
    async fn list_resources(&self) -> Result<Vec<Resource>, MCPError> {
        Ok(vec![]) // Default: no resources
    }

    async fn read_resource(&self, uri: &str) -> Result<ResourceContent, MCPError> {
        Err(MCPError::ResourceNotFound(uri.into()))
    }

    // Streaming method for long-running operations using tokio streams
    async fn call_tool_stream(&self, name: &str, args: &Value) -> Result<Pin<Box<dyn Stream<Item = StreamChunk> + Send>>, MCPError> {
        let _ = (name, args);
        Err(MCPError::StreamError("Streaming not supported".into()))
    }

    // Observability hooks
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

    pub fn with_prompts(mut self, prompts: Vec<Prompt>) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            "prompts".into(),
            Value::Array(prompts.into_iter().map(|p| serde_json::to_value(p).unwrap()).collect()),
        );
        self.capabilities.prompts = map;
        self
    }

    pub fn with_resources(mut self, resources: Vec<Resource>) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            "resources".into(),
            Value::Array(resources.into_iter().map(|r| serde_json::to_value(r).unwrap()).collect()),
        );
        self.capabilities.resources = map;
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

    fn detect_version(&self, req: &MCPRequest) -> JsonRpcVersion {
        match req.jsonrpc.as_deref() {
            Some("2.0") => JsonRpcVersion::V2_0,
            Some("1.0") | None => JsonRpcVersion::V1_0,
            Some(_) => JsonRpcVersion::V2_0,
        }
    }

    fn list_tools(&self) -> Value {
        Value::Object(self.capabilities.tools.clone())
    }

    fn list_prompts(&self) -> Value {
        Value::Object(self.capabilities.prompts.clone())
    }

    fn list_resources(&self) -> Value {
        Value::Object(self.capabilities.resources.clone())
    }

    pub async fn handle(&self, req: MCPRequest) -> Option<MCPResponse> {
        let version = self.detect_version(&req);

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
                        version: "0.3.0".into(), // Updated version
                    },
                }).map_err(MCPError::from)
            }

            "tools/list" => Ok(self.list_tools()),
            "tools/call" => self.handle_tool_call(&req).await,

            "prompts/list" => Ok(self.list_prompts()),
            "prompts/get" => self.handle_prompt_get(&req).await,

            "resources/list" => Ok(self.list_resources()),
            "resources/read" => self.handle_resource_read(&req).await,

            other => Err(MCPError::MethodNotFound(other.into())),
        };

        Some(self.build_response(version, req.id.clone(), result))
    }

    fn build_response(&self, version: JsonRpcVersion, id: Option<Value>, result: Result<Value, MCPError>) -> MCPResponse {
        match version {
            JsonRpcVersion::V1_0 => {
                match result {
                    Ok(res) => MCPResponse {
                        jsonrpc: None,
                        id,
                        result: Some(res),
                        error: None,
                    },
                    Err(err) => MCPResponse {
                        jsonrpc: None,
                        id,
                        result: Some(Value::Null),
                        error: Some(err.to_json_rpc_error()),
                    },
                }
            }
            JsonRpcVersion::V2_0 => {
                match result {
                    Ok(res) => MCPResponse {
                        jsonrpc: Some("2.0".into()),
                        id,
                        result: Some(res),
                        error: None,
                    },
                    Err(err) => MCPResponse {
                        jsonrpc: Some("2.0".into()),
                        id,
                        result: None,
                        error: Some(err.to_json_rpc_error()),
                    },
                }
            }
        }
    }

    async fn handle_tool_call(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        match (req.params.as_ref(), req.params.as_ref().and_then(|p| p.get("name")).and_then(Value::as_str)) {
            (Some(params), Some(name)) => {
                let args = params.get("arguments").unwrap_or(&Value::Null);

                self.handler.on_tool_called(name).await;

                let result = self.handler.call_tool(name, args).await;
                let success = result.is_ok();
                self.handler.on_tool_completed(name, success).await;

                match result {
                    Ok(tool_response) => serde_json::to_value(tool_response).map_err(MCPError::from),
                    Err(e) => Err(e),
                }
            }
            (None, _) => Err(MCPError::MissingParameters),
            (_, None) => Err(MCPError::MissingToolName),
        }
    }

    async fn handle_prompt_get(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or(MCPError::MissingParameters)?;
        let name = params.get("name").and_then(Value::as_str).ok_or(MCPError::MissingParameters)?;
        let args = params.get("arguments").unwrap_or(&Value::Null);

        let response = self.handler.get_prompt(name, args).await?;
        serde_json::to_value(response).map_err(MCPError::from)
    }

    async fn handle_resource_read(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or(MCPError::MissingParameters)?;
        let uri = params.get("uri").and_then(Value::as_str).ok_or(MCPError::MissingParameters)?;

        let content = self.handler.read_resource(uri).await?;
        serde_json::to_value(content).map_err(MCPError::from)
    }
}
