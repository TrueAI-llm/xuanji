use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for launching an MCP server subprocess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// A human-readable name for this MCP server.
    pub name: String,
    /// The command to execute (e.g. "npx", "python", etc.).
    pub command: String,
    /// Arguments passed to the command.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment variables to set for the subprocess.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env: HashMap<String, String>,
}
