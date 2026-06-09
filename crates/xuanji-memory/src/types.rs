/// Configuration for memory management.
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    /// Maximum number of messages to retain in short-term memory history.
    pub max_history: usize,
    /// Maximum number of recent context turns to keep during compression.
    pub max_context_turns: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_history: 100,
            max_context_turns: 20,
        }
    }
}
