use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::Mutex;
use xuanji_llm::types::ToolSchema;

use crate::client::{McpClient, McpToolResult};
use crate::error::PluginError;

/// Alias for a boxed, sendable async function that executes a system tool.
type SystemToolFn =
    Arc<dyn Fn(Value) -> Pin<Box<dyn Future<Output = Result<McpToolResult, PluginError>> + Send>> + Send + Sync>;

/// Describes where a registered tool comes from.
pub enum ToolSource {
    /// Tool is provided by an MCP server subprocess.
    Mcp { server_name: String },
    /// Tool is a built-in system function.
    System { tool_fn: SystemToolFn },
}

/// A single entry in the tool registry.
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub source: ToolSource,
}

/// Registry that aggregates tools from MCP servers and built-in system tools.
pub struct ToolRegistry {
    entries: HashMap<String, ToolEntry>,
    clients: HashMap<String, Arc<Mutex<McpClient>>>,
}

impl ToolRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    /// Register all tools from an MCP server.
    ///
    /// The client must already be initialized. This queries the server for its
    /// tool list and adds each tool to the registry.
    pub async fn register_server(&mut self, mut client: McpClient) -> Result<(), PluginError> {
        let server_name = client.name().to_string();

        let tools = client.list_tools().await?;

        let client = Arc::new(Mutex::new(client));

        for tool in tools {
            let description = tool.description.clone().unwrap_or_default();
            let tool_name = tool.name.clone();

            self.entries.insert(
                tool_name.clone(),
                ToolEntry {
                    name: tool_name,
                    description,
                    parameters: tool.input_schema,
                    source: ToolSource::Mcp {
                        server_name: server_name.clone(),
                    },
                },
            );
        }

        self.clients.insert(server_name, client);
        Ok(())
    }

    /// Register a built-in system tool.
    pub fn register_system_tool<F, Fut>(
        &mut self,
        name: &str,
        description: &str,
        parameters: Value,
        tool_fn: F,
    ) where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<McpToolResult, PluginError>> + Send + 'static,
    {
        let tool_fn: SystemToolFn =
            Arc::new(move |args| Box::pin(tool_fn(args)));

        self.entries.insert(
            name.to_string(),
            ToolEntry {
                name: name.to_string(),
                description: description.to_string(),
                parameters,
                source: ToolSource::System { tool_fn },
            },
        );
    }

    /// Ensure a specific MCP server's tools are loaded.
    ///
    /// Currently a no-op placeholder for lazy loading support.
    pub async fn ensure_loaded(&mut self, _server_name: &str) -> Result<(), PluginError> {
        // Placeholder for future lazy-loading logic.
        Ok(())
    }

    /// Load all registered MCP server tools eagerly.
    ///
    /// This is a no-op if tools have already been loaded via `register_server`.
    pub async fn load_all(&mut self) -> Result<(), PluginError> {
        // Tools are already loaded at registration time.
        Ok(())
    }

    /// Return tool schemas in the format expected by the LLM layer.
    pub fn all_tool_schemas(&self) -> Vec<ToolSchema> {
        self.entries
            .values()
            .map(|entry| ToolSchema {
                name: entry.name.clone(),
                description: entry.description.clone(),
                input_schema: entry.parameters.clone(),
            })
            .collect()
    }

    /// Invoke a tool by name.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<McpToolResult, PluginError> {
        let entry = self
            .entries
            .get(name)
            .ok_or_else(|| PluginError::ToolNotFound(name.to_string()))?;

        match &entry.source {
            ToolSource::Mcp { server_name } => {
                let client = self
                    .clients
                    .get(server_name)
                    .ok_or_else(|| PluginError::ServerNotFound(server_name.clone()))?;

                let mut locked = client.lock().await;
                locked.call_tool(name, arguments).await
            }
            ToolSource::System { tool_fn } => (tool_fn)(arguments).await,
        }
    }

    /// Shut down all MCP server processes.
    pub async fn shutdown_all(&self) -> Result<(), PluginError> {
        for (name, client) in &self.clients {
            let mut locked = client.lock().await;
            if let Err(e) = locked.shutdown().await {
                tracing::warn!(name = %name, error = %e, "error shutting down MCP server");
            }
        }
        Ok(())
    }

    /// List all registered tool names with descriptions.
    pub fn list_tools(&self) -> Vec<(&str, &str)> {
        self.entries
            .values()
            .map(|e| (e.name.as_str(), e.description.as_str()))
            .collect()
    }

    /// Look up a single tool entry by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolEntry> {
        self.entries.get(name)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
