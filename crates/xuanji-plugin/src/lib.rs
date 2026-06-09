pub mod client;
pub mod error;
pub mod process;
pub mod registry;
pub mod types;

pub use client::{McpClient, McpToolInfo, McpToolResult};
pub use error::PluginError;
pub use process::McpProcess;
pub use registry::{ToolEntry, ToolRegistry, ToolSource};
pub use types::McpServerConfig;
