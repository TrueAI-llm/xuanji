use std::process::Stdio;

use crate::error::PluginError;
use crate::types::McpServerConfig;

/// Manages a single MCP server subprocess.
pub struct McpProcess {
    config: McpServerConfig,
    pub(crate) child: Option<tokio::process::Child>,
}

impl McpProcess {
    /// Create a new (not yet started) process handle.
    pub fn new(config: McpServerConfig) -> Self {
        Self {
            config,
            child: None,
        }
    }

    /// Return the configured name of this server.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Lazily start the subprocess if it is not already running.
    ///
    /// If the child process has exited it will be restarted.
    pub async fn ensure_started(&mut self) -> Result<(), PluginError> {
        let needs_start = match self.child {
            Some(ref mut child) => {
                // Try to poll whether the child has already exited.
                matches!(child.try_wait(), Ok(Some(_)))
            }
            None => true,
        };

        if needs_start {
            tracing::info!(name = %self.config.name, "starting MCP server process");

            let mut cmd = tokio::process::Command::new(&self.config.command);
            cmd.args(&self.config.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit());

            // Merge configured env vars into the child environment.
            for (k, v) in &self.config.env {
                cmd.env(k, v);
            }

            let child = cmd.spawn().map_err(PluginError::Process)?;
            self.child = Some(child);
        }

        Ok(())
    }

    /// Take the child process, returning it as `Some` if it exists.
    /// Used by `McpClient` to take ownership of the child's pipes.
    pub(crate) fn take_child(&mut self) -> Option<tokio::process::Child> {
        self.child.take()
    }

    /// Kill the subprocess immediately.
    pub async fn kill(&mut self) -> Result<(), PluginError> {
        if let Some(ref mut child) = self.child {
            tracing::info!(name = %self.config.name, "killing MCP server process");
            child.start_kill().map_err(PluginError::Process)?;
        }
        self.child = None;
        Ok(())
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Best-effort kill; ignore the result since we are in a drop.
            let _ = child.start_kill();
        }
    }
}
