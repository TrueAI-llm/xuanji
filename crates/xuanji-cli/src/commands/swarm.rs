use anyhow::Result;
use std::sync::Arc;
use xuanji_agent::types::AgentConfig;
use xuanji_agent::Agent;
use xuanji_budget::BudgetController;
use xuanji_bus::state::SharedState;
use xuanji_bus::KnowledgeBus;
use xuanji_core::register_agent_delegate;
use xuanji_llm::{ArcProvider, LlmProvider, ProviderConfig};
use xuanji_memory::LongTermMemory;
use xuanji_plugin::types::McpServerConfig;

use super::runtime::{create_provider_arc, create_registry, render_markdown};

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

    // 2. Create shared provider + registry
    let provider_arc = create_provider_arc(provider_config)?;
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

    // 4. Create coordinator agent (share the provider via ArcProvider)
    let coordinator_provider: Box<dyn LlmProvider> = Box::new(ArcProvider(provider_arc.clone()));
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
        Ok(result) => {
            println!();
            render_markdown(&result.text);
            println!();
        }
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
