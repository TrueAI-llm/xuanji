use anyhow::Result;
use std::io::{self, BufRead, Write};
use termimad::MadSkin;
use xuanji_agent::types::AgentConfig;
use xuanji_llm::anthropic::AnthropicProvider;
use xuanji_llm::openai::OpenAIProvider;
use xuanji_llm::{LlmProvider, ProviderConfig, Protocol};
use xuanji_memory::LongTermMemory;
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;
use xuanji_agent::Agent;

/// Render markdown text to the terminal using termimad.
/// Handles tables, code blocks, headings, lists, and all standard markdown.
pub fn render_markdown(text: &str) {
    let skin = MadSkin::default();
    skin.print_text(text);
}

/// Run a single-shot agent task.
pub async fn run_agent(
    prompt: &str,
    provider_config: &ProviderConfig,
    agent_config: &AgentConfig,
    mcp_servers: &[McpServerConfig],
    workflows_dir: &str,
) -> Result<String> {
    let provider = create_provider(provider_config)?;
    let mut registry = create_registry(mcp_servers).await?;

    // Register built-in system tools
    xuanji_core::register_shell_run(&mut registry);
    xuanji_core::register_workflow_create(&mut registry, workflows_dir.to_string());

    let mut agent = Agent::new(provider, registry, agent_config.clone());

    // Attach long-term memory if available
    if let Ok(ltm) = LongTermMemory::default_path() {
        agent = agent.with_long_term_memory(ltm);
    }

    let result = agent.run(prompt.to_string()).await?;
    Ok(result)
}

/// Run interactive multi-turn chat.
pub async fn run_chat(
    provider_config: &ProviderConfig,
    agent_config: &AgentConfig,
    mcp_servers: &[McpServerConfig],
    workflows_dir: &str,
) -> Result<()> {
    let provider = create_provider(provider_config)?;
    let mut registry = create_registry(mcp_servers).await?;

    // Register built-in system tools
    xuanji_core::register_shell_run(&mut registry);
    xuanji_core::register_workflow_create(&mut registry, workflows_dir.to_string());

    println!("xuanji chat (type 'exit' to quit)\n");

    let stdin = io::stdin();
    let mut agent = Agent::new(provider, registry, agent_config.clone());

    // Attach long-term memory and enable chat mode
    if let Ok(ltm) = LongTermMemory::default_path() {
        agent = agent.with_long_term_memory(ltm);
    }
    agent.enable_chat_mode();

    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;
        let input = input.trim();

        if input.is_empty() {
            continue;
        }
        if input == "exit" || input == "quit" {
            break;
        }

        match agent.run(input.to_string()).await {
            Ok(result) => {
                println!();
                render_markdown(&result);
                println!();
            }
            Err(e) => println!("\nError: {}\n", e),
        }
    }

    Ok(())
}

fn create_provider(config: &ProviderConfig) -> Result<Box<dyn LlmProvider>> {
    // Resolve env vars in api_key
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
        let mut client = xuanji_plugin::McpClient::new(process);
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
