// src/main.rs

#![allow(missing_docs)]

use async_trait::async_trait;
use mcp_sdk::error::MCPError;
use mcp_sdk::notifications::ProgressSender;
use mcp_sdk::request::MCPRequest;
use mcp_sdk::server::{SystemMCPServer, ToolHandler};
use mcp_sdk::tools::{
    CallToolResult, CompleteResult, CompletionList, ContentBlock, EmptyResult, GetPromptResult,
    Implementation, InitializeResponse, ListPromptsResult, ListResourceTemplatesResult,
    ListResourcesResult, ListToolsResult, Prompt, PromptMessage, ReadResourceResult,
    ServerCapabilities, TextContent, Tool, ToolInputSchema,
};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use std::time::Duration;

struct BashToolHandler;

#[async_trait]
impl ToolHandler for BashToolHandler {
    async fn initialize(&self, mut capabilities: ServerCapabilities) -> Result<InitializeResponse, MCPError> {
        // Announce that this server provides tools, resources, and prompts.
        capabilities.tools = Some(Default::default());
        capabilities.resources = Some(Default::default());
        capabilities.prompts = Some(Default::default());
        capabilities.completions = Some(Default::default());

        Ok(InitializeResponse {
            protocol_version: "2025-06-18".to_string(),
            server_info: Implementation {
                name: "simple-mcp-server".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("Simple Bash Server".to_string()),
            },
            capabilities,
        })
    }

    async fn list_tools(&self, cursor: Option<String>) -> Result<ListToolsResult, MCPError> {
        if cursor.is_some() {
            return Ok(ListToolsResult { tools: vec![], next_cursor: None });
        }
        let bash_tool = Tool {
            name: "bash".to_string(),
            title: Some("Bash Command Executor".to_string()),
            description: Some("Execute bash commands.".to_string()),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert("command".to_string(), serde_json::json!({ "type": "string", "description": "The command to execute" }));
                    props.insert("timeout".to_string(), serde_json::json!({ "type": "number", "description": "Timeout in seconds (default: 30)" }));
                    props
                },
                required: vec!["command".to_string()],
            },
            output_schema: None,
            annotations: None,
        };
        Ok(ListToolsResult {
            tools: vec![bash_tool],
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        name: &str,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError> {
        match name {
            "bash" => self.execute_bash_command(args, progress_sender).await,
            _ => Err(MCPError::UnknownTool(name.to_string())),
        }
    }

    async fn list_resources(&self, _cursor: Option<String>) -> Result<ListResourcesResult, MCPError> {
        Ok(ListResourcesResult { resources: vec![], next_cursor: None })
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, MCPError> {
        Err(MCPError::ResourceNotFound(uri.to_string()))
    }

    async fn list_prompts(&self, _cursor: Option<String>) -> Result<ListPromptsResult, MCPError> {
        let hello_prompt = Prompt {
            name: "hello".to_string(),
            title: Some("Hello World Prompt".to_string()),
            description: Some("A simple prompt that says hello.".to_string()),
            arguments: None,
        };
        Ok(ListPromptsResult {
            prompts: vec![hello_prompt],
            next_cursor: None,
        })
    }

    async fn get_prompt(&self, name: &str, _args: &Value) -> Result<GetPromptResult, MCPError> {
        match name {
            "hello" => Ok(GetPromptResult {
                description: Some("A friendly greeting.".to_string()),
                messages: vec![PromptMessage {
                    role: "user".to_string(),
                    content: ContentBlock::Text(TextContent {
                        text: "Hello, world!".to_string(),
                        annotations: None,
                    }),
                }],
            }),
            _ => Err(MCPError::UnknownPrompt(name.to_string())),
        }
    }

    async fn ping(&self) -> Result<EmptyResult, MCPError> {
        eprintln!("[INFO] Received ping, sending pong.");
        Ok(EmptyResult {})
    }

    async fn list_resource_templates(&self, _cursor: Option<String>) -> Result<ListResourceTemplatesResult, MCPError> {
        Ok(ListResourceTemplatesResult { resource_templates: vec![], next_cursor: None })
    }

    async fn subscribe(&self, uri: &str) -> Result<EmptyResult, MCPError> {
        eprintln!("[INFO] Client subscribed to URI: {}", uri);
        Ok(EmptyResult {})
    }

    async fn unsubscribe(&self, uri: &str) -> Result<EmptyResult, MCPError> {
        eprintln!("[INFO] Client unsubscribed from URI: {}", uri);
        Ok(EmptyResult {})
    }

    async fn set_log_level(&self, level: &str) -> Result<EmptyResult, MCPError> {
        eprintln!("[INFO] Client requested log level: {}", level);
        Ok(EmptyResult {})
    }

    async fn complete(&self, params: &Value) -> Result<CompleteResult, MCPError> {
        eprintln!("[INFO] Received completion request with params: {:?}", params);
        Ok(CompleteResult {
            completion: CompletionList {
                values: vec!["ls -la".to_string(), "echo 'hello'".to_string()],
                has_more: Some(false),
                total: Some(2),
            }
        })
    }

    async fn on_request_cancelled(&self, request_id: &str, reason: Option<&str>) {
        eprintln!("[CANCEL] Request {} cancelled: {:?}", request_id, reason);
    }
}

impl BashToolHandler {
    async fn execute_bash_command(
        &self,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError> {
        let command = args.get("command").and_then(|v| v.as_str()).ok_or_else(|| {
            MCPError::MissingParameters("Missing required 'command' parameter".to_string())
        })?;

        let timeout_seconds = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);

        let mut cmd = Command::new("bash");
        cmd.kill_on_drop(true);
        cmd.arg("-c").arg(command).stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn().map_err(MCPError::IoError)?;

        progress_sender.send(0.1, Some("Command spawned".to_string()));

        let timeout = tokio::time::sleep(Duration::from_secs(timeout_seconds));
        tokio::pin!(timeout);

        tokio::select! {
            biased;
            result = child.wait_with_output() => {
                let output = result.map_err(MCPError::IoError)?;
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();

                let mut response_text = format!("Exit code: {}\n", output.status.code().unwrap_or(-1));
                if !stdout.is_empty() {
                    response_text.push_str("\nSTDOUT:\n");
                    response_text.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    response_text.push_str("\nSTDERR:\n");
                    response_text.push_str(&stderr);
                }

                progress_sender.send(1.0, Some("Command finished".to_string()));

                Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent { text: response_text, annotations: None })],
                    structured_content: None,
                    is_error: !output.status.success(),
                })
            }
            _ = &mut timeout => {
                let error_text = format!("Command timed out after {} seconds", timeout_seconds);
                Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent { text: error_text, annotations: None })],
                    structured_content: None,
                    is_error: true,
                })
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let server = SystemMCPServer::<BashToolHandler>::builder().build(BashToolHandler);

    eprintln!("Bash MCP Server starting...");
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(&mut stdin);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() { continue; }

                match serde_json::from_str::<MCPRequest>(line) {
                    Ok(request) => {
                        if let Some(response) = server.handle(request).await {
                            if let Ok(response_json) = serde_json::to_string(&response) {
                                if stdout.write_all(response_json.as_bytes()).await.is_err() { break; }
                                if stdout.write_all(b"\n").await.is_err() { break; }
                                if stdout.flush().await.is_err() { break; }
                            }
                        }
                    }
                    Err(e) => eprintln!("Failed to parse request: {}", e),
                }
            }
            Err(e) => {
                eprintln!("Failed to read line: {}", e);
                break;
            }
        }
    }
}