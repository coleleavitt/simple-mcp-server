//! A "prelude" for users of the `mcp-sdk`, containing the most common types.

pub use crate::{
    error::MCPError,
    notifications::ProgressSender,
    request::MCPRequest,
    response::MCPResponse,
    server::ToolHandler,
    tools::{CallToolResult, ContentBlock, ReadResourceResult, Resource, TextContent, Tool},
};
