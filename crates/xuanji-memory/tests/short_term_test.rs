use xuanji_llm::Message;
use xuanji_memory::{MemoryConfig, ShortTermMemory};

/// Verify that compression preserves the system message and the first user message,
/// and that middle messages are removed while recent ones are retained.
#[test]
fn compression_preserves_system_and_first_user() {
    let config = MemoryConfig {
        max_history: 8,
        max_context_turns: 3,
    };
    let mut mem = ShortTermMemory::new(config);

    // Push system prompt and first user message
    mem.push(Message::System { content: "system prompt".to_string() });
    mem.push(Message::User { content: "first user message".to_string() });

    // Push enough messages to exceed max_history (8).
    // After pushing the 9th total message, compression fires:
    //   - keep first 2 (system + first user)
    //   - keep last max_context_turns (3) messages
    //   - drain everything in between
    // Then the remaining pushes add more without re-triggering compression.
    for i in 0..20 {
        if i % 2 == 0 {
            mem.push(Message::User { content: format!("user msg {i}") });
        } else {
            mem.push(Message::Assistant { content: format!("assistant msg {i}") });
        }
    }

    let msgs = mem.messages();

    // First message must always be the system prompt
    match &msgs[0] {
        Message::System { content } => assert_eq!(content, "system prompt"),
        other => panic!("expected System message, got {other:?}"),
    }

    // Second message must always be the first user message
    match &msgs[1] {
        Message::User { content } => assert_eq!(content, "first user message"),
        other => panic!("expected User message, got {other:?}"),
    }

    // After all pushes and compressions, the total must be bounded:
    // at most 2 (preserved head) + max_context_turns (3) = 5 after a compression,
    // but can grow up to max_history (8) before the next compression.
    assert!(
        msgs.len() <= 8,
        "messages should not exceed max_history, got {}",
        msgs.len(),
    );
}

/// Verify that no compression occurs when the message count is under the limit.
#[test]
fn no_compression_under_limit() {
    let config = MemoryConfig {
        max_history: 100,
        max_context_turns: 20,
    };
    let mut mem = ShortTermMemory::new(config);

    mem.push(Message::System { content: "system".to_string() });
    mem.push(Message::User { content: "hello".to_string() });
    mem.push(Message::Assistant { content: "hi".to_string() });
    mem.push(Message::User { content: "how are you?".to_string() });
    mem.push(Message::Assistant { content: "doing well, thanks!".to_string() });

    let msgs = mem.messages();
    assert_eq!(msgs.len(), 5, "all 5 messages should be retained");

    // Verify the full conversation is intact
    match &msgs[0] {
        Message::System { content } => assert_eq!(content, "system"),
        _ => panic!("expected System"),
    }
    match &msgs[1] {
        Message::User { content } => assert_eq!(content, "hello"),
        _ => panic!("expected User"),
    }
    match &msgs[2] {
        Message::Assistant { content } => assert_eq!(content, "hi"),
        _ => panic!("expected Assistant"),
    }
    match &msgs[3] {
        Message::User { content } => assert_eq!(content, "how are you?"),
        _ => panic!("expected User"),
    }
    match &msgs[4] {
        Message::Assistant { content } => assert_eq!(content, "doing well, thanks!"),
        _ => panic!("expected Assistant"),
    }
}
