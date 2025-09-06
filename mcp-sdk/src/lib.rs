pub mod error;
pub mod request;
pub mod response;
pub mod server;
pub mod tools;
pub mod notifications;

pub use error::MCPError;
pub use request::MCPRequest;
pub use response::MCPResponse;
pub use server::{
    JsonRpcVersion, ServerBuilder, SystemMCPServer, ToolHandler,
};
pub use tools::{
    CancellationNotification, CancellationNotificationMessage, CancellationParams,
    InitializeResponse, ProgressNotification, ProgressNotificationMessage, ProgressParams, Prompt,
    PromptArgument, PromptContent, PromptMessage, PromptResponse, Resource, ResourceContent,
    ServerCapabilities, ServerInfo, StreamChunk, Tool, ToolContent, ToolInputSchema, ToolProperty,
    ToolResponse,
};
pub use notifications::{ServerNotification, ProgressSender}; // ← NEW EXPORTS