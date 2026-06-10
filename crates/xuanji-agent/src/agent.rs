use crate::error::AgentError;
use crate::prompt::build_system_prompt;
use crate::risk::RiskChecker;
use crate::types::{AgentConfig, ToolResult};
use xuanji_budget::BudgetController;
use xuanji_bus::state::SharedState;
use xuanji_bus::{KnowledgeBus, KnowledgeMessage};
use xuanji_llm::{LlmProvider, LlmResponse, Message, ToolCall};
use xuanji_memory::long_term::{HistoryEntry, LongTermMemory};
use xuanji_memory::short_term::ShortTermMemory;
use xuanji_memory::working::WorkingMemory;
use xuanji_memory::MemoryConfig;
use xuanji_plugin::ToolRegistry;
use std::sync::Arc;

/// The main Agent that drives the ReAct loop.
pub struct Agent {
    provider: Box<dyn LlmProvider>,
    registry: ToolRegistry,
    config: AgentConfig,
    risk_checker: RiskChecker,
    memory_config: MemoryConfig,
    /// Optional long-term memory for cross-session persistence.
    long_term_memory: Option<LongTermMemory>,
    /// Persistent short-term memory for chat mode (cross-turn).
    chat_memory: Option<ShortTermMemory>,
    /// Agent name for identification in multi-agent scenarios.
    agent_name: String,
    /// Knowledge bus for inter-agent communication.
    bus: Option<KnowledgeBus>,
    /// Budget controller for token metering.
    budget: Option<Arc<BudgetController>>,
    /// Shared state for conflict prevention.
    shared_state: Option<Arc<SharedState>>,
    /// Recursion depth (0 = top-level agent).
    depth: u32,
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
    loop {
        let start = result.find("<think");
        if let Some(s) = start {
            let end = result[s..].find("</think").map(|e| {
                let close_start = s + e + 7;
                result[close_start..].find('>').map(|gt| close_start + gt + 1).unwrap_or(close_start)
            });
            if let Some(e) = end {
                result = format!("{}{}", &result[..s], &result[e..]);
            } else {
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
fn parse_text_tool_calls(text: &str) -> Option<TextToolCall> {
    let cleaned = strip_think_blocks(text);
    if let Some(tc) = parse_action_format(&cleaned) {
        return Some(tc);
    }
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

fn parse_code_block(cleaned: &str) -> Option<TextToolCall> {
    for tag in &["```shell", "```bash", "```sh"] {
        if let Some(start) = cleaned.find(tag) {
            let code_start = start + tag.len();
            if let Some(end) = cleaned[code_start..].find("```") {
                let code = cleaned[code_start..code_start + end].trim();
                if !code.is_empty() {
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
            long_term_memory: None,
            chat_memory: None,
            agent_name: "main".to_string(),
            bus: None,
            budget: None,
            shared_state: None,
            depth: 0,
        }
    }

    /// Set long-term memory for this agent.
    pub fn with_long_term_memory(mut self, memory: LongTermMemory) -> Self {
        self.long_term_memory = Some(memory);
        self
    }

    /// Set the agent name for identification in multi-agent scenarios.
    pub fn with_name(mut self, name: &str) -> Self {
        self.agent_name = name.to_string();
        self
    }

    /// Set the knowledge bus for inter-agent communication.
    pub fn with_bus(mut self, bus: KnowledgeBus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Set the budget controller for token metering.
    pub fn with_budget(mut self, budget: Arc<BudgetController>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Set the shared state for conflict prevention.
    pub fn with_shared_state(mut self, state: Arc<SharedState>) -> Self {
        self.shared_state = Some(state);
        self
    }

    /// Set the recursion depth (0 = top-level).
    pub fn with_depth(mut self, depth: u32) -> Self {
        self.depth = depth;
        self
    }

    /// Enable chat mode: short-term memory persists across run() calls.
    pub fn enable_chat_mode(&mut self) {
        if self.chat_memory.is_none() {
            self.chat_memory = Some(ShortTermMemory::new(self.memory_config.clone()));
        }
    }

    /// Run a single-shot agent task.
    pub async fn run(&mut self, user_input: String) -> Result<String, AgentError> {
        let tools = self.registry.all_tool_schemas();
        let text_tool_mode = self.config.text_tool_mode;

        // Load long-term memory context first (before any mutable borrows)
        let memory_context = self.load_memory_context();

        // Take chat_memory out of self to avoid borrow conflicts.
        // We'll put it back at the end of this method.
        let mut short_term = self.chat_memory.take()
            .unwrap_or_else(|| ShortTermMemory::new(self.memory_config.clone()));

        // Set up bus message receiver if bus is available
        let mut bus_rx = self.bus.as_ref().map(|b| b.subscribe());

        let mut working = WorkingMemory::new();
        working.goal = Some(user_input.clone());

        let system_prompt = build_system_prompt(&tools, Some(&working), memory_context.as_deref(), text_tool_mode, None);
        short_term.push(Message::System {
            content: system_prompt,
        });
        short_term.push(Message::User {
            content: user_input.clone(),
        });

        let mut loop_count = 0;
        let mut final_result = String::new();

        loop {
            if loop_count >= self.config.max_loops {
                // Save history before returning error
                self.save_history(&user_input, "超过最大循环次数", false);
                return Err(AgentError::MaxLoopsExceeded(self.config.max_loops));
            }
            loop_count += 1;

            // Check budget before each LLM call
            if let Some(budget) = &self.budget {
                if let Err(e) = budget.acquire(&self.agent_name, 1000).await {
                    tracing::warn!("Budget exceeded for {}: {}", self.agent_name, e);
                    final_result = format!("预算超限: {}", e);
                    break;
                }
            }

            // Poll bus messages and collect any new ones
            let bus_messages = self.poll_bus_messages(&mut bus_rx);

            // Rebuild system prompt with updated working memory and bus messages
            let system_prompt = build_system_prompt(
                &tools, Some(&working), memory_context.as_deref(), text_tool_mode,
                if bus_messages.is_empty() { None } else { Some(&bus_messages) },
            );
            let messages = Self::prepare_messages(&short_term, &system_prompt);

            tracing::info!("Agent [{}] loop iteration {loop_count}", self.agent_name);
            let response = if text_tool_mode {
                self.provider.complete(&messages, &[]).await?
            } else {
                self.provider.complete(&messages, &tools).await?
            };

            // Report token usage to budget controller
            let usage = response.usage();
            if let Some(budget) = &self.budget {
                if usage.total_tokens > 0 {
                    budget.report(&self.agent_name, usage.total_tokens).await;
                }
            }

            if text_tool_mode {
                let content = response.text_content().unwrap_or("").to_string();
                tracing::debug!("LLM response text: {}", content);

                if let Some(parsed) = parse_text_tool_calls(&content) {
                    if !parsed.reasoning.is_empty() {
                        tracing::info!("Agent reasoning: {}", parsed.reasoning);
                    }
                    tracing::info!("Text tool call: {}({})", parsed.tool_name, parsed.arguments);

                    short_term.push(Message::Assistant {
                        content: content.clone(),
                    });

                    let fake_call = ToolCall {
                        id: format!("text-{}", loop_count),
                        name: parsed.tool_name.clone(),
                        arguments: parsed.arguments,
                    };
                    let result = self.execute_tool(&fake_call).await;

                    // Update working memory and publish to bus
                    if result.success {
                        working.key_results.push(format!(
                            "{}: {}",
                            result.tool_name,
                            truncate(&result.result, 200)
                        ));
                        self.publish_tool_success(&result.tool_name, &result.result);
                    } else {
                        working.errors.push(format!(
                            "{}: {}",
                            result.tool_name,
                            truncate(&result.result, 200)
                        ));
                        self.publish_tool_failure(&result.tool_name, &result.result);
                    }

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
                    final_result = content;
                    break;
                }
            } else {
                match response {
                    LlmResponse::ToolCalls { .. } => {
                        let calls = response.tool_calls().to_vec();

                        if let Some(text) = response.text_content() {
                            if !text.is_empty() {
                                tracing::info!("Agent reasoning: {}", text);
                            }
                        }

                        short_term.push(Message::AssistantToolCalls {
                            tool_calls: calls.clone(),
                            content: response.text_content().map(String::from),
                        });

                        let mut results = Vec::new();
                        for call in &calls {
                            let result = self.execute_tool(call).await;
                            results.push(result);
                        }

                        // Update working memory with results and publish to bus
                        for result in &results {
                            if result.success {
                                working.key_results.push(format!(
                                    "{}: {}",
                                    result.tool_name,
                                    truncate(&result.result, 200)
                                ));
                                self.publish_tool_success(&result.tool_name, &result.result);
                            } else {
                                working.errors.push(format!(
                                    "{}: {}",
                                    result.tool_name,
                                    truncate(&result.result, 200)
                                ));
                                self.publish_tool_failure(&result.tool_name, &result.result);
                            }
                        }

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
                        final_result = response.text_content().unwrap_or("").to_string();
                        break;
                    }
                }
            }
        }

        // Save execution history
        let success = working.errors.is_empty();
        let summary = if success {
            format!("完成，{} 个关键结果", working.key_results.len())
        } else {
            format!("完成但有 {} 个错误", working.errors.len())
        };
        self.save_history(&user_input, &summary, success);

        // Restore chat memory (was taken at the start of run())
        self.chat_memory = Some(short_term);

        Ok(final_result)
    }

    /// Poll the bus for new messages (non-blocking).
    fn poll_bus_messages(
        &self,
        bus_rx: &mut Option<tokio::sync::broadcast::Receiver<KnowledgeMessage>>,
    ) -> Vec<KnowledgeMessage> {
        let Some(rx) = bus_rx else { return Vec::new() };
        let mut messages = Vec::new();
        // Drain up to 10 messages to avoid flooding the prompt
        for _ in 0..10 {
            match rx.try_recv() {
                Ok(msg) => {
                    // Skip our own messages
                    if msg.source_agent != self.agent_name {
                        messages.push(msg);
                    }
                }
                Err(_) => break,
            }
        }
        messages
    }

    /// Publish a tool success as a discovery on the bus.
    fn publish_tool_success(&self, tool_name: &str, result: &str) {
        if let Some(bus) = &self.bus {
            bus.publish_discovery(&self.agent_name, serde_json::json!({
                "tool": tool_name,
                "text": truncate(result, 200),
            }));
        }
    }

    /// Publish a tool failure as a warning on the bus.
    fn publish_tool_failure(&self, tool_name: &str, result: &str) {
        if let Some(bus) = &self.bus {
            bus.publish_warning(&self.agent_name, serde_json::json!({
                "tool": tool_name,
                "text": truncate(result, 200),
            }));
        }
    }

    /// Execute a single tool call.
    async fn execute_tool(&self, call: &ToolCall) -> ToolResult {
        tracing::info!("Calling tool: {}({})", call.name, call.arguments);

        if self.config.confirm_risky && self.risk_checker.is_risky(&call.name, &call.arguments) {
            tracing::warn!("Risky tool call detected: {}", call.name);
        }

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
    fn prepare_messages(memory: &ShortTermMemory, system_prompt: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::System {
            content: system_prompt.to_string(),
        });
        for msg in memory.messages().iter().skip(1) {
            messages.push(msg.clone());
        }
        messages
    }

    /// Load long-term memory context as a string for prompt injection.
    fn load_memory_context(&self) -> Option<String> {
        let ltm = self.long_term_memory.as_ref()?;
        let cwd = std::env::current_dir().ok()?;
        let content = ltm.load_for_project(&cwd).ok()?;

        // Only return if there's actual content
        let ctx = LongTermMemory::to_prompt_context(&content);
        if ctx.is_empty() {
            None
        } else {
            Some(ctx)
        }
    }

    /// Save execution history to long-term memory.
    fn save_history(&self, goal: &str, summary: &str, success: bool) {
        if let Some(ltm) = &self.long_term_memory {
            if let Ok(cwd) = std::env::current_dir() {
                let timestamp = format_date();
                let entry = HistoryEntry {
                    timestamp,
                    goal: truncate(goal, 200).to_string(),
                    summary: truncate(summary, 200).to_string(),
                    success,
                };
                if let Err(e) = ltm.append_history(&cwd, entry) {
                    tracing::warn!("Failed to save history: {}", e);
                }
            }
        }
    }
}

/// Truncate a string to max_len characters.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a char boundary near max_len
        let mut end = max_len;
        while !s.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        &s[..end]
    }
}

/// Get current date as YYYY-MM-DD.
fn format_date() -> String {
    std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
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
