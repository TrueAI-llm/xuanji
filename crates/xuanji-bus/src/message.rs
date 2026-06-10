use serde_json::Value;

/// A knowledge message published to the bus.
#[derive(Debug, Clone)]
pub struct KnowledgeMessage {
    /// Name of the agent that published this message.
    pub source_agent: String,
    /// Channel category: "discovery", "warning", "state", or "insight".
    pub channel: String,
    /// Arbitrary JSON payload.
    pub payload: Value,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}

impl KnowledgeMessage {
    /// Create a new knowledge message with the current timestamp.
    pub fn new(source_agent: &str, channel: &str, payload: Value) -> Self {
        let timestamp = chrono::Utc::now().to_rfc3339();
        Self {
            source_agent: source_agent.to_string(),
            channel: channel.to_string(),
            payload,
            timestamp,
        }
    }
}
