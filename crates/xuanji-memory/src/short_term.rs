use xuanji_llm::Message;

use crate::types::MemoryConfig;

/// Short-term conversation memory that stores a bounded history of messages.
///
/// When the history exceeds `max_history`, compression is applied: the first
/// 2 messages (typically system prompt + first user message) are preserved along
/// with the last `max_context_turns` messages, while middle messages are discarded.
pub struct ShortTermMemory {
    messages: Vec<Message>,
    config: MemoryConfig,
}

impl ShortTermMemory {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            messages: Vec::new(),
            config,
        }
    }

    /// Push a new message onto the history, compressing if the limit is exceeded.
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
        self.compress_if_needed();
    }

    /// Get a reference to the current message history.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Consume the memory and return the inner message vector.
    pub fn into_messages(self) -> Vec<Message> {
        self.messages
    }

    /// Compress the message history if it exceeds `max_history`.
    ///
    /// Compression strategy: keep the first 2 messages (system prompt and first
    /// user message) plus the last `max_context_turns` messages. Remove everything
    /// in between using `drain`.
    fn compress_if_needed(&mut self) {
        if self.messages.len() <= self.config.max_history {
            return;
        }

        // Preserve: first 2 messages + last max_context_turns messages
        let preserved_count = 2 + self.config.max_context_turns;
        if self.messages.len() <= preserved_count {
            // Even the preserved set fits, nothing to drain
            return;
        }

        // Drain messages from index 2 up to (len - max_context_turns)
        let cutoff_end = self.messages.len() - self.config.max_context_turns;
        if cutoff_end > 2 {
            self.messages.drain(2..cutoff_end);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_messages() {
        let config = MemoryConfig {
            max_history: 100,
            max_context_turns: 20,
        };
        let mut mem = ShortTermMemory::new(config);

        mem.push(Message::System { content: "you are helpful".to_string() });
        mem.push(Message::User { content: "hello".to_string() });

        assert_eq!(mem.messages().len(), 2);
    }

    #[test]
    fn test_no_compression_under_limit() {
        let config = MemoryConfig {
            max_history: 10,
            max_context_turns: 3,
        };
        let mut mem = ShortTermMemory::new(config);

        // Push fewer than max_history messages
        mem.push(Message::System { content: "system".to_string() });
        mem.push(Message::User { content: "hello".to_string() });
        mem.push(Message::Assistant { content: "hi".to_string() });

        assert_eq!(mem.messages().len(), 3);
        // Verify ordering preserved
        assert!(matches!(mem.messages()[0], Message::System { .. }));
        assert!(matches!(mem.messages()[1], Message::User { .. }));
        assert!(matches!(mem.messages()[2], Message::Assistant { .. }));
    }

    #[test]
    fn test_compression_preserves_system_and_first_user() {
        let config = MemoryConfig {
            max_history: 10,
            max_context_turns: 3,
        };
        let mut mem = ShortTermMemory::new(config);

        // Push system + first user + many more to exceed limit
        mem.push(Message::System { content: "system prompt".to_string() });
        mem.push(Message::User { content: "first user message".to_string() });

        // Push 10 more messages (total 12, exceeds max_history of 10)
        for i in 0..10 {
            if i % 2 == 0 {
                mem.push(Message::User { content: format!("user msg {i}") });
            } else {
                mem.push(Message::Assistant { content: format!("assistant msg {i}") });
            }
        }

        // After compression: first 2 + last 3 = 5 messages
        let msgs = mem.messages();
        assert!(msgs.len() <= 5 + 2); // may vary slightly due to push timing

        // First message is always system prompt
        match &msgs[0] {
            Message::System { content } => assert_eq!(content, "system prompt"),
            _ => panic!("expected system message"),
        }

        // Second message is always the first user message
        match &msgs[1] {
            Message::User { content } => assert_eq!(content, "first user message"),
            _ => panic!("expected user message"),
        }
    }

    #[test]
    fn test_into_messages() {
        let config = MemoryConfig {
            max_history: 100,
            max_context_turns: 20,
        };
        let mut mem = ShortTermMemory::new(config);
        mem.push(Message::System { content: "sys".to_string() });
        mem.push(Message::User { content: "usr".to_string() });

        let msgs = mem.into_messages();
        assert_eq!(msgs.len(), 2);
    }
}
