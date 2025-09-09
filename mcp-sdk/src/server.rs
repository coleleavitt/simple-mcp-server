use crate::error::MCPError;
use crate::request::MCPRequest;
use crate::response::MCPResponse;
use crate::notifications::{ServerNotification, ProgressSender};
use crate::tools::{
    InitializeResponse, Prompt, PromptResponse, Resource, ResourceContent,
    ServerCapabilities, ServerInfo, StreamChunk, Tool, ToolResponse
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio_stream::Stream;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    // Tool methods
    async fn call_tool(&self, name: &str, args: &Value, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError>;

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

    // Cancellation hook
    async fn on_request_cancelled(&self, request_id: &str, reason: Option<&str>) {
        eprintln!("[CANCEL] Request {} cancelled: {:?}", request_id, reason);
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
        let (notification_tx, notification_rx) = mpsc::unbounded_channel();
        SystemMCPServer {
            handler,
            capabilities: self.capabilities,
            active_requests: Arc::new(RwLock::new(HashMap::new())),
            notification_tx,
            notification_rx: Some(notification_rx),
        }
    }
}

pub struct SystemMCPServer<H: ToolHandler> {
    handler: H,
    capabilities: ServerCapabilities,
    // Track in-progress requests for cancellation
    active_requests: Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    // Notification channel for progress updates
    notification_tx: mpsc::UnboundedSender<ServerNotification>,
    notification_rx: Option<mpsc::UnboundedReceiver<ServerNotification>>,
}

impl<H: ToolHandler> SystemMCPServer<H> {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    pub fn take_notification_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<ServerNotification>> {
        self.notification_rx.take()
    }

    fn validate_and_detect_version(&self, req: &MCPRequest) -> Result<JsonRpcVersion, MCPError> {
        #[cfg(all(feature = "schema-draft", not(feature = "schema-june-2025")))]
        {
            // Strict mode: only allow 2.0, and require id for requests
            if req.jsonrpc_version() != Some("2.0") {
                return Err(MCPError::InvalidJsonRpcVersion(
                    req.jsonrpc_version().unwrap_or("missing").to_string()
                ));
            }
            // In draft schema, all messages must have id (even notifications in some cases)
            // But we still allow None for notifications for practical compatibility
            Ok(JsonRpcVersion::V2_0)
        }
        #[cfg(feature = "schema-june-2025")]
        {
            // Flexible mode: support both 1.0 and 2.0
            match req.jsonrpc_version() {
                Some("2.0") => Ok(JsonRpcVersion::V2_0),
                Some("1.0") | None => Ok(JsonRpcVersion::V1_0),
                Some(other) => Err(MCPError::InvalidJsonRpcVersion(other.to_string())),
            }
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
        // Validate and detect JSON-RPC version
        let version = match self.validate_and_detect_version(&req) {
            Ok(version) => version,
            Err(err) => {
                return Some(self.create_error_response(JsonRpcVersion::V2_0, req.id.clone(), err));
            }
        };

        // Handle notifications (no response)
        if req.is_notification() {
            return match req.method.as_str() {
                "notifications/cancelled" => {
                    self.handle_cancellation(&req).await;
                    None
                }
                "notifications/ping" => {
                    eprintln!("[PING] Received ping from client");
                    None
                }
                _ => None,
            }
        }

        let result: Result<Value, MCPError> = match req.method.as_str() {
            "initialize" => {
                serde_json::to_value(InitializeResponse {
                    protocol_version: "2024-11-05".into(),
                    capabilities: self.capabilities.clone(),
                    server_info: ServerInfo {
                        name: "secure-system-mcp".into(),
                        version: env!("CARGO_PKG_VERSION").into(),
                    },
                }).map_err(MCPError::from)
            }
            "tools/list" => Ok(self.list_tools()),
            "tools/call" => self.handle_tool_call_with_cancellation(&req).await,
            "prompts/list" => Ok(self.list_prompts()),
            "prompts/get" => self.handle_prompt_get(&req).await,
            "resources/list" => Ok(self.list_resources()),
            "resources/read" => self.handle_resource_read(&req).await,
            other => Err(MCPError::MethodNotFound(other.into())),
        };

        match result {
            Ok(res) => Some(self.create_success_response(version, req.id.clone(), res)),
            Err(err) => Some(self.create_error_response(version, req.id.clone(), err)),
        }
    }

    fn create_success_response(&self, version: JsonRpcVersion, id: Option<Value>, result: Value) -> MCPResponse {
        match version {
            JsonRpcVersion::V1_0 => {
                #[cfg(feature = "jsonrpc-1")]
                {
                    MCPResponse::v1_success(id, result)
                }
                #[cfg(not(feature = "jsonrpc-1"))]
                {
                    MCPResponse::success(id, result)
                }
            }
            JsonRpcVersion::V2_0 => {
                #[cfg(feature = "jsonrpc-2")]
                {
                    MCPResponse::v2_success(id, result)
                }
                #[cfg(not(feature = "jsonrpc-2"))]
                {
                    MCPResponse::success(id, result)
                }
            }
        }
    }

    fn create_error_response(&self, version: JsonRpcVersion, id: Option<Value>, error: MCPError) -> MCPResponse {
        let json_rpc_error = error.to_json_rpc_error();
        match version {
            JsonRpcVersion::V1_0 => {
                #[cfg(feature = "jsonrpc-1")]
                {
                    MCPResponse::v1_error(id, json_rpc_error)
                }
                #[cfg(not(feature = "jsonrpc-1"))]
                {
                    MCPResponse::error(id, json_rpc_error)
                }
            }
            JsonRpcVersion::V2_0 => {
                #[cfg(feature = "jsonrpc-2")]
                {
                    MCPResponse::v2_error(id, json_rpc_error)
                }
                #[cfg(not(feature = "jsonrpc-2"))]
                {
                    MCPResponse::error(id, json_rpc_error)
                }
            }
        }
    }


    async fn handle_cancellation(&self, req: &MCPRequest) {
        if let Some(params) = &req.params {
            if let Some(request_id) = params.get("requestId").and_then(Value::as_str) {
                let reason = params.get("reason").and_then(Value::as_str);

                // Signal cancellation to active request
                {
                    let mut active = self.active_requests.write().await;
                    if let Some(cancel_tx) = active.remove(request_id) {
                        let _ = cancel_tx.send(());
                        eprintln!("[CANCEL] Request {} cancelled: {:?}", request_id, reason);

                        // Notify handler
                        self.handler.on_request_cancelled(request_id, reason).await;
                    }
                }
            }
        }
    }

    async fn handle_tool_call_with_cancellation(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let request_id = req.id.as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();

        // Register cancellation handler
        {
            let mut active = self.active_requests.write().await;
            active.insert(request_id.clone(), cancel_tx);
        }

        // Create progress sender for this request
        let progress_sender = ProgressSender::new(self.notification_tx.clone());

        // Execute with cancellation support
        let result = tokio::select! {
            result = self.handle_tool_call(req, progress_sender) => {
                result
            }
            _ = cancel_rx => {
                eprintln!("[CANCEL] Tool call {} was cancelled", request_id);
                Err(MCPError::RequestCancelled(request_id.clone()))
            }
        };

        // Clean up
        {
            let mut active = self.active_requests.write().await;
            active.remove(&request_id);
        }

        result
    }

    async fn handle_tool_call(&self, req: &MCPRequest, progress_sender: ProgressSender) -> Result<Value, MCPError> {
        match (req.params.as_ref(), req.params.as_ref().and_then(|p| p.get("name")).and_then(Value::as_str)) {
            (Some(params), Some(name)) => {
                let args = params.get("arguments").unwrap_or(&Value::Null);

                self.handler.on_tool_called(name).await;
                let result = self.handler.call_tool(name, args, progress_sender).await;
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
