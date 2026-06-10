use std::sync::Arc;
use async_trait::async_trait;
use xuanji_agent::types::AgentConfig;
use xuanji_agent::Agent;
use xuanji_budget::BudgetController;
use xuanji_bus::state::SharedState;
use xuanji_bus::KnowledgeBus;
use xuanji_llm::config::ProviderConfig;
use xuanji_llm::error::LlmError;
use xuanji_llm::types::{LlmResponse, Message, ToolSchema};
use xuanji_llm::LlmProvider;
use xuanji_plugin::ToolRegistry;

/// Wrapper to use `Arc<dyn LlmProvider>` as `Box<dyn LlmProvider>`.
struct ArcProvider(Arc<dyn LlmProvider>);

#[async_trait]
impl LlmProvider for ArcProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<LlmResponse, LlmError> {
        self.0.complete(messages, tools).await
    }

    fn config(&self) -> &ProviderConfig {
        self.0.config()
    }
}

/// Schema for llm.ask tool
pub const LLM_ASK_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "prompt": { "type": "string", "description": "The prompt to send to the LLM" },
        "provider": { "type": "string", "description": "Provider name override (optional)" },
        "model": { "type": "string", "description": "Model name override (optional)" },
        "temperature": { "type": "number", "description": "Temperature override (optional)" },
        "max_tokens": { "type": "integer", "description": "Max tokens override (optional)" }
    },
    "required": ["prompt"]
}"#;

/// Schema for agent.delegate tool
pub const AGENT_DELEGATE_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "task": { "type": "string", "description": "Description of the sub-task to delegate to a sub-agent" },
        "agent_name": { "type": "string", "description": "Name for the sub-agent (default: auto-generated)" }
    },
    "required": ["task"]
}"#;

/// Register system tools (llm.ask) with the tool registry.
pub fn register_system_tools(
    registry: &mut ToolRegistry,
    provider: Arc<dyn LlmProvider>,
) {
    let provider_clone = provider.clone();

    registry.register_system_tool(
        "llm.ask",
        "Ask an LLM a question and get a response. Useful for analysis, summarization, and decision-making within workflows.",
        serde_json::from_str(LLM_ASK_SCHEMA).unwrap_or_default(),
        move |args: serde_json::Value| {
            let provider = provider_clone.clone();
            Box::pin(async move {
                execute_llm_ask(args, &*provider).await
            })
        },
    );
}

/// Register agent.delegate system tool.
///
/// This allows an agent to spawn a sub-agent to handle a sub-task.
/// The sub-agent shares the same provider, bus, budget, and shared state.
pub fn register_agent_delegate(
    registry: &mut ToolRegistry,
    provider: Arc<dyn LlmProvider>,
    agent_config: AgentConfig,
    bus: KnowledgeBus,
    budget: Arc<BudgetController>,
    shared_state: Arc<SharedState>,
    parent_depth: u32,
    parent_name: String,
) {
    registry.register_system_tool(
        "agent.delegate",
        "Delegate a sub-task to a sub-agent. The sub-agent will execute the task autonomously and return the result.",
        serde_json::from_str(AGENT_DELEGATE_SCHEMA).unwrap_or_default(),
        move |args: serde_json::Value| {
            let provider = provider.clone();
            let agent_config = agent_config.clone();
            let bus = bus.clone();
            let budget = budget.clone();
            let shared_state = shared_state.clone();
            let parent_depth = parent_depth;
            let parent_name = parent_name.clone();

            Box::pin(async move {
                execute_agent_delegate(
                    args,
                    provider,
                    agent_config,
                    bus,
                    budget,
                    shared_state,
                    parent_depth,
                    &parent_name,
                ).await
            })
        },
    );
}

async fn execute_llm_ask(
    arguments: serde_json::Value,
    provider: &dyn LlmProvider,
) -> Result<xuanji_plugin::client::McpToolResult, xuanji_plugin::PluginError> {
    let prompt = arguments
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| xuanji_plugin::PluginError::Protocol("llm.ask: missing 'prompt' field".into()))?;

    let messages = vec![Message::User {
        content: prompt.to_string(),
    }];

    let response = provider
        .complete(&messages, &[])
        .await
        .map_err(|e| xuanji_plugin::PluginError::Protocol(format!("LLM error: {}", e)))?;

    let text = response.text_content().unwrap_or("").to_string();

    Ok(xuanji_plugin::client::McpToolResult {
        content: serde_json::json!([{ "type": "text", "text": text }]),
        is_error: false,
    })
}

async fn execute_agent_delegate(
    arguments: serde_json::Value,
    provider: Arc<dyn LlmProvider>,
    agent_config: AgentConfig,
    bus: KnowledgeBus,
    budget: Arc<BudgetController>,
    shared_state: Arc<SharedState>,
    parent_depth: u32,
    parent_name: &str,
) -> Result<xuanji_plugin::client::McpToolResult, xuanji_plugin::PluginError> {
    let task = arguments
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| xuanji_plugin::PluginError::Protocol("agent.delegate: missing 'task' field".into()))?;

    let child_name = arguments
        .get("agent_name")
        .and_then(|v| v.as_str())
        .unwrap_or("worker");

    let child_depth = parent_depth + 1;

    // Check depth limit
    if child_depth > budget.config().max_depth {
        return Ok(xuanji_plugin::client::McpToolResult {
            content: serde_json::json!([{ "type": "text", "text": format!("无法委派：递归深度 {} 超过最大限制 {}", child_depth, budget.config().max_depth) }]),
            is_error: true,
        });
    }

    let full_name = format!("{}-{}", parent_name, child_name);
    tracing::info!("Spawning sub-agent '{}' at depth {} for task: {}", full_name, child_depth, task);

    // Create a new tool registry for the sub-agent (empty — sub-agents don't get MCP tools by default)
    let sub_registry = ToolRegistry::new();

    // Create the sub-agent
    // Wrap Arc in a newtype that implements LlmProvider via Arc delegation
    let provider_box: Box<dyn LlmProvider> = Box::new(ArcProvider(provider));
    let mut sub_agent = Agent::new(
        provider_box,
        sub_registry,
        agent_config,
    )
        .with_name(&full_name)
        .with_bus(bus)
        .with_budget(budget)
        .with_shared_state(shared_state)
        .with_depth(child_depth);

    match sub_agent.run(task.to_string()).await {
        Ok(result) => {
            tracing::info!("Sub-agent '{}' completed successfully", full_name);
            Ok(xuanji_plugin::client::McpToolResult {
                content: serde_json::json!([{ "type": "text", "text": result }]),
                is_error: false,
            })
        }
        Err(e) => {
            tracing::warn!("Sub-agent '{}' failed: {}", full_name, e);
            Ok(xuanji_plugin::client::McpToolResult {
                content: serde_json::json!([{ "type": "text", "text": format!("子 Agent '{}' 执行失败: {}", full_name, e) }]),
                is_error: true,
            })
        }
    }
}
