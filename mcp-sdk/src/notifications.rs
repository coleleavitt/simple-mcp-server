// mcp-sdk/src/notifications.rs

#![allow(missing_docs)]

use crate::request::ProgressToken;
use serde::Serialize;
use tokio::sync::mpsc;

/// Represents a notification that the server can send to the client.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "method", content = "params")]
pub enum ServerNotification {
    #[serde(rename = "notifications/progress")]
    Progress {
        progress_token: ProgressToken,
        progress: f64,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total: Option<u64>,
    },
    #[serde(rename = "notifications/resources/updated")]
    ResourceUpdated { uri: String },
}

/// Provides a way for tool handlers to send progress updates.
#[derive(Debug, Clone)]
pub struct ProgressSender {
    token: Option<ProgressToken>,
    sender: mpsc::UnboundedSender<ServerNotification>,
}

impl ProgressSender {
    /// Creates a new progress sender.
    pub fn new(token: Option<ProgressToken>, sender: mpsc::UnboundedSender<ServerNotification>) -> Self {
        Self { token, sender }
    }

    /// Sends a progress update. Does nothing if no progress token was provided by the client.
    pub fn send(&self, progress: f64, message: Option<String>) {
        if let Some(token) = &self.token {
            let notification = ServerNotification::Progress {
                progress_token: token.clone(),
                progress,
                message,
                total: None,
            };
            let _ = self.sender.send(notification);
        }
    }
}