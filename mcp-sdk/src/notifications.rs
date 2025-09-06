use tokio::sync::mpsc;

/// Notification types for multiplexed output
#[derive(Debug, Clone)]
pub enum ServerNotification {
    Progress {
        request_id: String,
        progress: f64,
        message: Option<String>,
    },
}

/// Progress sender for handlers to use
#[derive(Debug, Clone)]
pub struct ProgressSender {
    sender: mpsc::UnboundedSender<ServerNotification>,
}

impl ProgressSender {
    /// Create a new progress sender from an unbounded channel sender
    pub fn new(sender: mpsc::UnboundedSender<ServerNotification>) -> Self {
        Self { sender }
    }

    /// Send a progress notification
    pub async fn send_progress(&self, request_id: &str, progress: f64, message: Option<String>) {
        let notification = ServerNotification::Progress {
            request_id: request_id.to_string(),
            progress,
            message,
        };
        let _ = self.sender.send(notification);
    }
}
