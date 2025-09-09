use async_trait::async_trait;
use mcp_sdk::error::MCPError;
use mcp_sdk::notifications::ProgressSender;
use mcp_sdk::request::MCPRequest;
use mcp_sdk::server::{SystemMCPServer, ToolHandler};
use mcp_sdk::tools::{Tool, ToolInputSchema, ToolProperty, ToolResponse};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

struct BashToolHandler;

#[async_trait]
impl ToolHandler for BashToolHandler {
    async fn call_tool(
        &self,
        name: &str,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<ToolResponse, MCPError> {
        match name {
            "bash" => self.execute_bash_command(args, progress_sender).await,
            _ => Err(MCPError::UnknownTool(name.to_string())),
        }
    }
}

impl BashToolHandler {
    async fn execute_bash_command(
        &self,
        args: &Value,
        progress_sender: ProgressSender,
    ) -> Result<ToolResponse, MCPError> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or(MCPError::MissingParameters)?;

        let timeout_seconds = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);

        let working_dir = args.get("working_dir").and_then(|v| v.as_str());

        let _ = progress_sender
            .send_progress(
                "request",
                0.1,
                Some("Starting command execution".to_string()),
            )
            .await;

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| MCPError::IoError(e))?;

        let _ = progress_sender
            .send_progress(
                "request",
                0.2,
                Some("Command started, reading output".to_string()),
            )
            .await;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let stdout_reader = BufReader::new(stdout);
        let stderr_reader = BufReader::new(stderr);

        let mut stdout_lines = stdout_reader.lines();
        let mut stderr_lines = stderr_reader.lines();

        let mut stdout_output = Vec::new();
        let mut stderr_output = Vec::new();

        let timeout =
            tokio::time::timeout(std::time::Duration::from_secs(timeout_seconds), async {
                loop {
                    tokio::select! {
                        stdout_line = stdout_lines.next_line() => {
                            match stdout_line {
                                Ok(Some(line)) => stdout_output.push(line),
                                Ok(None) => break,
                                Err(e) => return Err(MCPError::IoError(e)),
                            }
                        }
                        stderr_line = stderr_lines.next_line() => {
                            match stderr_line {
                                Ok(Some(line)) => stderr_output.push(line),
                                Ok(None) => {},
                                Err(e) => return Err(MCPError::IoError(e)),
                            }
                        }
                    }
                }

                let _ = progress_sender
                    .send_progress(
                        "request",
                        0.8,
                        Some("Waiting for command completion".to_string()),
                    )
                    .await;

                let exit_status = child.wait().await.map_err(|e| MCPError::IoError(e))?;

                Ok((exit_status, stdout_output, stderr_output))
            });

        let (exit_status, stdout_output, stderr_output) = match timeout.await {
            Ok(result) => result?,
            Err(_) => {
                let _ = child.kill().await;
                return Ok(ToolResponse::new(
                    format!("Command timed out after {} seconds", timeout_seconds),
                    true,
                ));
            }
        };

        let _ = progress_sender
            .send_progress("request", 1.0, Some("Command completed".to_string()))
            .await;

        let mut response_text = String::new();

        response_text.push_str(&format!("Command: {}\n", command));
        response_text.push_str(&format!(
            "Exit code: {}\n\n",
            exit_status.code().unwrap_or(-1)
        ));

        if !stdout_output.is_empty() {
            response_text.push_str("STDOUT:\n");
            response_text.push_str(&stdout_output.join("\n"));
            response_text.push_str("\n\n");
        }

        if !stderr_output.is_empty() {
            response_text.push_str("STDERR:\n");
            response_text.push_str(&stderr_output.join("\n"));
            response_text.push_str("\n");
        }

        let is_error = !exit_status.success();
        Ok(ToolResponse::new(response_text, is_error))
    }
}

#[tokio::main]
async fn main() {
    let bash_tool = Tool {
        name: "bash".to_string(),
        description: "Execute bash commands with support for complex operations like rg, sed, awk, grep, find, etc.".to_string(),
        input_schema: ToolInputSchema {
            schema_type: "object".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert(
                    "command".to_string(),
                    ToolProperty::string("The bash command to execute")
                );
                props.insert(
                    "timeout".to_string(),
                    ToolProperty {
                        property_type: "number".to_string(),
                        description: "Timeout in seconds (default: 30)".to_string(),
                        items: None,
                        default: Some(Value::Number(30.into())),
                    }
                );
                props.insert(
                    "working_dir".to_string(),
                    ToolProperty {
                        property_type: "string".to_string(),
                        description: "Working directory for command execution (optional)".to_string(),
                        items: None,
                        default: None,
                    }
                );
                props
            },
            required: vec!["command".to_string()],
        },
    };

    let server = SystemMCPServer::<BashToolHandler>::builder()
        .with_tools(vec![bash_tool])
        .build(BashToolHandler);

    eprintln!("Bash MCP Server starting...");

    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    loop {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

        let mut reader = BufReader::new(&mut stdin);
        let mut line = String::new();

        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<MCPRequest>(&line) {
                    Ok(request) => {
                        if let Some(response) = server.handle(request).await {
                            let response_json = serde_json::to_string(&response).unwrap();
                            stdout.write_all(response_json.as_bytes()).await.unwrap();
                            stdout.write_all(b"\n").await.unwrap();
                            stdout.flush().await.unwrap();
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to parse request: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read line: {}", e);
                break;
            }
        }
    }
}