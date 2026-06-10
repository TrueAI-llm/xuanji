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

    /// Run a YAML workflow
    Run {
        /// Path to the workflow YAML file
        workflow: String,

        /// Input parameters (key=value format, repeatable)
        #[arg(long, value_parser = parse_key_value)]
        input: Vec<String>,
    },
}

#[derive(clap::Subcommand)]
enum McpAction {
    /// List registered MCP servers and their tools
    List,

    /// Add an MCP server to the config manually
    Add {
        /// Unique name for this MCP server
        name: String,

        /// Command to execute (e.g., "npx", "python", "/path/to/binary")
        #[arg(long)]
        command: String,

        /// Arguments to pass to the command (after --)
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,

        /// Environment variables (KEY=VALUE format, repeatable)
        #[arg(long, value_parser = parse_key_value)]
        env: Vec<String>,

        /// Save to global config (~/.xuanji/config.toml) instead of local
        #[arg(long)]
        global: bool,
    },

    /// Install an MCP server from a package identifier
    Install {
        /// Package identifier (npm: @scope/pkg, python: pkg-name)
        package: String,

        /// Override the server name (defaults to package name)
        #[arg(long)]
        name: Option<String>,

        /// Force package type: "npm" or "python"
        #[arg(long, value_name = "TYPE")]
        r#type: Option<String>,

        /// Environment variables (KEY=VALUE format, repeatable)
        #[arg(long, value_parser = parse_key_value)]
        env: Vec<String>,

        /// Save to global config instead of local
        #[arg(long)]
        global: bool,
    },

    /// Remove an MCP server from the config
    Remove {
        /// Name of the server to remove
        name: String,

        /// Remove from global config instead of local
        #[arg(long)]
        global: bool,
    },
}

fn parse_key_value(s: &str) -> Result<String, String> {
    if s.contains('=') {
        Ok(s.to_string())
    } else {
        Err(format!("Expected KEY=VALUE format, got: {}", s))
    }
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
    let config = config::XuanjiConfig::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config: {}", e);
        config::XuanjiConfig::default()
    });

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

        // MCP add
        (None, Some(Commands::Mcp {
            action: McpAction::Add { name, command, args, env, global },
        })) => {
            commands::mcp::add_server(&name, &command, &args, &env, global)?;
        }

        // MCP install
        (None, Some(Commands::Mcp {
            action: McpAction::Install { package, name, r#type, env, global },
        })) => {
            commands::mcp::install_server(&package, name.as_deref(), r#type.as_deref(), &env, global)?;
        }

        // MCP remove
        (None, Some(Commands::Mcp {
            action: McpAction::Remove { name, global },
        })) => {
            commands::mcp::remove_server(&name, global)?;
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

        // Run workflow: xuanji run <workflow.yaml>
        (None, Some(Commands::Run { workflow, input })) => {
            let (_, provider_config) = get_default_provider(&config)?;
            commands::workflow::run_workflow(
                &workflow,
                &input,
                &provider_config,
                &config.mcp_servers,
            )
            .await?;
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
