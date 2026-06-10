use serde::{Deserialize, Serialize};

/// Configuration for memory management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum number of messages to retain in short-term memory history.
    #[serde(default = "default_max_history")]
    pub max_history: usize,
    /// Maximum number of recent context turns to keep during compression.
    #[serde(default = "default_max_context_turns")]
    pub max_context_turns: usize,
}

fn default_max_history() -> usize {
    100
}
fn default_max_context_turns() -> usize {
    20
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_history: default_max_history(),
            max_context_turns: default_max_context_turns(),
        }
    }
}
