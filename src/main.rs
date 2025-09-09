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
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

struct BashToolHandler;

#[async_trait]
impl ToolHandler for BashToolHandler {
    async fn initialize(
        &self,
        mut capabilities: ServerCapabilities,
    ) -> Result<InitializeResponse, MCPError> {
        Self::setup_capabilities(&mut capabilities);
        Ok(InitializeResponse {
            protocol_version: "2025-06-18".to_string(),
            server_info: Self::create_server_info(),
            capabilities,
        })
    }

    async fn list_tools(&self, cursor: Option<String>) -> Result<ListToolsResult, MCPError> {
        if cursor.is_some() {
            return Ok(ListToolsResult {
                tools: vec![],
                next_cursor: None,
            });
        }
        Ok(ListToolsResult {
            tools: vec![Self::create_bash_tool()],
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

    async fn list_resources(
        &self,
        _cursor: Option<String>,
    ) -> Result<ListResourcesResult, MCPError> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, MCPError> {
        Err(MCPError::ResourceNotFound(uri.to_string()))
    }

    async fn list_prompts(&self, _cursor: Option<String>) -> Result<ListPromptsResult, MCPError> {
        Ok(ListPromptsResult {
            prompts: vec![Self::create_hello_prompt()],
            next_cursor: None,
        })
    }

    async fn get_prompt(&self, name: &str, _args: &Value) -> Result<GetPromptResult, MCPError> {
        match name {
            "hello" => Ok(Self::create_hello_prompt_result()),
            _ => Err(MCPError::UnknownPrompt(name.to_string())),
        }
    }

    async fn ping(&self) -> Result<EmptyResult, MCPError> {
        Ok(EmptyResult {})
    }

    async fn list_resource_templates(
        &self,
        _cursor: Option<String>,
    ) -> Result<ListResourceTemplatesResult, MCPError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![],
            next_cursor: None,
        })
    }

    async fn subscribe(&self, _uri: &str) -> Result<EmptyResult, MCPError> {
        Ok(EmptyResult {})
    }

    async fn unsubscribe(&self, _uri: &str) -> Result<EmptyResult, MCPError> {
        Ok(EmptyResult {})
    }

    async fn set_log_level(&self, _level: &str) -> Result<EmptyResult, MCPError> {
        Ok(EmptyResult {})
    }

    async fn complete(&self, _params: &Value) -> Result<CompleteResult, MCPError> {
        Ok(CompleteResult {
            completion: CompletionList {
                values: vec!["ls -la".to_string(), "echo 'hello'".to_string()],
                has_more: Some(false),
                total: Some(2),
            },
        })
    }

    async fn on_request_cancelled(&self, _request_id: &str, _reason: Option<&str>) {
        // Silent handling
    }
}

impl BashToolHandler {
    fn setup_capabilities(capabilities: &mut ServerCapabilities) {
        capabilities.tools = Some(Default::default());
        capabilities.resources = Some(Default::default());
        capabilities.prompts = Some(Default::default());
        capabilities.completions = Some(Default::default());
    }

    fn create_server_info() -> Implementation {
        Implementation {
            name: "simple-mcp-server".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            title: Some("Simple Bash Server".to_string()),
        }
    }

    fn create_bash_tool() -> Tool {
        let mut props = HashMap::new();
        props.insert(
            "command".to_string(),
            serde_json::json!({ "type": "string", "description": "The command to execute" }),
        );
        props.insert("timeout".to_string(), serde_json::json!({ "type": "number", "description": "Timeout in seconds (default: 30)" }));

        Tool {
            name: "bash".to_string(),
            title: Some("Bash Command Executor".to_string()),
            description: Some("Execute bash commands.".to_string()),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: props,
                required: vec!["command".to_string()],
            },
            output_schema: None,
            annotations: None,
        }
    }

    fn create_hello_prompt() -> Prompt {
        Prompt {
            name: "hello".to_string(),
            title: Some("Hello World Prompt".to_string()),
            description: Some("A simple prompt that says hello.".to_string()),
            arguments: None,
        }
    }

    fn create_hello_prompt_result() -> GetPromptResult {
        GetPromptResult {
            description: Some("A friendly greeting.".to_string()),
            messages: vec![PromptMessage {
                role: "user".to_string(),
                content: ContentBlock::Text(TextContent {
                    text: "Hello, world!".to_string(),
                    annotations: None,
                }),
            }],
        }
    }

    async fn execute_bash_command(
        &self,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError> {
        let command = Self::extract_command(args)?;
        let timeout_seconds = Self::extract_timeout(args);

        let child = Self::spawn_command(&command)?;
        progress_sender.send(0.1, Some("Command spawned".to_string()));

        let timeout = tokio::time::sleep(Duration::from_secs(timeout_seconds));
        tokio::pin!(timeout);

        tokio::select! {
            biased;
            result = child.wait_with_output() => {
                Self::handle_command_output(result, progress_sender).await
            }
            _ = &mut timeout => {
                Self::handle_timeout(timeout_seconds)
            }
        }
    }

    fn extract_command(args: &Value) -> Result<String, MCPError> {
        args.get("command")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                MCPError::MissingParameters("Missing required 'command' parameter".to_string())
            })
    }

    fn extract_timeout(args: &Value) -> u64 {
        args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30)
    }

    fn spawn_command(command: &str) -> Result<tokio::process::Child, MCPError> {
        let mut cmd = Command::new("bash");
        cmd.kill_on_drop(true);
        cmd.arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.spawn().map_err(MCPError::IoError)
    }

    async fn handle_command_output(
        result: Result<std::process::Output, std::io::Error>,
        progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError> {
        let output = result.map_err(MCPError::IoError)?;
        let response_text = Self::format_output(&output);

        progress_sender.send(1.0, Some("Command finished".to_string()));

        Ok(CallToolResult {
            content: vec![ContentBlock::Text(TextContent {
                text: response_text,
                annotations: None,
            })],
            structured_content: None,
            is_error: !output.status.success(),
        })
    }

    fn handle_timeout(timeout_seconds: u64) -> Result<CallToolResult, MCPError> {
        let error_text = format!("Command timed out after {} seconds", timeout_seconds);
        Ok(CallToolResult {
            content: vec![ContentBlock::Text(TextContent {
                text: error_text,
                annotations: None,
            })],
            structured_content: None,
            is_error: true,
        })
    }

    fn format_output(output: &std::process::Output) -> String {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut response_text = format!("Exit code: {}\n", output.status.code().unwrap_or(-1));

        if !stdout.is_empty() {
            response_text.push_str("\nSTDOUT:\n");
            response_text.push_str(&stdout);
        }

        if !stderr.is_empty() {
            response_text.push_str("\nSTDERR:\n");
            response_text.push_str(&stderr);
        }

        response_text
    }
}

struct McpServer {
    server: SystemMCPServer<BashToolHandler>,
}

impl McpServer {
    fn new() -> Self {
        Self {
            server: SystemMCPServer::<BashToolHandler>::builder().build(BashToolHandler),
        }
    }

    async fn run(&self) {
        let mut stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut reader = BufReader::new(&mut stdin);

        loop {
            if self
                .process_single_request(&mut reader, &mut stdout)
                .await
                .is_err()
            {
                break;
            }
        }
    }

    async fn process_single_request(
        &self,
        reader: &mut BufReader<&mut tokio::io::Stdin>,
        stdout: &mut tokio::io::Stdout,
    ) -> Result<(), ()> {
        let line = Self::read_line(reader).await?;
        let request = Self::parse_request(&line)?;
        self.handle_and_respond(request, stdout).await
    }

    // FIXED: Replaced recursion with a loop
    async fn read_line(reader: &mut BufReader<&mut tokio::io::Stdin>) -> Result<String, ()> {
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => return Err(()),
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        return Ok(trimmed.to_string());
                    }
                    // Continue loop to skip empty lines
                }
                Err(_) => return Err(()),
            }
        }
    }

    fn parse_request(line: &str) -> Result<MCPRequest, ()> {
        serde_json::from_str(line).map_err(|_| ())
    }

    async fn handle_and_respond(
        &self,
        request: MCPRequest,
        stdout: &mut tokio::io::Stdout,
    ) -> Result<(), ()> {
        if let Some(response) = self.server.handle(request).await {
            Self::write_response(&response, stdout).await
        } else {
            Ok(())
        }
    }

    async fn write_response(
        response: &mcp_sdk::response::MCPResponse,
        stdout: &mut tokio::io::Stdout,
    ) -> Result<(), ()> {
        let response_json = serde_json::to_string(response).map_err(|_| ())?;

        stdout
            .write_all(response_json.as_bytes())
            .await
            .map_err(|_| ())?;
        stdout.write_all(b"\n").await.map_err(|_| ())?;
        stdout.flush().await.map_err(|_| ())
    }
}

#[tokio::main]
async fn main() {
    let mcp_server = McpServer::new();
    mcp_server.run().await;
}
