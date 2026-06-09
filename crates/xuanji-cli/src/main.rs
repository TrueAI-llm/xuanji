mod commands;
mod config;

use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "xuanji")]
#[command(version, about = "AI-driven universal automation platform")]
struct Cli {
    /// Natural language task description (starts agent mode)
    prompt: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Interactive multi-turn agent chat
    Chat,

    /// MCP server management
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Initialize configuration file
    ConfigInit,
}

#[derive(clap::Subcommand)]
enum McpAction {
    /// List registered MCP servers and their tools
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("XUANJI_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = config::XuanjiConfig::load().unwrap_or_default();

    match (cli.prompt, cli.command) {
        // Agent mode: xuanji "task description"
        (Some(prompt), None) => {
            let (_, provider_config) = get_default_provider(&config)?;

            let result = commands::agent::run_agent(
                &prompt,
                &provider_config,
                &config.agent,
                &config.mcp_servers,
            )
            .await?;

            println!("{}", result);
        }

        // Chat mode: xuanji chat
        (None, Some(Commands::Chat)) => {
            let (_, provider_config) = get_default_provider(&config)?;

            commands::agent::run_chat(
                &provider_config,
                &config.agent,
                &config.mcp_servers,
            )
            .await?;
        }

        // MCP list
        (None, Some(Commands::Mcp { action: McpAction::List })) => {
            commands::mcp::list_tools(&config.mcp_servers).await?;
        }

        // Config init
        (None, Some(Commands::ConfigInit)) => {
            let example = r#"[llm]
default_provider = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
model = "deepseek-chat"
api_key = "${DEEPSEEK_API_KEY}"
base_url = "https://api.deepseek.com/v1"

[agent]
max_loops = 20
confirm_risky = true

[[mcp_server]]
name = "shell"
command = "xuanji-mcp-shell"
"#;
            std::fs::write("xuanji.toml", example)?;
            println!("Created xuanji.toml");
        }

        // No args - show help
        _ => {
            Cli::parse_from(["xuanji", "--help"]);
        }
    }

    Ok(())
}

fn get_default_provider(config: &config::XuanjiConfig) -> Result<(String, xuanji_llm::ProviderConfig)> {
    let provider_name = config
        .llm
        .default_provider
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No default_provider set. Run 'xuanji config-init' to create a config."))?;

    let provider_config = config
        .llm
        .providers
        .get(provider_name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found in config.", provider_name))?;

    Ok((provider_name.to_string(), provider_config))
}
