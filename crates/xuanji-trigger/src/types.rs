use serde::{Deserialize, Serialize};

/// Sender channel type for trigger events.
pub type TriggerSender = tokio::sync::mpsc::Sender<TriggerEvent>;

/// Event emitted by a trigger when it fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerEvent {
    pub trigger_type: String,
    pub workflow_name: String,
    pub payload: serde_json::Value,
}

/// Trigger-specific configuration from xuanji.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    #[serde(default = "default_webhook_port")]
    pub webhook_port: u16,
    #[serde(default = "default_workflows_dir")]
    pub workflows_dir: String,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        Self {
            webhook_port: default_webhook_port(),
            workflows_dir: default_workflows_dir(),
        }
    }
}

fn default_webhook_port() -> u16 {
    9090
}

fn default_workflows_dir() -> String {
    dirs::home_dir()
        .map(|p: std::path::PathBuf| p.join(".xuanji/workflows").to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.xuanji/workflows".to_string())
}
