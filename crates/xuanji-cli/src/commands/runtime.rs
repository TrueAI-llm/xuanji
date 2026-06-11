//! Shared runtime construction helpers: provider, tool registry, markdown rendering.
//!
//! Extracted from `agent.rs` / `swarm.rs` / `workflow.rs` so every entry point
//! (god, role, swarm, workflow, legacy agent) constructs providers and registries
//! the same way. Single source of truth — no more duplicated `create_provider`.

use crate::config::XuanjiConfig;
use anyhow::Result;
use std::sync::Arc;
use termimad::MadSkin;
use xuanji_agent::types::AgentConfig;
use xuanji_agent::Agent;
use xuanji_llm::anthropic::AnthropicProvider;
use xuanji_llm::openai::OpenAIProvider;
use xuanji_llm::{ArcProvider, LlmProvider, ProviderConfig, Protocol};
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::{McpClient, ToolRegistry};
use xuanji_role::AgentFactory;

/// Resolve a `${VAR}` api_key from the environment, returning an owned config.
fn resolve_env(config: &ProviderConfig) -> ProviderConfig {
    let mut config = config.clone();
    if config.api_key.starts_with("${") && config.api_key.ends_with('}') {
        let var_name = &config.api_key[2..config.api_key.len() - 1];
        config.api_key = std::env::var(var_name).unwrap_or_default();
    }
    config
}

/// Create a shared `Arc<dyn LlmProvider>` from config.
///
/// Use this when the provider must back multiple consumers (an agent + orchestration
/// calls + sub-agents). Clone the `Arc` for each consumer; wrap in `ArcProvider` when
/// an owned `Box<dyn LlmProvider>` is needed (e.g. for `Agent::new`).
pub fn create_provider_arc(config: &ProviderConfig) -> Result<Arc<dyn LlmProvider>> {
    let config = resolve_env(config);
    match config.protocol {
        Protocol::OpenAI => Ok(Arc::new(OpenAIProvider::new(config))),
        Protocol::Anthropic => Ok(Arc::new(AnthropicProvider::new(config))),
        Protocol::Gemini => anyhow::bail!("Gemini protocol not yet implemented"),
    }
}

/// Start every configured MCP server and register its tools. Failures are logged
/// and skipped (a missing optional server shouldn't abort startup).
pub async fn create_registry(mcp_servers: &[McpServerConfig]) -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();
    for server_config in mcp_servers {
        let process = McpProcess::new(server_config.clone());
        let mut client = McpClient::new(process);
        match client.initialize().await {
            Ok(()) => {
                registry.register_server(client).await?;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to start MCP server '{}': {}. Skipping.",
                    server_config.name,
                    e
                );
            }
        }
    }
    Ok(registry)
}

/// Render markdown text to the terminal (tables, code blocks, headings, lists).
pub fn render_markdown(text: &str) {
    let skin = MadSkin::default();
    skin.print_text(text);
}

/// Build a ready-to-run [`Agent`] for a role: shared provider (via `ArcProvider`),
/// full MCP registry + built-in tools, with the role's persona and memory context
/// injected. When `chat` is true, enables cross-turn short-term memory.
pub async fn build_agent(
    provider: &Arc<dyn LlmProvider>,
    config: &XuanjiConfig,
    persona: &str,
    memory_context: &str,
    chat: bool,
) -> Result<Agent> {
    let provider_box: Box<dyn LlmProvider> = Box::new(ArcProvider(provider.clone()));
    let mut registry = create_registry(&config.mcp_servers).await?;
    xuanji_core::register_shell_run(&mut registry);
    xuanji_core::register_workflow_create(&mut registry, config.trigger.workflows_dir.clone());

    let mut agent = Agent::new(provider_box, registry, config.agent.clone())
        .with_persona(persona)
        .with_memory_context(memory_context.to_string());
    if chat {
        agent.enable_chat_mode();
    }
    Ok(agent)
}

/// CLI implementation of [`AgentFactory`]. Builds a worker agent that shares the
/// provider and config; workers get built-in tools (shell.run + workflow.create) and
/// the role's persona + memory context. (MCP servers are reserved for the top-level
/// role agent to avoid spawning a process pool per worker.)
pub struct CliAgentFactory {
    provider: Arc<dyn LlmProvider>,
    agent_config: AgentConfig,
    workflows_dir: String,
}

impl CliAgentFactory {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        agent_config: AgentConfig,
        workflows_dir: String,
    ) -> Self {
        Self {
            provider,
            agent_config,
            workflows_dir,
        }
    }
}

impl AgentFactory for CliAgentFactory {
    fn build(&self, _role_name: &str, persona: &str, memory_context: &str) -> Agent {
        let provider_box: Box<dyn LlmProvider> = Box::new(ArcProvider(self.provider.clone()));
        let mut registry = ToolRegistry::new();
        xuanji_core::register_shell_run(&mut registry);
        xuanji_core::register_workflow_create(&mut registry, self.workflows_dir.clone());

        Agent::new(provider_box, registry, self.agent_config.clone())
            .with_persona(persona)
            .with_memory_context(memory_context.to_string())
    }
}
