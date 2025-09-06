use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// One chunk of tool output
#[derive(Debug, Serialize, Clone)]
pub struct ToolContent {
    #[serde(rename = "type")]
    pub content_type: String,
    pub text: String,
}

/// Full tool response
#[derive(Debug, Serialize, Clone)]
pub struct ToolResponse {
    pub content: Vec<ToolContent>,
    #[serde(rename = "isError")]
    pub is_error: bool,
}

impl ToolResponse {
    pub fn new(text: String, is_error: bool) -> Self {
        ToolResponse {
            content: vec![ToolContent { content_type: "text".into(), text }],
            is_error,
        }
    }
}

/// Prompt definition with parameters
#[derive(Debug, Serialize, Clone)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}

/// Prompt argument definition
#[derive(Debug, Serialize, Clone)]
pub struct PromptArgument {
    pub name: String,
    pub description: String,
    pub required: bool,
}

/// Prompt response with messages
#[derive(Debug, Serialize, Clone)]
pub struct PromptResponse {
    pub description: String,
    pub messages: Vec<PromptMessage>,
}

/// Individual prompt message
#[derive(Debug, Serialize, Clone)]
pub struct PromptMessage {
    pub role: String, // "user", "assistant", "system"
    pub content: PromptContent,
}

/// Prompt message content
#[derive(Debug, Serialize, Clone)]
pub struct PromptContent {
    #[serde(rename = "type")]
    pub content_type: String, // "text"
    pub text: String,
}

/// Resource definition
#[derive(Debug, Serialize, Clone)]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Resource content response
#[derive(Debug, Serialize, Clone)]
pub struct ResourceContent {
    pub uri: String,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub text: String,
}

/// Streaming chunk for long operations
#[derive(Debug, Serialize, Clone)]
pub struct StreamChunk {
    pub chunk_type: String, // "progress", "data", "complete", "error"
    pub data: Value,
}

/// Server capabilities object
#[derive(Debug, Serialize, Clone)]
pub struct ServerCapabilities {
    pub tools: serde_json::Map<String, Value>,
    pub prompts: serde_json::Map<String, Value>,
    pub resources: serde_json::Map<String, Value>,
}

/// Response to initialize()
#[derive(Debug, Serialize, Clone)]
pub struct InitializeResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

/// Static server info
#[derive(Debug, Serialize, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Schema for a single tool's inputs
#[derive(Debug, Serialize, Clone)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: HashMap<String, ToolProperty>,
    pub required: Vec<String>,
}

/// One property in a tool's input schema
#[derive(Debug, Serialize, Clone)]
pub struct ToolProperty {
    #[serde(rename = "type")]
    pub property_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<ToolPropertyItems>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

/// When `ToolProperty` is an array
#[derive(Debug, Serialize, Clone)]
pub struct ToolPropertyItems {
    #[serde(rename = "type")]
    pub item_type: String,
}

/// One tool's metadata
#[derive(Debug, Serialize, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: ToolInputSchema,
}

impl ToolProperty {
    pub fn string(description: impl Into<String>) -> Self {
        ToolProperty {
            property_type: "string".into(),
            description: description.into(),
            items: None,
            default: None,
        }
    }

    pub fn array(description: impl Into<String>, item_type: impl Into<String>) -> Self {
        ToolProperty {
            property_type: "array".into(),
            description: description.into(),
            items: Some(ToolPropertyItems { item_type: item_type.into() }),
            default: None,
        }
    }

    pub fn boolean(description: impl Into<String>, default: bool) -> Self {
        ToolProperty {
            property_type: "boolean".into(),
            description: description.into(),
            items: None,
            default: Some(Value::Bool(default)),
        }
    }
}

impl Prompt {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Prompt {
            name: name.into(),
            description: description.into(),
            arguments: None,
        }
    }

    pub fn with_arguments(mut self, args: Vec<PromptArgument>) -> Self {
        self.arguments = Some(args);
        self
    }
}

impl PromptArgument {
    pub fn new(name: impl Into<String>, description: impl Into<String>, required: bool) -> Self {
        PromptArgument {
            name: name.into(),
            description: description.into(),
            required,
        }
    }
}

impl Resource {
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Resource {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}
