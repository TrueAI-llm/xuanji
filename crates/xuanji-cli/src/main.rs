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

    /// Initialize configuration file (non-interactive template)
    ConfigInit,

    /// Interactive setup wizard
    Init {
        /// Write to global config (~/.xuanji/config.toml) instead of local
        #[arg(long)]
        global: bool,
    },

    /// Run a YAML workflow
    Run {
        /// Path to the workflow YAML file
        workflow: String,

        /// Input parameters (key=value format, repeatable)
        #[arg(long, value_parser = parse_key_value)]
        input: Vec<String>,
    },

    /// Daemon management
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },

    /// Memory management
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },

    /// Run a multi-agent swarm task
    Swarm {
        /// Task description for multi-agent collaboration
        task: Vec<String>,

        /// Number of worker agents
        #[arg(long, default_value = "2")]
        workers: u32,
    },

    /// Show budget configuration
    Budget,

    /// Internal: run the daemon process (hidden)
    #[command(hide = true)]
    #[command(name = "_daemon_run")]
    DaemonRun {
        /// PID file path
        #[arg(long)]
        pid_file: String,

        /// Log file path
        #[arg(long)]
        log_file: String,
    },
}

#[derive(clap::Subcommand)]
enum DaemonAction {
    /// Start the daemon process
    Start,
    /// Check daemon status
    Status,
    /// Stop the daemon process
    Stop,
}

#[derive(clap::Subcommand)]
enum MemoryAction {
    /// Show current project memory
    Show,
    /// Clear current project memory
    Clear,
    /// Add a custom rule
    Rule {
        /// The rule text to add
        text: Vec<String>,
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

/// Functions accessible from command modules.
pub mod main_fns {
    use anyhow::Result;
    use xuanji_llm::ProviderConfig;

    pub fn get_default_provider(
        config: &super::config::XuanjiConfig,
    ) -> Result<(String, ProviderConfig)> {
        let provider_name = config
            .llm
            .default_provider
            .as_deref()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No default_provider set. Run 'xuanji config-init' to create a config."
                )
            })?;

        let provider_config = config
            .llm
            .providers
            .get(provider_name)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in config.", provider_name)
            })?;

        Ok((provider_name.to_string(), provider_config))
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
            let (_, provider_config) = main_fns::get_default_provider(&config)?;

            let result = commands::agent::run_agent(
                &prompt,
                &provider_config,
                &config.agent,
                &config.mcp_servers,
                &config.trigger.workflows_dir,
            )
            .await?;

            commands::agent::render_markdown(&result);
        }

        // Chat mode: xuanji chat
        (None, Some(Commands::Chat)) => {
            let (_, provider_config) = main_fns::get_default_provider(&config)?;

            commands::agent::run_chat(
                &provider_config,
                &config.agent,
                &config.mcp_servers,
                &config.trigger.workflows_dir,
            ).await?;
        }

        // MCP list
        (None, Some(Commands::Mcp { action: McpAction::List })) => {
            commands::mcp::list_tools(&config.mcp_servers).await?;
        }

        // MCP add
        (
            None,
            Some(Commands::Mcp {
                action: McpAction::Add { name, command, args, env, global },
            }),
        ) => {
            commands::mcp::add_server(&name, &command, &args, &env, global)?;
        }

        // MCP install
        (
            None,
            Some(Commands::Mcp {
                action:
                    McpAction::Install {
                        package,
                        name,
                        r#type,
                        env,
                        global,
                    },
            }),
        ) => {
            commands::mcp::install_server(
                &package,
                name.as_deref(),
                r#type.as_deref(),
                &env,
                global,
            )?;
        }

        // MCP remove
        (
            None,
            Some(Commands::Mcp {
                action: McpAction::Remove { name, global },
            }),
        ) => {
            commands::mcp::remove_server(&name, global)?;
        }

        // Config init (non-interactive template)
        (None, Some(Commands::ConfigInit)) => {
            let path = std::path::Path::new("xuanji.toml");
            if path.exists() {
                println!("xuanji.toml already exists. Use 'xuanji init' for interactive setup.");
                return Ok(());
            }
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
"#;
            std::fs::write("xuanji.toml", example)?;
            println!("Created xuanji.toml");
        }

        // Interactive init
        (None, Some(Commands::Init { global })) => {
            commands::init::run_init(global)?;
        }

        // Run workflow: xuanji run <workflow.yaml>
        (None, Some(Commands::Run { workflow, input })) => {
            let (_, provider_config) = main_fns::get_default_provider(&config)?;
            commands::workflow::run_workflow(
                &workflow,
                &input,
                &provider_config,
                &config.mcp_servers,
            )
            .await?;
        }

        // Daemon start
        (None, Some(Commands::Daemon { action: DaemonAction::Start })) => {
            commands::daemon::start_daemon()?;
        }

        // Daemon status
        (None, Some(Commands::Daemon { action: DaemonAction::Status })) => {
            commands::daemon::status_daemon()?;
        }

        // Daemon stop
        (None, Some(Commands::Daemon { action: DaemonAction::Stop })) => {
            commands::daemon::stop_daemon()?;
        }

        // Memory show
        (None, Some(Commands::Memory { action: MemoryAction::Show })) => {
            commands::memory::show_memory()?;
        }

        // Memory clear
        (None, Some(Commands::Memory { action: MemoryAction::Clear })) => {
            commands::memory::clear_memory()?;
        }

        // Memory rule add
        (None, Some(Commands::Memory { action: MemoryAction::Rule { text } })) => {
            let rule = text.join(" ");
            if rule.is_empty() {
                anyhow::bail!("Rule text cannot be empty");
            }
            commands::memory::add_rule(&rule)?;
        }

        // Swarm mode
        (None, Some(Commands::Swarm { task, workers })) => {
            let (_, provider_config) = main_fns::get_default_provider(&config)?;
            let task_str = task.join(" ");
            commands::swarm::run_swarm(
                &task_str,
                workers,
                &provider_config,
                &config.agent,
                &config.mcp_servers,
                &config.budget,
            ).await?;
        }

        // Budget status
        (None, Some(Commands::Budget)) => {
            commands::swarm::show_budget(&config.budget).await?;
        }

        // Internal: daemon run
        (
            None,
            Some(Commands::DaemonRun { pid_file, log_file }),
        ) => {
            commands::daemon::run_daemon(&pid_file, &log_file).await?;
        }

        // No args - show help
        _ => {
            Cli::parse_from(["xuanji", "--help"]);
        }
    }

    Ok(())
}
