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

        // Initial system prompt
        let system_prompt = build_system_prompt(&tools, Some(&working), None);
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
            let system_prompt = build_system_prompt(&tools, Some(&working), None);
            let messages = self.prepare_messages(&short_term, &system_prompt);

            // Call LLM
            tracing::info!("Agent loop iteration {loop_count}");
            let response = self.provider.complete(&messages, &tools).await?;

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
                    // TODO: Parallel execution requires restructuring ToolRegistry
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

    /// Execute a single tool call.
    async fn execute_tool(&self, call: &ToolCall) -> ToolResult {
        tracing::info!("Calling tool: {}({})", call.name, call.arguments);

        // Risk check
        if self.config.confirm_risky && self.risk_checker.is_risky(&call.name, &call.arguments) {
            tracing::warn!("Risky tool call detected: {}", call.name);
        }

        // Check if it's a system tool
        if let Some(entry) = self.registry.get_tool(&call.name) {
            if matches!(entry.source, xuanji_plugin::ToolSource::System { .. }) {
                return ToolResult {
                    tool_call_id: call.id.clone(),
                    tool_name: call.name.clone(),
                    result: "System tool not yet implemented in MVP".into(),
                    success: false,
                };
            }
        }

        // Execute via MCP
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
