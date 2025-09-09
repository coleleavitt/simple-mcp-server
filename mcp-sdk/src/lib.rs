// mcp-sdk/src/lib.rs

// FIX: Allow missing docs for now to silence the 130 warnings.
// You can remove this line later and add documentation comments (`///`).
#![allow(missing_docs)]

//! A Software Development Kit (SDK) for building servers that implement the
//! Model Context Protocol (MCP).

pub mod error;
pub mod macros;
pub mod notifications;
pub mod prelude;
pub mod request;
pub mod response;
pub mod server;
pub mod tools;

pub use error::MCPError;
pub use notifications::{ProgressSender, ServerNotification};
pub use request::MCPRequest;
pub use response::MCPResponse;
pub use server::{ServerBuilder, SystemMCPServer, ToolHandler};
pub use tools::{
    Annotations, AudioContent, BlobResourceContents, CallToolResult, ContentBlock,
    EmbeddedResource, ImageContent, Implementation, InitializeResponse, Prompt, PromptArgument,
    ReadResourceResult, Resource, ResourceContents, ResourceLink, ServerCapabilities, TextContent,
    TextResourceContents, Tool, ToolAnnotations, ToolInputSchema,
};
