// mcp-sdk/src/server.rs

#![allow(missing_docs)]

use crate::error::MCPError;
use crate::notifications::{ProgressSender, ServerNotification};
use crate::request::MCPRequest;
use crate::response::MCPResponse;
use crate::tools::{
    CallToolResult, CompleteResult, EmptyResult, GetPromptResult, InitializeResponse,
    ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
    ReadResourceResult, ServerCapabilities, Tool,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio_stream::{Stream, StreamExt};

#[async_trait]
pub trait ToolHandler: Send + Sync {
    async fn initialize(
        &self,
        capabilities: ServerCapabilities,
    ) -> Result<InitializeResponse, MCPError>;
    async fn list_tools(&self, cursor: Option<String>) -> Result<ListToolsResult, MCPError>;
    async fn call_tool(
        &self,
        name: &str,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError>;
    async fn list_resources(&self, cursor: Option<String>)
    -> Result<ListResourcesResult, MCPError>;
    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, MCPError>;
    async fn list_prompts(&self, cursor: Option<String>) -> Result<ListPromptsResult, MCPError>;
    async fn get_prompt(&self, name: &str, args: &Value) -> Result<GetPromptResult, MCPError>;
    async fn ping(&self) -> Result<EmptyResult, MCPError>;
    async fn list_resource_templates(
        &self,
        cursor: Option<String>,
    ) -> Result<ListResourceTemplatesResult, MCPError>;
    async fn subscribe(&self, uri: &str) -> Result<EmptyResult, MCPError>;
    async fn unsubscribe(&self, uri: &str) -> Result<EmptyResult, MCPError>;
    async fn set_log_level(&self, level: &str) -> Result<EmptyResult, MCPError>;
    async fn complete(&self, params: &Value) -> Result<CompleteResult, MCPError>;
    async fn on_request_cancelled(&self, request_id: &str, reason: Option<&str>);
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
            subscriptions: Arc::new(RwLock::new(HashSet::new())),
        }
    }
}

type SubscriptionManager = Arc<RwLock<HashSet<String>>>;

pub struct SystemMCPServer<H: ToolHandler> {
    handler: Arc<H>,
    capabilities: ServerCapabilities,
    active_requests: Arc<RwLock<HashMap<String, oneshot::Sender<()>>>>,
    notification_tx: mpsc::UnboundedSender<ServerNotification>,
    notification_rx: Option<mpsc::UnboundedReceiver<ServerNotification>>,
    subscriptions: SubscriptionManager,
}

/// Wrapper to make the notification receiver a named Stream type.
pub struct NotificationStream {
    inner: mpsc::UnboundedReceiver<ServerNotification>,
}

impl Stream for NotificationStream {
    type Item = ServerNotification;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl NotificationStream {
    pub fn new(receiver: mpsc::UnboundedReceiver<ServerNotification>) -> Self {
        Self { inner: receiver }
    }

    /// Filter notifications by type using a direct boolean predicate.
    pub fn filter_progress(self) -> impl Stream<Item = ServerNotification> {
        self.filter(|notification| matches!(notification, ServerNotification::Progress { .. }))
    }

    /// Filter notifications by resource updates using a direct boolean predicate.
    pub fn filter_resource_updates(self) -> impl Stream<Item = ServerNotification> {
        self.filter(|notification| {
            matches!(notification, ServerNotification::ResourceUpdated { .. })
        })
    }

    /// Batch notifications with a timeout.
    pub fn batch_with_timeout(
        self,
        max_size: usize,
        timeout: std::time::Duration,
    ) -> impl Stream<Item = Vec<ServerNotification>> {
        self.chunks_timeout(max_size, timeout)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum JsonRpcVersion {
    V1_0,
    V2_0,
}

impl<H: ToolHandler> SystemMCPServer<H> {
    pub fn builder() -> ServerBuilder {
        ServerBuilder::default()
    }

    /// Take the notification receiver as a `NotificationStream`.
    pub fn take_notification_stream(&mut self) -> Option<NotificationStream> {
        self.notification_rx.take().map(NotificationStream::new)
    }

    /// Get a stream of all notifications.
    pub fn notification_stream(&mut self) -> Option<impl Stream<Item = ServerNotification>> {
        self.take_notification_stream()
    }

    /// Get a filtered stream of only progress notifications.
    pub fn progress_stream(&mut self) -> Option<impl Stream<Item = ServerNotification>> {
        self.take_notification_stream()
            .map(|stream| stream.filter_progress())
    }

    /// Get a filtered stream of only resource update notifications.
    pub fn resource_update_stream(&mut self) -> Option<impl Stream<Item = ServerNotification>> {
        self.take_notification_stream()
            .map(|stream| stream.filter_resource_updates())
    }

    fn validate_and_detect_version(&self, req: &MCPRequest) -> Result<JsonRpcVersion, MCPError> {
        match req.jsonrpc_version() {
            Some("2.0") => Ok(JsonRpcVersion::V2_0),
            Some("1.0") | None => Ok(JsonRpcVersion::V1_0),
            Some(other) => Err(MCPError::InvalidJsonRpcVersion(other.to_string())),
        }
    }

    pub async fn handle(&self, req: MCPRequest) -> Option<MCPResponse> {
        let _version = self
            .validate_and_detect_version(&req)
            .unwrap_or(JsonRpcVersion::V2_0);

        if req.is_notification() {
            if req.method == "notifications/cancelled" {
                self.handle_cancellation(&req).await;
            }
            return None;
        }

        let request_id = req.id.clone();

        let result: Result<Value, MCPError> = match req.method.as_str() {
            "initialize" => {
                async {
                    self.handler
                        .initialize(self.capabilities.clone())
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "ping" => {
                async {
                    self.handler
                        .ping()
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "tools/list" => {
                async {
                    let params = req.params.as_ref();
                    let cursor = params
                        .and_then(|p| p.get("cursor"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    self.handler
                        .list_tools(cursor)
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "tools/call" => self.handle_tool_call_with_cancellation(&req).await,
            "resources/list" => {
                async {
                    let params = req.params.as_ref();
                    let cursor = params
                        .and_then(|p| p.get("cursor"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    self.handler
                        .list_resources(cursor)
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "resources/read" => self.handle_resource_read(&req).await,
            "resources/templates/list" => {
                async {
                    let params = req.params.as_ref();
                    let cursor = params
                        .and_then(|p| p.get("cursor"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    self.handler
                        .list_resource_templates(cursor)
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "resources/subscribe" => {
                async {
                    let params = req
                        .params
                        .as_ref()
                        .ok_or_else(|| MCPError::MissingParameters("params object".into()))?;
                    let uri = params
                        .get("uri")
                        .and_then(Value::as_str)
                        .ok_or_else(|| MCPError::MissingParameters("uri".into()))?;
                    let res = self.handler.subscribe(uri).await?;
                    self.subscriptions.write().await.insert(uri.to_string());
                    serde_json::to_value(res).map_err(MCPError::from)
                }
                .await
            }
            "resources/unsubscribe" => {
                async {
                    let params = req
                        .params
                        .as_ref()
                        .ok_or_else(|| MCPError::MissingParameters("params object".into()))?;
                    let uri = params
                        .get("uri")
                        .and_then(Value::as_str)
                        .ok_or_else(|| MCPError::MissingParameters("uri".into()))?;
                    let res = self.handler.unsubscribe(uri).await?;
                    self.subscriptions.write().await.remove(uri);
                    serde_json::to_value(res).map_err(MCPError::from)
                }
                .await
            }
            "prompts/list" => {
                async {
                    let params = req.params.as_ref();
                    let cursor = params
                        .and_then(|p| p.get("cursor"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    self.handler
                        .list_prompts(cursor)
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "prompts/get" => self.handle_prompt_get(&req).await,
            "logging/setLevel" => {
                async {
                    let params = req
                        .params
                        .as_ref()
                        .ok_or_else(|| MCPError::MissingParameters("params object".into()))?;
                    let level = params
                        .get("level")
                        .and_then(Value::as_str)
                        .ok_or_else(|| MCPError::MissingParameters("level".into()))?;
                    self.handler
                        .set_log_level(level)
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
            "completion/complete" => {
                async {
                    let params = req
                        .params
                        .as_ref()
                        .ok_or_else(|| MCPError::MissingParameters("params object".into()))?;
                    self.handler
                        .complete(params)
                        .await
                        .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
                }
                .await
            }
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

    async fn handle_tool_call_with_cancellation(
        &self,
        req: &MCPRequest,
    ) -> Result<Value, MCPError> {
        let request_id = req
            .id
            .as_ref()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let (cancel_tx, cancel_rx) = oneshot::channel();
        self.active_requests
            .write()
            .await
            .insert(request_id.clone(), cancel_tx);
        let progress_token = req.meta.as_ref().and_then(|m| m.progress_token.clone());
        let progress_sender = ProgressSender::new(progress_token, self.notification_tx.clone());

        let result = tokio::select! {
            result = self.handle_tool_call(req, progress_sender) => result,
            _ = cancel_rx => Err(MCPError::RequestCancelled(request_id.clone())),
        };
        self.active_requests.write().await.remove(&request_id);
        result
    }

    async fn handle_tool_call(
        &self,
        req: &MCPRequest,
        progress_sender: ProgressSender,
    ) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or_else(|| {
            MCPError::MissingParameters("Missing 'params' for tools/call".to_string())
        })?;
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or(MCPError::MissingToolName)?;
        let args = params.get("arguments").unwrap_or(&Value::Null);
        self.handler
            .call_tool(name, args, progress_sender)
            .await
            .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
    }

    async fn handle_resource_read(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or_else(|| {
            MCPError::MissingParameters("Missing 'params' for resources/read".to_string())
        })?;
        let uri = params.get("uri").and_then(Value::as_str).ok_or_else(|| {
            MCPError::MissingParameters("Missing 'uri' for resources/read".to_string())
        })?;
        self.handler
            .read_resource(uri)
            .await
            .and_then(|content| serde_json::to_value(content).map_err(MCPError::from))
    }

    async fn handle_prompt_get(&self, req: &MCPRequest) -> Result<Value, MCPError> {
        let params = req.params.as_ref().ok_or_else(|| {
            MCPError::MissingParameters("Missing 'params' for prompts/get".to_string())
        })?;
        let name = params.get("name").and_then(Value::as_str).ok_or_else(|| {
            MCPError::MissingParameters("Missing 'name' in params for prompts/get".to_string())
        })?;
        let args = params.get("arguments").unwrap_or(&Value::Null);

        self.handler
            .get_prompt(name, args)
            .await
            .and_then(|resp| serde_json::to_value(resp).map_err(MCPError::from))
    }
}
