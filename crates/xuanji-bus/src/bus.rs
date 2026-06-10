use crate::message::KnowledgeMessage;
use serde_json::Value;
use tokio::sync::broadcast;

/// In-process knowledge bus using tokio broadcast channels.
///
/// All agents share the same bus. Messages published are received by all subscribers.
/// Late subscribers do not receive messages published before they subscribed.
#[derive(Clone)]
pub struct KnowledgeBus {
    sender: broadcast::Sender<KnowledgeMessage>,
}

impl KnowledgeBus {
    /// Create a new knowledge bus with the given message capacity.
    ///
    /// If messages are produced faster than consumed, older messages are dropped.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish a message to all subscribers.
    ///
    /// If no subscribers exist, the message is dropped silently.
    pub fn publish(&self, message: KnowledgeMessage) {
        // send() returns Err only when there are no receivers — that's fine.
        let _ = self.sender.send(message);
    }

    /// Subscribe to all messages on the bus.
    ///
    /// The returned receiver will get all messages published after this call.
    pub fn subscribe(&self) -> broadcast::Receiver<KnowledgeMessage> {
        self.sender.subscribe()
    }

    /// Convenience: publish a discovery message.
    pub fn publish_discovery(&self, agent: &str, payload: Value) {
        let msg = KnowledgeMessage::new(agent, "discovery", payload);
        tracing::debug!(agent = %agent, "Publishing discovery");
        self.publish(msg);
    }

    /// Convenience: publish a warning message.
    pub fn publish_warning(&self, agent: &str, payload: Value) {
        let msg = KnowledgeMessage::new(agent, "warning", payload);
        tracing::debug!(agent = %agent, "Publishing warning");
        self.publish(msg);
    }

    /// Convenience: publish a state change message.
    pub fn publish_state(&self, agent: &str, payload: Value) {
        let msg = KnowledgeMessage::new(agent, "state", payload);
        tracing::debug!(agent = %agent, "Publishing state change");
        self.publish(msg);
    }

    /// Convenience: publish an insight message.
    pub fn publish_insight(&self, agent: &str, payload: Value) {
        let msg = KnowledgeMessage::new(agent, "insight", payload);
        tracing::debug!(agent = %agent, "Publishing insight");
        self.publish(msg);
    }

    /// Format bus messages as a human-readable string for prompt injection.
    pub fn format_messages(messages: &[KnowledgeMessage]) -> String {
        if messages.is_empty() {
            return String::new();
        }

        let mut out = String::from("## 协作消息\n来自其他 Agent 的信息：\n");
        for msg in messages {
            let summary = match &msg.payload {
                Value::String(s) => s.clone(),
                Value::Object(m) => m.get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("（无详情）")
                    .to_string(),
                other => other.to_string(),
            };
            // Truncate to 200 chars for prompt brevity
            let truncated = if summary.len() > 200 { &summary[..200] } else { &summary };
            out.push_str(&format!(
                "- [{}] Agent \"{}\": {}\n",
                msg.channel, msg.source_agent, truncated
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_bus_publish_subscribe() {
        let bus = KnowledgeBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(KnowledgeMessage::new("agent-1", "discovery", json!("found something")));

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.source_agent, "agent-1");
        assert_eq!(msg.channel, "discovery");
    }

    #[tokio::test]
    async fn test_bus_multiple_subscribers() {
        let bus = KnowledgeBus::new(16);
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.publish(KnowledgeMessage::new("agent-1", "insight", json!("test")));

        assert!(rx1.try_recv().is_ok());
        assert!(rx2.try_recv().is_ok());
    }

    #[tokio::test]
    async fn test_bus_convenience_methods() {
        let bus = KnowledgeBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish_discovery("a", json!({"text": "found X"}));
        bus.publish_warning("b", json!({"text": "port busy"}));
        bus.publish_state("c", json!({"text": "file changed"}));
        bus.publish_insight("d", json!({"text": "pattern found"}));

        let mut channels = Vec::new();
        for _ in 0..4 {
            channels.push(rx.try_recv().unwrap().channel);
        }
        assert!(channels.contains(&"discovery".to_string()));
        assert!(channels.contains(&"warning".to_string()));
        assert!(channels.contains(&"state".to_string()));
        assert!(channels.contains(&"insight".to_string()));
    }

    #[test]
    fn test_format_messages() {
        let messages = vec![
            KnowledgeMessage::new("researcher", "discovery", json!({"text": "项目使用 pnpm"})),
            KnowledgeMessage::new("builder", "warning", json!({"text": "端口 8080 被占用"})),
        ];
        let formatted = KnowledgeBus::format_messages(&messages);
        assert!(formatted.contains("协作消息"));
        assert!(formatted.contains("researcher"));
        assert!(formatted.contains("builder"));
        assert!(formatted.contains("pnpm"));
    }

    #[test]
    fn test_format_messages_empty() {
        let formatted = KnowledgeBus::format_messages(&[]);
        assert!(formatted.is_empty());
    }
}
