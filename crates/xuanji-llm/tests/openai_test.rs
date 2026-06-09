use xuanji_llm::{Message, OpenAIProvider, ProviderConfig, Protocol, ToolSchema};
use serde_json::json;

fn make_config() -> ProviderConfig {
    ProviderConfig {
        protocol: Protocol::OpenAI,
        model: "gpt-4".to_string(),
        api_key: "sk-test".to_string(),
        base_url: Some("https://api.openai.com".to_string()),
        max_tokens: Some(1024),
        temperature: Some(0.5),
    }
}

#[test]
fn test_build_request_body_basic_messages() {
    let provider = OpenAIProvider::new(make_config());
    let messages = vec![
        Message::System { content: "You are helpful.".into() },
        Message::User { content: "Hello!".into() },
    ];

    let body = provider.build_request_body(&messages, &[]);

    assert_eq!(body["model"], "gpt-4");
    assert_eq!(body["max_tokens"], 1024);
    assert_eq!(body["temperature"], 0.5);

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "You are helpful.");
    assert_eq!(msgs[1]["role"], "user");
    assert_eq!(msgs[1]["content"], "Hello!");
}

#[test]
fn test_build_request_body_with_tools() {
    let provider = OpenAIProvider::new(make_config());
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

    let body = provider.build_request_body(&messages, &tools);

    let tools_array = body["tools"].as_array().unwrap();
    assert_eq!(tools_array.len(), 1);
    assert_eq!(tools_array[0]["type"], "function");
    assert_eq!(tools_array[0]["function"]["name"], "get_weather");
    assert_eq!(tools_array[0]["function"]["description"], "Get current weather");
}

#[test]
fn test_build_request_body_with_tool_calls_message() {
    let provider = OpenAIProvider::new(make_config());
    let messages = vec![
        Message::User { content: "Check weather".into() },
        Message::AssistantToolCalls {
            tool_calls: vec![xuanji_llm::ToolCall {
                id: "call_123".into(),
                name: "get_weather".into(),
                arguments: json!({"location": "Tokyo"}),
            }],
            content: Some("Let me check.".into()),
        },
    ];

    let body = provider.build_request_body(&messages, &[]);

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2);

    let assistant_msg = &msgs[1];
    assert_eq!(assistant_msg["role"], "assistant");
    assert_eq!(assistant_msg["content"], "Let me check.");

    let tool_calls = assistant_msg["tool_calls"].as_array().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0]["id"], "call_123");
    assert_eq!(tool_calls[0]["type"], "function");
    assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
}

#[test]
fn test_build_request_body_with_tool_result() {
    let provider = OpenAIProvider::new(make_config());
    let messages = vec![
        Message::ToolResult {
            tool_call_id: "call_456".into(),
            tool_name: "get_weather".into(),
            result: "Sunny, 22C".into(),
            success: true,
        },
    ];

    let body = provider.build_request_body(&messages, &[]);

    let msgs = body["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["role"], "tool");
    assert_eq!(msgs[0]["tool_call_id"], "call_456");
    assert_eq!(msgs[0]["content"], "Sunny, 22C");
}

#[test]
fn test_build_request_body_no_tools_field_when_empty() {
    let provider = OpenAIProvider::new(make_config());
    let messages = vec![Message::User { content: "Hi".into() }];

    let body = provider.build_request_body(&messages, &[]);

    assert!(body.get("tools").is_none());
}
