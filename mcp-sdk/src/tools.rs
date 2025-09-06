use serde::{Serialize};
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

/// Schema for a single tool’s inputs
#[derive(Debug, Serialize, Clone)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    pub properties: HashMap<String, ToolProperty>,
    pub required: Vec<String>,
}

/// One property in a tool’s input schema
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

/// One tool’s metadata
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
