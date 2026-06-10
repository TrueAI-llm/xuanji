use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::config::ProviderConfig;
use crate::error::LlmError;
use crate::provider::LlmProvider;
use crate::types::{LlmResponse, Message, ToolCall, ToolSchema, Usage};

pub struct AnthropicProvider {
    config: ProviderConfig,
    client: Client,
}

/// Strip a trailing "/v1" from the base URL, if present.
fn strip_trailing_v1(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed[..trimmed.len() - 3].to_string()
    } else {
        trimmed.to_string()
    }
}

impl AnthropicProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// Build the request components for the Anthropic messages API.
    ///
    /// Returns `(system_prompt, body)` where `system_prompt` is separated from
    /// the message list (Anthropic requires system as a top-level field).
    pub fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> (Option<String>, Value) {
        let mut system_prompt: Option<String> = None;
        let mut messages_json: Vec<Value> = Vec::new();

        for msg in messages {
            match msg {
                Message::System { content } => {
                    system_prompt = Some(content.clone());
                }
                Message::User { content } => {
                    messages_json.push(json!({
                        "role": "user",
                        "content": content,
                    }));
                }
                Message::Assistant { content } => {
                    messages_json.push(json!({
                        "role": "assistant",
                        "content": content,
                    }));
                }
                Message::AssistantToolCalls { tool_calls, content } => {
                    let mut content_blocks: Vec<Value> = Vec::new();

                    if let Some(text) = content {
                        if !text.is_empty() {
                            content_blocks.push(json!({
                                "type": "text",
                                "text": text,
                            }));
                        }
                    }

                    for tc in tool_calls {
                        content_blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments,
                        }));
                    }

                    messages_json.push(json!({
                        "role": "assistant",
                        "content": content_blocks,
                    }));
                }
                Message::ToolResult {
                    tool_call_id,
                    tool_name,
                    result,
                    success,
                    ..
                } => {
                    let text_content = if *success {
                        result.clone()
                    } else {
                        format!("Error: {result}")
                    };

                    messages_json.push(json!({
                        "role": "user",
                        "content": [
                            {
                                "type": "tool_result",
                                "tool_use_id": tool_call_id,
                                "content": text_content,
                            }
                        ],
                    }));

                    let _ = tool_name; // used for logging/debugging if needed
                }
            }
        }

        let mut body = json!({
            "model": self.config.model,
            "messages": messages_json,
            "max_tokens": self.config.max_tokens.unwrap_or(4096),
        });

        if let Some(system) = &system_prompt {
            body["system"] = json!(system);
        }

        if let Some(temperature) = self.config.temperature {
            body["temperature"] = json!(temperature);
        }

        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            body["tools"] = json!(tools_json);
        }

        (system_prompt, body)
    }

    fn parse_response(&self, value: &Value) -> Result<LlmResponse, LlmError> {
        // Extract usage information (Anthropic uses input_tokens/output_tokens)
        let usage = value.get("usage").map(|u| {
            let prompt_tokens = u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let completion_tokens = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let total_tokens = prompt_tokens + completion_tokens;
            Usage { prompt_tokens, completion_tokens, total_tokens }
        }).unwrap_or_default();

        let content_array = value
            .get("content")
            .ok_or_else(|| LlmError::ParseError("missing content field".into()))?
            .as_array()
            .ok_or_else(|| LlmError::ParseError("content is not an array".into()))?;

        let mut text_parts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();

        for block in content_array {
            let block_type = block
                .get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("");

            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(text.to_string());
                    }
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let input = block
                        .get("input")
                        .cloned()
                        .unwrap_or(json!({}));
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
                _ => {}
            }
        }

        let text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join(""))
        };

        if !tool_calls.is_empty() {
            Ok(LlmResponse::ToolCalls {
                calls: tool_calls,
                text,
                usage,
            })
        } else {
            Ok(LlmResponse::Text {
                content: text.unwrap_or_default(),
                usage,
            })
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<LlmResponse, LlmError> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .unwrap_or("https://api.anthropic.com");
        let cleaned_base = strip_trailing_v1(base_url);
        let url = format!("{cleaned_base}/v1/messages");

        let (_system_prompt, body) = self.build_request_body(messages, tools);

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_trailing_v1() {
        assert_eq!(strip_trailing_v1("https://api.anthropic.com/v1"), "https://api.anthropic.com");
        assert_eq!(strip_trailing_v1("https://api.anthropic.com/v1/"), "https://api.anthropic.com");
        assert_eq!(strip_trailing_v1("https://api.anthropic.com"), "https://api.anthropic.com");
        assert_eq!(strip_trailing_v1("https://example.com/custom/v1"), "https://example.com/custom");
        assert_eq!(strip_trailing_v1("https://example.com"), "https://example.com");
    }
}
