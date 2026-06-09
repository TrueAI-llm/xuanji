use thiserror::Error;

/// Errors that can occur in the plugin subsystem.
#[derive(Error, Debug)]
pub enum PluginError {
    #[error("process error: {0}")]
    Process(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("server not found: {0}")]
    ServerNotFound(String),

    #[error("tool not found: {0}")]
    ToolNotFound(String),

    #[error("tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
