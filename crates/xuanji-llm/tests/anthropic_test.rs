use xuanji_llm::{AnthropicProvider, Message, ProviderConfig, Protocol, ToolSchema};
use serde_json::json;

fn make_config() -> ProviderConfig {
    ProviderConfig {
        protocol: Protocol::Anthropic,
        model: "claude-sonnet-4-20250514".to_string(),
        api_key: "sk-ant-test".to_string(),
        base_url: None,
        max_tokens: Some(2048),
        temperature: None,
    }
}

#[test]
fn test_build_request_body_system_prompt_separated() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::System { content: "You are helpful.".into() },
        Message::User { content: "Hello!".into() },
    ];

    let (system_prompt, body) = provider.build_request_body(&messages, &[]);

    // System prompt should be extracted into the return value
    assert_eq!(system_prompt, Some("You are helpful.".into()));
    // System prompt should also be set as a top-level field in the body
    assert_eq!(body["system"], "You are helpful.");

    // No "system" role should appear in messages
    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "user");
    assert_eq!(msgs[0]["content"], "Hello!");
}

#[test]
fn test_build_request_body_no_system_prompt() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::User { content: "Hello!".into() },
    ];

    let (system_prompt, body) = provider.build_request_body(&messages, &[]);

    assert_eq!(system_prompt, None);
    assert!(body.get("system").is_none());

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
}

#[test]
fn test_build_request_body_assistant_tool_calls() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::User { content: "Check weather".into() },
        Message::AssistantToolCalls {
            tool_calls: vec![xuanji_llm::ToolCall {
                id: "toolu_123".into(),
                name: "get_weather".into(),
                arguments: json!({"location": "Tokyo"}),
            }],
            content: Some("Let me check.".into()),
        },
    ];

    let (_system, body) = provider.build_request_body(&messages, &[]);

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);

    // Assistant message should have content blocks
    let assistant_msg = &msgs[1];
    assert_eq!(assistant_msg["role"], "assistant");
    let content_blocks = assistant_msg["content"].as_array().unwrap();

    // First block should be text
    assert_eq!(content_blocks[0]["type"], "text");
    assert_eq!(content_blocks[0]["text"], "Let me check.");

    // Second block should be tool_use
    assert_eq!(content_blocks[1]["type"], "tool_use");
    assert_eq!(content_blocks[1]["id"], "toolu_123");
    assert_eq!(content_blocks[1]["name"], "get_weather");
    assert_eq!(content_blocks[1]["input"]["location"], "Tokyo");
}

#[test]
fn test_build_request_body_tool_result() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::ToolResult {
            tool_call_id: "toolu_456".into(),
            tool_name: "get_weather".into(),
            result: "Sunny, 22C".into(),
            success: true,
        },
    ];

    let (_system, body) = provider.build_request_body(&messages, &[]);

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);

    // Tool result is sent as user role with content blocks
    let msg = &msgs[0];
    assert_eq!(msg["role"], "user");
    let content_blocks = msg["content"].as_array().unwrap();
    assert_eq!(content_blocks[0]["type"], "tool_result");
    assert_eq!(content_blocks[0]["tool_use_id"], "toolu_456");
    assert_eq!(content_blocks[0]["content"], "Sunny, 22C");
}

#[test]
fn test_build_request_body_tool_result_error() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::ToolResult {
            tool_call_id: "toolu_789".into(),
            tool_name: "get_weather".into(),
            result: "Connection refused".into(),
            success: false,
        },
    ];

    let (_system, body) = provider.build_request_body(&messages, &[]);

    let msgs = body["messages"].as_array().unwrap();
    let content_blocks = msgs[0]["content"].as_array().unwrap();
    assert_eq!(content_blocks[0]["content"], "Error: Connection refused");
}

#[test]
fn test_build_request_body_with_tools() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::User { content: "Use the tool".into() },
    ];
    let tools = vec![ToolSchema {
        name: "get_weather".into(),
        description: "Get current weather".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "location": { "type": "string" }
            },
            "required": ["location"]
        }),
    }];

    let (_system, body) = provider.build_request_body(&messages, &tools);

    let tools_array = body["tools"].as_array().unwrap();
    assert_eq!(tools_array.len(), 1);
    // Anthropic format: name, description, input_schema (no wrapping "function" key)
    assert_eq!(tools_array[0]["name"], "get_weather");
    assert_eq!(tools_array[0]["description"], "Get current weather");
    assert!(tools_array[0].get("input_schema").is_some());
    // Should NOT have "type": "function" wrapper like OpenAI
    assert!(tools_array[0].get("type").is_none());
}

#[test]
fn test_no_system_role_in_messages() {
    let provider = AnthropicProvider::new(make_config());
    let messages = vec![
        Message::System { content: "System message".into() },
        Message::User { content: "User message".into() },
        Message::System { content: "Another system".into() },
    ];

    let (_system, body) = provider.build_request_body(&messages, &[]);

    let msgs = body["messages"].as_array().unwrap();
    for msg in msgs {
        assert_ne!(
            msg["role"].as_str().unwrap(),
            "system",
            "No message with 'system' role should appear in the messages array"
        );
    }
}

#[test]
fn test_default_max_tokens() {
    let config = ProviderConfig {
        protocol: Protocol::Anthropic,
        model: "claude-sonnet-4-20250514".to_string(),
        api_key: "sk-ant-test".to_string(),
        base_url: None,
        max_tokens: None,
        temperature: None,
    };
    let provider = AnthropicProvider::new(config);
    let messages = vec![Message::User { content: "Hi".into() }];

    let (_system, body) = provider.build_request_body(&messages, &[]);

    // Anthropic requires max_tokens; defaults to 4096 if not set
    assert_eq!(body["max_tokens"], 4096);
}
