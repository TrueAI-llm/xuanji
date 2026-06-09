use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{ChildStdin, ChildStdout};

use crate::error::PluginError;
use crate::process::McpProcess;

/// Metadata for a single tool exposed by an MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: Value,
}

/// The result of invoking a tool on an MCP server.
#[derive(Debug, Clone)]
pub struct McpToolResult {
    pub content: Value,
    pub is_error: bool,
}

/// JSON-RPC 2.0 request envelope.
#[derive(Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC 2.0 response envelope.
#[derive(Deserialize, Debug)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    #[allow(dead_code)]
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

/// JSON-RPC 2.0 notification envelope (no id).
#[derive(Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// An MCP client that communicates with a single server subprocess over
/// newline-delimited JSON-RPC 2.0 on stdin/stdout.
///
/// The pipes are taken from the child process during `initialize()` and stored
/// for the lifetime of this client, avoiding borrow-checker issues with
/// repeated `take()` calls.
pub struct McpClient {
    process: McpProcess,
    stdin: Option<BufWriter<ChildStdin>>,
    stdout: Option<BufReader<ChildStdout>>,
    next_id: u64,
}

impl McpClient {
    /// Create a new client wrapping the given process handle.
    pub fn new(process: McpProcess) -> Self {
        Self {
            process,
            stdin: None,
            stdout: None,
            next_id: 1,
        }
    }

    /// Return the configured name of the MCP server.
    pub fn name(&self) -> &str {
        self.process.name()
    }

    /// Perform the MCP initialization handshake.
    ///
    /// Starts the subprocess (if not already running), takes ownership of its
    /// stdin/stdout pipes, then sends `initialize` and the
    /// `notifications/initialized` notification.
    pub async fn initialize(&mut self) -> Result<(), PluginError> {
        self.process.ensure_started().await?;
        self.take_pipes()?;

        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "xuanji",
                "version": env!("CARGO_PKG_VERSION"),
            }
        });

        let _response = self
            .send_request("initialize", Some(init_params))
            .await?;

        self.send_notification("notifications/initialized", None)
            .await?;

        tracing::info!(name = %self.process.name(), "MCP client initialized");
        Ok(())
    }

    /// List all tools exposed by the server.
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, PluginError> {
        let response = self.send_request("tools/list", None).await?;

        let tools: Vec<McpToolInfo> = serde_json::from_value(
            response
                .get("tools")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        )
        .map_err(|e| {
            PluginError::Protocol(format!("failed to parse tools/list response: {e}"))
        })?;

        Ok(tools)
    }

    /// Invoke a tool by name with the given arguments.
    pub async fn call_tool(
        &mut self,
        name: &str,
        arguments: Value,
    ) -> Result<McpToolResult, PluginError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let response = self.send_request("tools/call", Some(params)).await?;

        let is_error = response
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let content = response
            .get("content")
            .cloned()
            .unwrap_or(response.clone());

        Ok(McpToolResult { content, is_error })
    }

    /// Shut down the server gracefully.
    pub async fn shutdown(&mut self) -> Result<(), PluginError> {
        // Drop the pipes first so the subprocess sees EOF.
        self.stdin = None;
        self.stdout = None;
        self.process.kill().await
    }

    // -- private helpers -------------------------------------------------------

    /// Extract the stdin and stdout handles from the child process.
    ///
    /// This consumes the `Child` inside `McpProcess`, so it can only be called
    /// once. The process will not be killable through `McpProcess::kill()` after
    /// this; instead, dropping the pipes or calling `McpClient::shutdown()` will
    /// terminate it.
    fn take_pipes(&mut self) -> Result<(), PluginError> {
        let child = self
            .process
            .take_child()
            .ok_or_else(|| PluginError::Protocol("process not started".into()))?;

        let stdin = child
            .stdin
            .ok_or_else(|| PluginError::Protocol("stdin not captured".into()))?;
        let stdout = child
            .stdout
            .ok_or_else(|| PluginError::Protocol("stdout not captured".into()))?;

        self.stdin = Some(BufWriter::new(stdin));
        self.stdout = Some(BufReader::new(stdout));
        Ok(())
    }

    /// Send a JSON-RPC request and wait for the response.
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, PluginError> {
        let id = self.next_id;
        self.next_id += 1;

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        let line = serde_json::to_string(&request).map_err(PluginError::Json)?;

        // Write request to stdin.
        {
            let writer = self.stdin.as_mut().ok_or_else(|| {
                PluginError::Protocol(
                    "stdin not available (client not initialized?)".into(),
                )
            })?;

            writer
                .write_all(format!("{line}\n").as_bytes())
                .await
                .map_err(PluginError::Process)?;
            writer.flush().await.map_err(PluginError::Process)?;
        }

        // Read response from stdout.
        let response_line = {
            let reader = self.stdout.as_mut().ok_or_else(|| {
                PluginError::Protocol(
                    "stdout not available (client not initialized?)".into(),
                )
            })?;

            let mut buf = String::new();
            reader
                .read_line(&mut buf)
                .await
                .map_err(PluginError::Process)?;
            buf
        };

        if response_line.trim().is_empty() {
            return Err(PluginError::Protocol(
                "received empty response from MCP server".into(),
            ));
        }

        let response: JsonRpcResponse = serde_json::from_str(&response_line).map_err(|e| {
            PluginError::Protocol(format!("failed to parse JSON-RPC response: {e}"))
        })?;

        if let Some(err) = response.error {
            return Err(PluginError::Protocol(format!(
                "JSON-RPC error: {}",
                err.message
            )));
        }

        response
            .result
            .ok_or_else(|| PluginError::Protocol("JSON-RPC response missing result field".into()))
    }

    /// Send a JSON-RPC notification (no id, no response expected).
    async fn send_notification(
        &mut self,
        method: &str,
        params: Option<Value>,
    ) -> Result<(), PluginError> {
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        let line = serde_json::to_string(&notification).map_err(PluginError::Json)?;

        let writer = self.stdin.as_mut().ok_or_else(|| {
            PluginError::Protocol(
                "stdin not available (client not initialized?)".into(),
            )
        })?;

        writer
            .write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(PluginError::Process)?;
        writer.flush().await.map_err(PluginError::Process)?;

        Ok(())
    }
}
