use xuanji_llm::Message;

/// Estimate token count (rough: 1 token ≈ 4 chars).
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| match m {
            Message::System { content } => content.len() / 4,
            Message::User { content } => content.len() / 4,
            Message::Assistant { content } => content.len() / 4,
            Message::AssistantToolCalls { tool_calls, content } => {
                let calls_size: usize = tool_calls.iter().map(|c| c.name.len() / 4 + 50).sum();
                let text_size = content.as_ref().map(|t| t.len() / 4).unwrap_or(0);
                calls_size + text_size
            }
            Message::ToolResult { result, .. } => result.len() / 4,
        })
        .sum()
}
