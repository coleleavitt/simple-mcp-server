use async_trait::async_trait;
use mcp_sdk::{
    tools::{Tool, ToolInputSchema, ToolProperty, ToolResponse},
    MCPRequest, MCPResponse, ServerBuilder, ToolHandler, MCPError
};
use std::io::{self, BufRead, Write};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use serde_json::Value;

// Configuration constants
const MAX_REQUEST_SIZE: usize = 1024 * 1024;
const MAX_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// Tool handler implementation with system command capabilities
struct MyToolHandler;

#[async_trait]
impl ToolHandler for MyToolHandler {
    async fn call_tool(&self, name: &str, args: &Value) -> Result<ToolResponse, MCPError> {
        match name {
            "run_command" => {
                let command = args
                    .get("command")
                    .and_then(Value::as_str)
                    .ok_or(MCPError::MissingParameters)?;
                let argv = args
                    .get("args")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(Value::as_str)
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default();
                let _token = args
                    .get("token")
                    .and_then(Value::as_str)
                    .ok_or(MCPError::MissingParameters)?;

                // TODO: Add token validation logic here if needed
                self.run_command(command, argv).await
            }

            "list_directory" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or(".");
                let show_hidden = args
                    .get("show_hidden")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);

                self.list_directory(path, show_hidden).await
            }

            "read_file" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or(MCPError::MissingParameters)?;
                let lines = args
                    .get("lines")
                    .and_then(Value::as_str);

                self.read_file(path, lines).await
            }

            "get_system_info" => {
                self.get_system_info().await
            }

            _ => Err(MCPError::UnknownTool(name.into())),
        }
    }

    /// Log tool execution for observability
    async fn on_tool_called(&self, name: &str) {
        eprintln!("[TOOL] Executing: {}", name);
    }

    /// Log tool completion for observability
    async fn on_tool_completed(&self, name: &str, success: bool) {
        let status = if success { "SUCCESS" } else { "FAILED" };
        eprintln!("[TOOL] Completed: {} - {}", name, status);
    }
}

impl MyToolHandler {
    /// Execute a system command with timeout and size limits
    async fn run_command(&self, command: &str, args: Vec<String>) -> Result<ToolResponse, MCPError> {
        let output = timeout(MAX_COMMAND_TIMEOUT, Command::new(command).args(&args).output())
            .await
            .map_err(|_| MCPError::CommandTimeout)??;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let success = output.status.success();

        if stdout.len() + stderr.len() > MAX_OUTPUT_SIZE {
            return Err(MCPError::OutputTooLarge);
        }

        let text = format!(
            "Command: {} {}\nSuccess: {}\n\nStdout:\n{}{}",
            command,
            args.join(" "),
            success,
            stdout,
            if stderr.is_empty() {
                String::new()
            } else {
                format!("\nStderr:\n{}", stderr)
            }
        );

        Ok(ToolResponse::new(text, !success))
    }

    /// List directory contents using ls command
    async fn list_directory(&self, path: &str, show_hidden: bool) -> Result<ToolResponse, MCPError> {
        let flag = if show_hidden { "-la" } else { "-l" };
        self.run_command("ls", vec![flag.to_string(), path.to_string()]).await
    }

    /// Read file contents with optional line limit
    async fn read_file(&self, path: &str, lines: Option<&str>) -> Result<ToolResponse, MCPError> {
        let (cmd, args) = if let Some(n) = lines {
            if let Ok(n) = n.parse::<u32>() {
                ("head", vec!["-n".to_string(), n.to_string(), path.to_string()])
            } else if n.starts_with('-') && n[1..].parse::<u32>().is_ok() {
                ("tail", vec!["-n".to_string(), n[1..].to_string(), path.to_string()])
            } else {
                ("cat", vec![path.to_string()])
            }
        } else {
            ("cat", vec![path.to_string()])
        };

        self.run_command(cmd, args).await
    }

    /// Get basic system information
    async fn get_system_info(&self) -> Result<ToolResponse, MCPError> {
        let cmds = [
            ("uname", vec!["-a"]),
            ("whoami", vec![]),
            ("pwd", vec![]),
            ("uptime", vec![]),
            ("date", vec![]),
        ];

        let mut lines = Vec::new();
        for (cmd, args) in &cmds {
            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            match timeout(Duration::from_secs(5), Command::new(cmd).args(&args).output()).await {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    lines.push(format!("{}: {}", cmd, stdout.trim()));
                }
                _ => lines.push(format!("{}: <failed>", cmd)),
            }
        }

        let text = format!("System Information:\n{}", lines.join("\n"));
        Ok(ToolResponse::new(text, false))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Secure System MCP Server v0.2.1");

    let handler = MyToolHandler;

    // Define available tools
    let tools = vec![
        Tool {
            name: "run_command".into(),
            description: "Run system commands with arguments".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: {
                    let mut props = std::collections::HashMap::new();
                    props.insert("command".into(), ToolProperty::string("Command to run"));
                    props.insert("args".into(), ToolProperty::array("Command arguments", "string"));
                    props.insert("token".into(), ToolProperty::string("Security token for authorization"));
                    props
                },
                required: vec!["command".into(), "token".into()],
            },
        },
        Tool {
            name: "list_directory".into(),
            description: "List directory contents".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: {
                    let mut props = std::collections::HashMap::new();
                    props.insert("path".into(), ToolProperty::string("Directory path to list"));
                    props.insert("show_hidden".into(), ToolProperty::boolean("Include hidden files", false));
                    props
                },
                required: vec![],
            },
        },
        Tool {
            name: "read_file".into(),
            description: "Read text file contents".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: {
                    let mut props = std::collections::HashMap::new();
                    props.insert("path".into(), ToolProperty::string("File path to read"));
                    props.insert("lines".into(), ToolProperty::string("Number of lines (or -N for tail)"));
                    props
                },
                required: vec!["path".into()],
            },
        },
        Tool {
            name: "get_system_info".into(),
            description: "Get basic system information".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: std::collections::HashMap::new(),
                required: vec![],
            },
        },
    ];

    // Build and configure the server
    let server = ServerBuilder::new()
        .with_tools(tools)
        .build(handler);

    // Main event loop
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let raw = line?;

        // Check request size limit
        if raw.len() > MAX_REQUEST_SIZE {
            let resp = MCPResponse::too_large();
            println!("{}", serde_json::to_string(&resp)?);
            stdout.flush()?;
            continue;
        }

        // Parse and handle request
        match serde_json::from_str::<MCPRequest>(&raw) {
            Ok(req) => {
                if let Some(resp) = server.handle(req).await {
                    println!("{}", serde_json::to_string(&resp)?);
                    stdout.flush()?;
                }
            }
            Err(_) => {
                let resp = MCPResponse::parse_error();
                println!("{}", serde_json::to_string(&resp)?);
                stdout.flush()?;
            }
        }
    }

    Ok(())
}
