use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use tokio::process::Command;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let result = match method {
            "initialize" => handle_initialize(),
            "notifications/initialized" => {
                continue;
            }
            "tools/list" => handle_tools_list(),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                handle_tools_call(params)
            }
            _ => json!({"error": {"code": -32601, "message": format!("Unknown method: {}", method)}}),
        };

        if let Some(id) = id {
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            });
            writeln!(stdout, "{}", serde_json::to_string(&response).unwrap()).ok();
        }
    }
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "xuanji-mcp-shell",
            "version": "0.1.0"
        }
    })
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "shell.run",
                "description": "在 shell 中执行命令",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "要执行的命令"
                        }
                    },
                    "required": ["command"]
                }
            }
        ]
    })
}

fn handle_tools_call(params: Value) -> Value {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool_name {
        "shell.run" => {
            let command = args.get("command").and_then(|c| c.as_str()).unwrap_or("");

            let rt = tokio::runtime::Runtime::new().unwrap();
            let output = rt.block_on(async {
                Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
            });

            match output {
                Ok(output) => {
                    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
                    let is_error = !output.status.success();

                    let mut content = vec![json!({
                        "type": "text",
                        "text": if stdout_str.is_empty() { stderr_str.clone() } else { stdout_str },
                    })];

                    if !stderr_str.is_empty() && !output.status.success() {
                        content.push(json!({
                            "type": "text",
                            "text": format!("stderr: {}", stderr_str),
                        }));
                    }

                    json!({
                        "content": content,
                        "isError": is_error,
                    })
                }
                Err(e) => json!({
                    "content": [{"type": "text", "text": format!("Failed to execute: {}", e)}],
                    "isError": true,
                }),
            }
        }
        _ => json!({
            "content": [{"type": "text", "text": format!("Unknown tool: {}", tool_name)}],
            "isError": true,
        }),
    }
}
