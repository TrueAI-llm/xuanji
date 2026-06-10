use anyhow::Result;
use std::sync::Arc;
use xuanji_agent::types::AgentConfig;
use xuanji_agent::Agent;
use xuanji_budget::BudgetController;
use xuanji_bus::state::SharedState;
use xuanji_bus::KnowledgeBus;
use xuanji_core::register_agent_delegate;
use xuanji_llm::anthropic::AnthropicProvider;
use xuanji_llm::openai::OpenAIProvider;
use xuanji_llm::{LlmProvider, ProviderConfig, Protocol};
use xuanji_memory::LongTermMemory;
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::{McpClient, ToolRegistry};

/// Run a multi-agent swarm task.
pub async fn run_swarm(
    task: &str,
    _workers: u32,
    provider_config: &ProviderConfig,
    agent_config: &AgentConfig,
    mcp_servers: &[McpServerConfig],
    budget_config: &xuanji_budget::BudgetConfig,
) -> Result<()> {
    println!("🚀 启动 Swarm 模式...");
    println!("   任务: {}", task);

    // 1. Create shared components
    let bus = KnowledgeBus::new(1024);
    let budget = Arc::new(BudgetController::new(budget_config.clone()));
    let shared_state = Arc::new(SharedState::new(bus.clone()));

    // 2. Create provider + registry
    let provider_box = create_provider(provider_config)?;
    let provider_arc: Arc<dyn xuanji_llm::LlmProvider> = Arc::from(provider_box);
    let mut registry = create_registry(mcp_servers).await?;

    // 3. Register agent.delegate system tool
    register_agent_delegate(
        &mut registry,
        provider_arc.clone(),
        agent_config.clone(),
        bus.clone(),
        budget.clone(),
        shared_state.clone(),
        0, // parent depth
        "coordinator".to_string(),
    );

    // 4. Create coordinator agent
    // Re-create a Box provider for the coordinator agent
    let coordinator_provider = create_provider(provider_config)?;
    let mut main_agent = Agent::new(coordinator_provider, registry, agent_config.clone())
        .with_name("coordinator")
        .with_bus(bus.clone())
        .with_budget(budget.clone())
        .with_shared_state(shared_state.clone())
        .with_depth(0);

    // Attach long-term memory
    if let Ok(ltm) = LongTermMemory::default_path() {
        main_agent = main_agent.with_long_term_memory(ltm);
    }

    // 5. Run
    match main_agent.run(task.to_string()).await {
        Ok(result) => println!("\n{}\n", result),
        Err(e) => println!("\n❌ 任务失败: {}\n", e),
    }

    // 6. Print budget statistics
    let status = budget.status().await;
    if status.total_budget > 0 {
        println!("📊 Token 消耗: {}/{}", status.total_consumed, status.total_budget);
    } else if status.total_consumed > 0 {
        println!("📊 Token 消耗: {} (无预算上限)", status.total_consumed);
    }
    for (agent, consumed) in &status.per_agent {
        println!("   - {}: {} tokens", agent, consumed);
    }

    Ok(())
}

/// Show current budget status.
pub async fn show_budget(budget_config: &xuanji_budget::BudgetConfig) -> Result<()> {
    let controller = BudgetController::new(budget_config.clone());
    let status = controller.status().await;

    if status.total_budget == 0 {
        println!("预算: 无限制");
    } else {
        println!("预算: {}/{} tokens (剩余 {})", status.total_consumed, status.total_budget, status.remaining);
    }
    println!("每 Agent 上限: {}", if budget_config.per_agent_budget == 0 { "无限制".to_string() } else { format!("{} tokens", budget_config.per_agent_budget) });
    println!("最大递归深度: {}", budget_config.max_depth);

    Ok(())
}

fn create_provider(config: &ProviderConfig) -> Result<Box<dyn LlmProvider>> {
    let mut config = config.clone();
    if config.api_key.starts_with("${") && config.api_key.ends_with('}') {
        let var_name = &config.api_key[2..config.api_key.len() - 1];
        config.api_key = std::env::var(var_name).unwrap_or_default();
    }

    match config.protocol {
        Protocol::OpenAI => {
            let provider = OpenAIProvider::new(config);
            Ok(Box::new(provider))
        }
        Protocol::Anthropic => {
            let provider = AnthropicProvider::new(config);
            Ok(Box::new(provider))
        }
        Protocol::Gemini => {
            anyhow::bail!("Gemini protocol not yet implemented")
        }
    }
}

async fn create_registry(mcp_servers: &[McpServerConfig]) -> Result<ToolRegistry> {
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
