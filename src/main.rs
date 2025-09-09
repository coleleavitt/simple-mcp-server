// src/main.rs

use async_trait::async_trait;
use mcp_sdk::error::MCPError;
use mcp_sdk::notifications::ProgressSender;
use mcp_sdk::request::MCPRequest;
use mcp_sdk::server::{SystemMCPServer, ToolHandler};
use mcp_sdk::tools::{
    CallToolResult, ContentBlock, ReadResourceResult, TextContent, Tool, ToolInputSchema,
};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
// FIX: Import the trait that provides the .read_line() method
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use std::time::Duration;

struct BashToolHandler;

#[async_trait]
impl ToolHandler for BashToolHandler {
    async fn list_tools(&self) -> Result<Vec<Tool>, MCPError> {
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
        Ok(vec![bash_tool])
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

    async fn read_resource(&self, uri: &str) -> Result<ReadResourceResult, MCPError> {
        Err(MCPError::ResourceNotFound(uri.to_string()))
    }
}

impl BashToolHandler {
    async fn execute_bash_command(
        &self,
        args: &Value,
        _progress_sender: ProgressSender,
    ) -> Result<CallToolResult, MCPError> {
        let command = args.get("command").and_then(|v| v.as_str()).ok_or_else(|| {
            MCPError::MissingParameters("Missing required 'command' parameter".to_string())
        })?;

        let timeout_seconds = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);

        let mut cmd = Command::new("bash");
        // FIX: Set kill_on_drop to true to automatically handle cleanup on timeout.
        cmd.kill_on_drop(true);
        cmd.arg("-c").arg(command).stdout(Stdio::piped()).stderr(Stdio::piped());

        let child = cmd.spawn().map_err(MCPError::IoError)?;

        // Race the process completion against a timer.
        let timeout = tokio::time::sleep(Duration::from_secs(timeout_seconds));
        tokio::pin!(timeout);

        tokio::select! {
            // Bias select to poll the future first if both are ready.
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

                Ok(CallToolResult {
                    content: vec![ContentBlock::Text(TextContent { text: response_text, annotations: None })],
                    structured_content: None,
                    is_error: !output.status.success(),
                })
            }
            _ = &mut timeout => {
                // The timer finished first. kill_on_drop will handle the process.
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