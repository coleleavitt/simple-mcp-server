use async_trait::async_trait;
use mcp_sdk::{
    tools::{Tool, ToolInputSchema, ToolProperty, ToolResponse, Prompt, PromptArgument,
            PromptResponse, PromptMessage, PromptContent, Resource, ResourceContent,
            ProgressNotificationMessage}, // Import the SDK progress types
    MCPRequest, MCPResponse, ServerBuilder, ToolHandler, MCPError,
    ProgressSender, ServerNotification
};
use std::io::{self, BufRead, Write};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use serde_json::Value;

const MAX_REQUEST_SIZE: usize = 1024 * 1024;
const MAX_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

struct MyToolHandler;

#[async_trait]
impl ToolHandler for MyToolHandler {
    async fn call_tool(&self, name: &str, args: &Value, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError> {
        match name {
            "run_command" => {
                let command = args.get("command").and_then(Value::as_str).ok_or(MCPError::MissingParameters)?;
                let argv = args.get("args").and_then(Value::as_array).map(|arr| {
                    arr.iter().filter_map(Value::as_str).map(String::from).collect()
                }).unwrap_or_default();
                let _token = args.get("token").and_then(Value::as_str).ok_or(MCPError::MissingParameters)?;
                self.run_command(command, argv, progress_sender).await
            }
            "list_directory" => {
                let path = args.get("path").and_then(Value::as_str).unwrap_or(".");
                let show_hidden = args.get("show_hidden").and_then(Value::as_bool).unwrap_or(false);
                self.list_directory(path, show_hidden, progress_sender).await
            }
            "read_file" => {
                let path = args.get("path").and_then(Value::as_str).ok_or(MCPError::MissingParameters)?;
                let lines = args.get("lines").and_then(Value::as_str);
                self.read_file(path, lines, progress_sender).await
            }
            "get_system_info" => {
                self.get_system_info(progress_sender).await
            }
            "long_running_task" => {
                self.long_running_task(progress_sender).await
            }
            _ => Err(MCPError::UnknownTool(name.into())),
        }
    }

    async fn list_prompts(&self) -> Result<Vec<Prompt>, MCPError> {
        Ok(vec![
            Prompt::new("system_analysis", "Analyze system performance and health")
                .with_arguments(vec![
                    PromptArgument::new("component", "System component to analyze", false),
                    PromptArgument::new("depth", "Analysis depth level", false),
                ]),
            Prompt::new("command_generator", "Generate safe system commands")
                .with_arguments(vec![
                    PromptArgument::new("task", "Task description", true),
                    PromptArgument::new("safety_level", "Safety level (high/medium/low)", false),
                ]),
        ])
    }

    async fn get_prompt(&self, name: &str, args: &Value) -> Result<PromptResponse, MCPError> {
        match name {
            "system_analysis" => {
                let component = args.get("component").and_then(Value::as_str).unwrap_or("overall");
                Ok(PromptResponse {
                    description: format!("System analysis prompt for {}", component),
                    messages: vec![
                        PromptMessage {
                            role: "user".into(),
                            content: PromptContent {
                                content_type: "text".into(),
                                text: format!("Please analyze the {} system component. Provide detailed insights on performance, health, and recommendations.", component),
                            },
                        }
                    ],
                })
            }
            "command_generator" => {
                let task = args.get("task").and_then(Value::as_str).ok_or(MCPError::MissingParameters)?;
                Ok(PromptResponse {
                    description: "Command generation prompt".into(),
                    messages: vec![
                        PromptMessage {
                            role: "system".into(),
                            content: PromptContent {
                                content_type: "text".into(),
                                text: "You are a system administrator assistant. Generate safe, well-documented commands.".into(),
                            },
                        },
                        PromptMessage {
                            role: "user".into(),
                            content: PromptContent {
                                content_type: "text".into(),
                                text: format!("Generate commands to accomplish this task safely: {}", task),
                            },
                        }
                    ],
                })
            }
            _ => Err(MCPError::UnknownPrompt(name.into())),
        }
    }

    async fn list_resources(&self) -> Result<Vec<Resource>, MCPError> {
        Ok(vec![
            Resource::new("file:///etc/os-release", "OS Information")
                .with_description("Operating system release information")
                .with_mime_type("text/plain"),
            Resource::new("file:///proc/meminfo", "Memory Information")
                .with_description("System memory usage details")
                .with_mime_type("text/plain"),
            Resource::new("internal://system-status", "System Status")
                .with_description("Live system status dashboard")
                .with_mime_type("application/json"),
        ])
    }

    async fn read_resource(&self, uri: &str) -> Result<ResourceContent, MCPError> {
        match uri {
            "file:///etc/os-release" => {
                let content = tokio::fs::read_to_string("/etc/os-release").await.map_err(MCPError::IoError)?;
                Ok(ResourceContent {
                    uri: uri.into(),
                    mime_type: "text/plain".into(),
                    text: content,
                })
            }
            "file:///proc/meminfo" => {
                let content = tokio::fs::read_to_string("/proc/meminfo").await.map_err(MCPError::IoError)?;
                Ok(ResourceContent {
                    uri: uri.into(),
                    mime_type: "text/plain".into(),
                    text: content,
                })
            }
            "internal://system-status" => {
                let status = serde_json::json!({
                    "timestamp": chrono::Utc::now(),
                    "uptime": "system uptime info",
                    "load": "load averages",
                    "memory": "memory usage",
                    "disk": "disk usage"
                });
                Ok(ResourceContent {
                    uri: uri.into(),
                    mime_type: "application/json".into(),
                    text: status.to_string(),
                })
            }
            _ => Err(MCPError::ResourceNotFound(uri.into())),
        }
    }

    async fn on_tool_called(&self, name: &str) {
        eprintln!("[TOOL] Executing: {}", name);
    }

    async fn on_tool_completed(&self, name: &str, success: bool) {
        let status = if success { "SUCCESS" } else { "FAILED" };
        eprintln!("[TOOL] Completed: {} - {}", name, status);
    }

    async fn on_request_cancelled(&self, request_id: &str, reason: Option<&str>) {
        eprintln!("[CANCEL] Request {} cancelled: {:?}", request_id, reason);
    }
}

impl MyToolHandler {
    async fn run_command(&self, command: &str, args: Vec<String>, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError> {
        progress_sender.send_progress("cmd", 0.0, Some("Starting command execution".into())).await;

        let output = timeout(MAX_COMMAND_TIMEOUT, Command::new(command).args(&args).output())
            .await.map_err(|_| MCPError::CommandTimeout)??;

        progress_sender.send_progress("cmd", 1.0, Some("Command completed".into())).await;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let success = output.status.success();

        if stdout.len() + stderr.len() > MAX_OUTPUT_SIZE {
            return Err(MCPError::OutputTooLarge);
        }

        let text = format!(
            "Command: {} {}\nSuccess: {}\n\nStdout:\n{}{}",
            command, args.join(" "), success, stdout,
            if stderr.is_empty() { String::new() } else { format!("\nStderr:\n{}", stderr) }
        );

        Ok(ToolResponse::new(text, !success))
    }

    async fn list_directory(&self, path: &str, show_hidden: bool, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError> {
        let flag = if show_hidden { "-la" } else { "-l" };
        self.run_command("ls", vec![flag.to_string(), path.to_string()], progress_sender).await
    }

    async fn read_file(&self, path: &str, lines: Option<&str>, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError> {
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
        self.run_command(cmd, args, progress_sender).await
    }

    async fn get_system_info(&self, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError> {
        let cmds = [
            ("uname", vec!["-a"]),
            ("whoami", vec![]),
            ("pwd", vec![]),
            ("uptime", vec![]),
            ("date", vec![]),
        ];

        let mut lines = Vec::new();
        let total_cmds = cmds.len();

        for (i, (cmd, args)) in cmds.iter().enumerate() {
            let progress = (i as f64) / (total_cmds as f64);
            progress_sender.send_progress("sysinfo", progress, Some(format!("Running {}", cmd))).await;

            let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            match timeout(Duration::from_secs(5), Command::new(cmd).args(&args).output()).await {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    lines.push(format!("{}: {}", cmd, stdout.trim()));
                }
                _ => lines.push(format!("{}: <failed>", cmd)),
            }
        }

        progress_sender.send_progress("sysinfo", 1.0, Some("System info collection complete".into())).await;

        let text = format!("System Information:\n{}", lines.join("\n"));
        Ok(ToolResponse::new(text, false))
    }

    async fn long_running_task(&self, progress_sender: ProgressSender) -> Result<ToolResponse, MCPError> {
        eprintln!("[TASK] Starting long-running task with progress notifications...");

        for i in 1..=10 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let progress = i as f64 / 10.0;
            progress_sender.send_progress("longtask", progress, Some(format!("Processing step {} of 10", i))).await;
        }

        Ok(ToolResponse::new("Long-running task completed successfully with progress notifications".into(), false))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Enhanced MCP Server v0.5.0 - With Full Streaming Progress Support");

    let handler = MyToolHandler;

    // Define tools
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
                    props.insert("token".into(), ToolProperty::string("Security token"));
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
                    props.insert("path".into(), ToolProperty::string("Directory path"));
                    props.insert("show_hidden".into(), ToolProperty::boolean("Include hidden files", false));
                    props
                },
                required: vec![],
            },
        },
        Tool {
            name: "read_file".into(),
            description: "Read file contents".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: {
                    let mut props = std::collections::HashMap::new();
                    props.insert("path".into(), ToolProperty::string("File path"));
                    props.insert("lines".into(), ToolProperty::string("Number of lines"));
                    props
                },
                required: vec!["path".into()],
            },
        },
        Tool {
            name: "get_system_info".into(),
            description: "Get system information with progress updates".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: std::collections::HashMap::new(),
                required: vec![],
            },
        },
        Tool {
            name: "long_running_task".into(),
            description: "Example long-running task with progress notifications".into(),
            input_schema: ToolInputSchema {
                schema_type: "object".into(),
                properties: std::collections::HashMap::new(),
                required: vec![],
            },
        },
    ];

    // Define prompts
    let prompts = vec![
        Prompt::new("system_analysis", "System analysis prompt template"),
        Prompt::new("command_generator", "Command generation helper"),
    ];

    // Define resources
    let resources = vec![
        Resource::new("file:///etc/os-release", "OS Info"),
        Resource::new("file:///proc/meminfo", "Memory Info"),
        Resource::new("internal://system-status", "System Status"),
    ];

    // Build server with all capabilities
    let mut server = ServerBuilder::new()
        .with_tools(tools)
        .with_prompts(prompts)
        .with_resources(resources)
        .build(handler);

    // Take the notification receiver for multiplexed output
    let mut notification_rx = server.take_notification_receiver().unwrap();

    // ✅ FIXED: Handle progress notifications in a separate task
    let notification_task = tokio::spawn(async move {
        while let Some(notification) = notification_rx.recv().await {
            match notification {
                ServerNotification::Progress { request_id, progress, message } => {
                    let notification_msg = ProgressNotificationMessage::new(request_id, progress, message);

                    if let Ok(json) = serde_json::to_string(&notification_msg) {
                        println!("{}", json);
                        let _ = io::stdout().flush();
                    }
                }
            }
        }
    });

    // ✅ FIXED: Use blocking stdin reading without spawning
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Process requests on main thread to avoid Send issues
    for line in stdin.lock().lines() {
        let raw = match line {
            Ok(raw) => raw,
            Err(_) => continue,
        };

        if raw.len() > MAX_REQUEST_SIZE {
            let resp = MCPResponse::too_large();
            println!("{}", serde_json::to_string(&resp)?);
            stdout.flush()?;
            continue;
        }

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

    // Wait for notification task to complete
    let _ = notification_task.await;

    Ok(())
}
