use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::config::ProviderConfig;
use crate::error::LlmError;
use crate::provider::LlmProvider;
use crate::types::{LlmResponse, Message, ToolCall, ToolSchema, Usage};

pub struct OpenAIProvider {
    config: ProviderConfig,
    client: Client,
}

impl OpenAIProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Build the JSON request body for the OpenAI chat completions API.
    pub fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Value {
        let base_url = self.config.base_url.as_deref().unwrap_or("https://api.openai.com");
        let _url = format!("{base_url}/chat/completions");

        let mut messages_json: Vec<Value> = Vec::new();

        for msg in messages {
            let value = match msg {
                Message::System { content } => json!({
                    "role": "system",
                    "content": content,
                }),
                Message::User { content } => json!({
                    "role": "user",
                    "content": content,
                }),
                Message::Assistant { content } => json!({
                    "role": "assistant",
                    "content": content,
                }),
                Message::AssistantToolCalls { tool_calls, content } => {
                    let tool_calls_json: Vec<Value> = tool_calls
                        .iter()
                        .map(|tc| {
                            json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string(),
                                }
                            })
                        })
                        .collect();

                    let mut assistant_msg = json!({
                        "role": "assistant",
                        "tool_calls": tool_calls_json,
                    });
                    if let Some(text) = content {
                        assistant_msg["content"] = json!(text);
                    } else {
                        assistant_msg["content"] = Value::Null;
                    }
                    assistant_msg
                }
                Message::ToolResult {
                    tool_call_id,
                    result,
                    ..
                } => {
                    json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": result,
                    })
                }
            };
            messages_json.push(value);
        }

        let mut body = json!({
            "model": self.config.model,
            "messages": messages_json,
        });

        if let Some(max_tokens) = self.config.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }

        if let Some(temperature) = self.config.temperature {
            body["temperature"] = json!(temperature);
        }

        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools_json);
        }

        body
    }

    fn parse_response(&self, value: &Value) -> Result<LlmResponse, LlmError> {
        // Extract usage information
        let usage = value.get("usage").map(|u| {
            let prompt_tokens = u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let completion_tokens = u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let total_tokens = u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            Usage { prompt_tokens, completion_tokens, total_tokens }
        }).unwrap_or_default();

        let choices = value
            .get("choices")
            .ok_or_else(|| LlmError::ParseError("missing choices field".into()))?
            .as_array()
            .ok_or_else(|| LlmError::ParseError("choices is not an array".into()))?;

        let choice = choices
            .first()
            .ok_or_else(|| LlmError::ParseError("empty choices array".into()))?;

        let message = choice
            .get("message")
            .ok_or_else(|| LlmError::ParseError("missing message field".into()))?;

        let content: Option<String> = message
            .get("content")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());

        let tool_calls_json = message.get("tool_calls").and_then(|tc| tc.as_array());

        if let Some(tc_array) = tool_calls_json {
            let calls: Vec<ToolCall> = tc_array
                .iter()
                .filter_map(|tc| {
                    let id = tc.get("id")?.as_str()?.to_string();
                    let func = tc.get("function")?;
                    let name = func.get("name")?.as_str()?.to_string();
                    let args_str = func.get("arguments")?.as_str()?;
                    let arguments: Value =
                        serde_json::from_str(args_str).unwrap_or(json!({}));
                    Some(ToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect();

            if !calls.is_empty() {
                return Ok(LlmResponse::ToolCalls {
                    calls,
                    text: content,
                    usage,
                });
            }
        }

        Ok(LlmResponse::Text {
            content: content.unwrap_or_default(),
            usage,
        })
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<LlmResponse, LlmError> {
        let base_url = self.config.base_url.as_deref().unwrap_or("https://api.openai.com");
        let url = format!("{base_url}/chat/completions");

        let body = self.build_request_body(messages, tools);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::ApiError {
                status: status.as_u16(),
                message: error_text,
            });
        }

        let response_json: Value = response
            .json()
            .await
            .map_err(|e| LlmError::ParseError(e.to_string()))?;

        self.parse_response(&response_json)
    }

    fn config(&self) -> &ProviderConfig {
        &self.config
    }
}
