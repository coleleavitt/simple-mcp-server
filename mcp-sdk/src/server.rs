// mcp-sdk/src/server.rs

//! The core MCP server logic, including the ToolHandler trait and the SystemMCPServer implementation.

use crate::error::MCPError;
use crate::notifications::{ProgressSender, ServerNotification};
use crate::request::MCPRequest;
use crate::response::MCPResponse;
use crate::tools::{
    CallToolResult, Implementation, InitializeResponse, Prompt, ReadResourceResult, Resource,
    ServerCapabilities, Tool,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, RwLock};

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn initialize(&self, capabilities: ServerCapabilities) -> Result<InitializeResponse, MCPError> {
        Ok(InitializeResponse {
            protocol_version: "2025-06-18".to_string(),
            server_info: Implementation {
                name: "simple-mcp-server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Simple MCP Server".to_string()),
            },
            capabilities,
        })
    }

    async fn list_tools(&self) -> Result<Vec<Tool>, MCPError> {
        Ok(vec![])
    }

    async fn call_tool(
        &self,
        name: &str,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError>;

    async fn list_resources(&self) -> Result<Vec<Resource>, MCPError> {
        Ok(vec![])
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, MCPError>;

    async fn list_prompts(&self) -> Result<Vec<Prompt>, MCPError> {
        Ok(vec![])
    }

    async fn on_request_cancelled(&self, request_id: &str, reason: Option<&str>) {
        eprintln!("[CANCEL] Request {} cancelled: {:?}", request_id, reason);
    }
}

pub struct ServerBuilder {
    capabilities: ServerCapabilities,
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBuilder {
    pub fn new() -> Self {
        ServerBuilder {
            capabilities: ServerCapabilities::default(),
        }
    }

    #[must_use]
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            "tools".into(),
            serde_json::to_value(tools).unwrap_or_default(),
        );
        self.capabilities.tools = Some(map);
        self
    }

    pub fn build<H: ToolHandler>(self, handler: H) -> SystemMCPServer<H> {
        let (notification_tx, notification_rx) = mpsc::unbounded_channel();
        SystemMCPServer {
            handler: Arc::new(handler),
            capabilities: self.capabilities,
            active_requests: Arc::new(RwLock::new(HashMap::new())),
            notification_tx,
            notification_rx: Some(notification_rx),
        }
    }
}

pub struct SystemMCPServer<H: ToolHandler> {
    handler: Arc<H>,
    capabilities: ServerCapabilities,
    active_requests: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,
    notification_tx: mpsc::UnboundedSender<ServerNotification>,
    notification_rx: Option<mpsc::UnboundedReceiver<ServerNotification>>,
}

#[derive(Debug, Clone, Copy)]
pub enum JsonRpcVersion { V1_0, V2_0 }

impl<H: ToolHandler> SystemMCPServer<H> {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::default()
    }

    pub fn take_notification_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<ServerNotification>> {
        self.notification_rx.take()
    }

    fn validate_and_detect_version(&self, req: &MCPRequest) -> Result<JsonRpcVersion, MCPError> {
        match req.jsonrpc_version() {
            Some("2.0") => Ok(JsonRpcVersion::V2_0),
            Some("1.0") | None => Ok(JsonRpcVersion::V1_0),
            Some(other) => Err(MCPError::InvalidJsonRpcVersion(other.to_string())),
        }
    }

    pub async fn handle(&self, req: MCPRequest) -> Option<MCPResponse> {
        // FIX: Prefix unused variable with an underscore to silence the warning.
        let _version = self.validate_and_detect_version(&req).unwrap_or(JsonRpcVersion::V2_0);

        if req.is_notification() {
            if req.method == "notifications/cancelled" {
                self.handle_cancellation(&req).await;
            }
            return None;
        }

        let request_id = req.id.clone();
        let result: Result<Value, MCPError> = match req.method.as_str() {
            "initialize" => self.handler.initialize(self.capabilities.clone()).await
                .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from)),

            "tools/list" => self.handler.list_tools().await
                .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from)),

            "tools/call" => self.handle_tool_call_with_cancellation(&req).await,

            "resources/list" => self.handler.list_resources().await
                .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from)),

            "resources/read" => self.handle_resource_read(&req).await,

            "prompts/list" => self.handler.list_prompts().await
                .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from)),

            other => Err(MCPError::MethodNotFound(other.into())),
        };

        match result {
            Ok(res) => Some(MCPResponse::success(request_id, res)),
            Err(err) => Some(MCPResponse::error(request_id, err.to_json_rpc_error())),
        }
    }

    async fn handle_cancellation(&self, req: &MCPRequest) {
        if let Some(params) = &req.params {
            if let Some(request_id) = params.get("requestId").and_then(Value::as_str) {
                let reason = params.get("reason").and_then(Value::as_str);
                if let Some(cancel_tx) = self.active_requests.write().await.remove(request_id) {
                    let _ = cancel_tx.send(());
                    self.handler.on_request_cancelled(request_id, reason).await;
                }
            }
        }
    }

    async fn handle_tool_call_with_cancellation(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let request_id = req.id.as_ref().map(|id| id.to_string()).unwrap_or_else(|| "unknown".to_string());
        let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
        self.active_requests.write().await.insert(request_id.clone(), cancel_tx);
        let progress_sender = ProgressSender::new(self.notification_tx.clone());

        let result = tokio::select! {
            result = self.handle_tool_call(req, progress_sender) => {
                result
            }
            _ = cancel_rx => {
                Err(MCPError::RequestCancelled(request_id.clone()))
            }
        };
        self.active_requests.write().await.remove(&request_id);
        result
    }

    async fn handle_tool_call(&self, req: &MCPRequest, progress_sender: ProgressSender) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or_else(|| MCPError::MissingParameters("Missing 'params' for tools/call".to_string()))?;
        let name = params.get("name").and_then(Value::as_str).ok_or(MCPError::MissingToolName)?;
        let args = params.get("arguments").unwrap_or(&Value::Null);
        self.handler.call_tool(name, args, progress_sender).await
            .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
    }

    async fn handle_resource_read(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or_else(|| MCPError::MissingParameters("Missing 'params' for resources/read".to_string()))?;
        let uri = params.get("uri").and_then(Value::as_str).ok_or_else(|| MCPError::MissingParameters("Missing 'uri' for resources/read".to_string()))?;
        self.handler.read_resource(uri).await
            .and_then(|content| serde_json::to_value(content).map_err(MCPError::from))
    }
}