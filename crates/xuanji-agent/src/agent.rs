use crate::error::AgentError;
use crate::prompt::build_system_prompt;
use crate::risk::RiskChecker;
use crate::types::{AgentConfig, ToolResult};
use xuanji_llm::{LlmProvider, LlmResponse, Message, ToolCall};
use xuanji_memory::short_term::ShortTermMemory;
use xuanji_memory::working::WorkingMemory;
use xuanji_memory::MemoryConfig;
use xuanji_plugin::ToolRegistry;

/// The main Agent that drives the ReAct loop.
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    registry: ToolRegistry,
    config: AgentConfig,
    risk_checker: RiskChecker,
    memory_config: MemoryConfig,
}

/// Parsed text-based tool call from LLM output.
struct TextToolCall {
    tool_name: String,
    arguments: serde_json::Value,
    /// Text before the ACTION block (reasoning)
    reasoning: String,
}

/// Strip <think...</think/> blocks from reasoning models like deepseek-r1.
fn strip_think_blocks(text: &str) -> String {
    let mut result = text.to_string();
    // Handle <think ...>...</think/> or </think > variants
    loop {
        let start = result.find("<think");
        if let Some(s) = start {
            // Find the closing tag - try </think first, then </think/>
            let end = result[s..].find("</think").map(|e| {
                // Find the '>' after </think
                let close_start = s + e + 7; // len of "</think"
                result[close_start..].find('>').map(|gt| close_start + gt + 1).unwrap_or(close_start)
            });
            if let Some(e) = end {
                result = format!("{}{}", &result[..s], &result[e..]);
            } else {
                // No closing tag found, strip from <think to end
                result = result[..s].to_string();
                break;
            }
        } else {
            break;
        }
    }
    result.trim().to_string()
}

/// Parse ACTION/PARAMS blocks from LLM text output.
/// Falls back to extracting commands from ```shell/```bash code blocks.
fn parse_text_tool_calls(text: &str) -> Option<TextToolCall> {
    // First strip any <think/> blocks
    let cleaned = strip_think_blocks(text);

    // Try ACTION/PARAMS format first
    if let Some(tc) = parse_action_format(&cleaned) {
        return Some(tc);
    }

    // Fallback: extract first command from markdown code blocks
    parse_code_block(&cleaned)
}

fn parse_action_format(cleaned: &str) -> Option<TextToolCall> {
    let action_pos = cleaned.rfind("ACTION:")?;
    let after_action = &cleaned[action_pos + 7..];

    let tool_name = after_action.lines().next().unwrap_or("").trim().to_string();
    if tool_name.is_empty() {
        return None;
    }

    let after_action_trimmed = after_action.trim_start();
    let params_pos = after_action_trimmed.find("PARAMS:")?;
    let after_params = &after_action_trimmed[params_pos + 7..];
    let params_line = after_params.lines().next().unwrap_or("{}").trim();

    let arguments: serde_json::Value =
        serde_json::from_str(params_line).unwrap_or(serde_json::json!({}));

    let reasoning = cleaned[..action_pos].trim().to_string();

    Some(TextToolCall {
        tool_name,
        arguments,
        reasoning,
    })
}

/// Parse the first ```shell or ```bash code block and use it as a shell.run call.
fn parse_code_block(cleaned: &str) -> Option<TextToolCall> {
    // Find opening ```shell or ```bash or ```sh
    for tag in &["```shell", "```bash", "```sh"] {
        if let Some(start) = cleaned.find(tag) {
            let code_start = start + tag.len();
            // Find closing ```
            if let Some(end) = cleaned[code_start..].find("```") {
                let code = cleaned[code_start..code_start + end].trim();
                if !code.is_empty() {
                    // Take only the first command (first non-empty line)
                    let cmd = code.lines()
                        .find(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .trim();
                    if !cmd.is_empty() {
                        return Some(TextToolCall {
                            tool_name: "shell.run".to_string(),
                            arguments: serde_json::json!({ "command": cmd }),
                            reasoning: cleaned[..start].trim().to_string(),
                        });
                    }
                }
            }
        }
    }
    None
}

impl Agent {
    pub fn new(
        provider: Box<dyn LlmProvider>,
        registry: ToolRegistry,
        config: AgentConfig,
    ) -> Self {
        let risk_checker = RiskChecker::new(&config);
        Self {
            provider,
            registry,
            config,
            risk_checker,
            memory_config: MemoryConfig::default(),
        }
    }

    /// Run a single-shot agent task.
    pub async fn run(&mut self, user_input: String) -> Result<String, AgentError> {
        let tools = self.registry.all_tool_schemas();
        let mut short_term = ShortTermMemory::new(self.memory_config.clone());
        let mut working = WorkingMemory::new();
        working.goal = Some(user_input.clone());

        let text_tool_mode = self.config.text_tool_mode;

        let system_prompt = build_system_prompt(&tools, Some(&working), None, text_tool_mode);
        short_term.push(Message::System {
            content: system_prompt,
        });
        short_term.push(Message::User {
            content: user_input,
        });

        let mut loop_count = 0;

        loop {
            if loop_count >= self.config.max_loops {
                return Err(AgentError::MaxLoopsExceeded(self.config.max_loops));
            }
            loop_count += 1;

            // Build system prompt with current working memory
            let system_prompt = build_system_prompt(&tools, Some(&working), None, text_tool_mode);
            let messages = self.prepare_messages(&short_term, &system_prompt);

            // Call LLM — in text_tool_mode, send empty tools slice
            tracing::info!("Agent loop iteration {loop_count}");
            let response = if text_tool_mode {
                self.provider.complete(&messages, &[]).await?
            } else {
                self.provider.complete(&messages, &tools).await?
            };

            if text_tool_mode {
                // Text-based tool calling mode
                let content = response.text_content().unwrap_or("").to_string();
                tracing::debug!("LLM response text: {}", content);

                // Check if the response contains an ACTION block
                if let Some(parsed) = parse_text_tool_calls(&content) {
                    if !parsed.reasoning.is_empty() {
                        tracing::info!("Agent reasoning: {}", parsed.reasoning);
                    }
                    tracing::info!("Text tool call: {}({})", parsed.tool_name, parsed.arguments);

                    // Store assistant message
                    short_term.push(Message::Assistant {
                        content: content.clone(),
                    });

                    // Execute the tool
                    let fake_call = ToolCall {
                        id: format!("text-{}", loop_count),
                        name: parsed.tool_name.clone(),
                        arguments: parsed.arguments,
                    };
                    let result = self.execute_tool(&fake_call).await;

                    // Feed result back as user message
                    let result_text = if result.success {
                        format!("工具 {} 执行成功:\n{}", result.tool_name, result.result)
                    } else {
                        format!("工具 {} 执行失败:\n{}", result.tool_name, result.result)
                    };
                    tracing::info!("Tool result: {}", result_text);
                    short_term.push(Message::User {
                        content: result_text,
                    });
                } else {
                    // No ACTION block — final text response
                    return Ok(content);
                }
            } else {
                // Native tool calling mode
                match response {
                    LlmResponse::ToolCalls { .. } => {
                        let calls = response.tool_calls().to_vec();

                        // Log reasoning if present
                        if let Some(text) = response.text_content() {
                            if !text.is_empty() {
                                tracing::info!("Agent reasoning: {}", text);
                            }
                        }

                        // Push assistant tool calls to history
                        short_term.push(Message::AssistantToolCalls {
                            tool_calls: calls.clone(),
                            content: response.text_content().map(String::from),
                        });

                        // Execute all tool calls sequentially
                        let mut results = Vec::new();
                        for call in &calls {
                            let result = self.execute_tool(call).await;
                            results.push(result);
                        }

                        // Push all tool results to history
                        for result in &results {
                            short_term.push(Message::ToolResult {
                                tool_call_id: result.tool_call_id.clone(),
                                tool_name: result.tool_name.clone(),
                                result: result.result.clone(),
                                success: result.success,
                            });
                        }
                    }

                    LlmResponse::Text { .. } => {
                        let content = response.text_content().unwrap_or("").to_string();
                        return Ok(content);
                    }
                }
            }
        }
    }

    /// Execute a single tool call.
    async fn execute_tool(&self, call: &ToolCall) -> ToolResult {
        tracing::info!("Calling tool: {}({})", call.name, call.arguments);

        // Risk check
        if self.config.confirm_risky && self.risk_checker.is_risky(&call.name, &call.arguments) {
            tracing::warn!("Risky tool call detected: {}", call.name);
        }

        // Execute via MCP or system tool
        match self.registry.call_tool(&call.name, call.arguments.clone()).await {
            Ok(mcp_result) => {
                let text = mcp_result
                    .content
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(String::from))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                ToolResult {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    result: text,
                    success: !mcp_result.is_error,
                }
            }
            Err(e) => ToolResult {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                result: format!("Error: {}", e),
                success: false,
            },
        }
    }

    /// Prepare messages with updated system prompt.
    fn prepare_messages(&self, memory: &ShortTermMemory, system_prompt: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::System {
            content: system_prompt.to_string(),
        });
        // Skip the first system message from memory, use the fresh one
        for msg in memory.messages().iter().skip(1) {
            messages.push(msg.clone());
        }
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_text_tool_calls_basic() {
        let text = "我来创建一个 hello world 程序。\n\nACTION: shell.run\nPARAMS: {\"command\": \"echo hello\"}\n";
        let parsed = parse_text_tool_calls(text).unwrap();
        assert_eq!(parsed.tool_name, "shell.run");
        assert_eq!(parsed.arguments["command"], "echo hello");
    }

    #[test]
    fn test_parse_text_tool_calls_no_action() {
        let text = "任务已完成！Hello world 程序已创建并运行成功。";
        assert!(parse_text_tool_calls(text).is_none());
    }

    #[test]
    fn test_parse_text_tool_calls_with_reasoning() {
        let text = "首先我需要创建文件。\n\nACTION: shell.run\nPARAMS: {\"command\": \"mkdir -p /tmp/test\"}";
        let parsed = parse_text_tool_calls(text).unwrap();
        assert!(parsed.reasoning.contains("首先"));
        assert_eq!(parsed.tool_name, "shell.run");
    }

    #[test]
    fn test_strip_think_blocks() {
        let text = "<think\n我来想想...\n</think\n\n好的，开始执行。";
        let cleaned = strip_think_blocks(text);
        assert!(cleaned.contains("好的"));
        assert!(!cleaned.contains("想想"));
    }

    #[test]
    fn test_parse_after_think_block() {
        let text = "<think\n分析中...\n</think\n\nACTION: shell.run\nPARAMS: {\"command\": \"ls\"}";
        let parsed = parse_text_tool_calls(text).unwrap();
        assert_eq!(parsed.tool_name, "shell.run");
        assert_eq!(parsed.arguments["command"], "ls");
    }

    #[test]
    fn test_strip_unclosed_think() {
        let text = "前面文字<think\n思考内容";
        let cleaned = strip_think_blocks(text);
        assert_eq!(cleaned, "前面文字");
    }

    #[test]
    fn test_parse_code_block_shell() {
        let text = "创建目录：\n```shell\nmkdir -p /tmp/hello\n```\n然后继续。";
        let parsed = parse_text_tool_calls(text).unwrap();
        assert_eq!(parsed.tool_name, "shell.run");
        assert_eq!(parsed.arguments["command"], "mkdir -p /tmp/hello");
    }

    #[test]
    fn test_parse_code_block_bash() {
        let text = "运行以下命令：\n```bash\necho hello world\n```";
        let parsed = parse_text_tool_calls(text).unwrap();
        assert_eq!(parsed.tool_name, "shell.run");
        assert_eq!(parsed.arguments["command"], "echo hello world");
    }

    #[test]
    fn test_action_format_preferred_over_code_block() {
        let text = "```shell\nls\n```\n\nACTION: shell.run\nPARAMS: {\"command\": \"pwd\"}";
        let parsed = parse_text_tool_calls(text).unwrap();
        assert_eq!(parsed.arguments["command"], "pwd");
    }
}
