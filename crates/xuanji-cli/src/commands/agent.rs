use anyhow::Result;
use std::io::{self, BufRead, Write};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use xuanji_agent::types::AgentConfig;
use xuanji_llm::anthropic::AnthropicProvider;
use xuanji_llm::openai::OpenAIProvider;
use xuanji_llm::{LlmProvider, ProviderConfig, Protocol};
use xuanji_memory::LongTermMemory;
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;
use xuanji_agent::Agent;

// ANSI escape codes
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";

/// Render markdown text to the terminal with colors and formatting.
/// Uses pulldown-cmark for parsing and direct ANSI codes for styling.
/// No terminal state manipulation — safe for all terminals.
pub fn render_markdown(text: &str) {
    let parser = Parser::new(text);
    let mut in_code_block = false;

    for event in parser {
        match event {
            // Headings
            Event::Start(Tag::Heading { level, .. }) => {
                match level {
                    HeadingLevel::H1 => print!("\n{BOLD}{CYAN}"),
                    HeadingLevel::H2 => print!("\n{BOLD}{YELLOW}"),
                    _ => print!("\n{BOLD}"),
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                print!("{RESET}\n");
            }

            // Code blocks
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                print!("{DIM}");
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                print!("{RESET}");
            }

            // Inline code
            Event::Code(code) => {
                print!("{YELLOW}{}{RESET}", code);
            }

            // Bold
            Event::Start(Tag::Strong) => print!("{BOLD}"),
            Event::End(TagEnd::Strong) => print!("{RESET}"),

            // Italic
            Event::Start(Tag::Emphasis) => print!("{ITALIC}"),
            Event::End(TagEnd::Emphasis) => print!("{RESET}"),

            // Links
            Event::Start(Tag::Link { .. }) => print!("{UNDERLINE}{BLUE}"),
            Event::End(TagEnd::Link) => print!("{RESET}"),

            // Lists
            Event::Start(Tag::List(None)) => {}
            Event::Start(Tag::List(Some(_))) => {}
            Event::End(TagEnd::List(false)) => {}
            Event::End(TagEnd::List(true)) => {}
            Event::Start(Tag::Item) => print!("  {GREEN}•{RESET} "),
            Event::End(TagEnd::Item) => print!("\n"),

            // Paragraphs
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => print!("\n"),

            // Block quotes
            Event::Start(Tag::BlockQuote(_)) => print!("{DIM}│ "),
            Event::End(TagEnd::BlockQuote(_)) => print!("{RESET}\n"),

            // Horizontal rule
            Event::Rule => {
                print!("{DIM}{}\n{RESET}", "─".repeat(60));
            }

            // Soft/hard breaks
            Event::SoftBreak => print!(" "),
            Event::HardBreak => print!("\n"),

            // Tables — render cells with separators
            Event::Start(Tag::Table(_)) => {}
            Event::End(TagEnd::Table) => { print!("\n"); }
            Event::Start(Tag::TableHead) => print!("{BOLD}"),
            Event::End(TagEnd::TableHead) => print!("{RESET}\n"),
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => {}
            Event::End(TagEnd::TableCell) => print!(" │ "),

            // Plain text
            Event::Text(text) => {
                if in_code_block {
                    // Preserve newlines in code blocks
                    print!("{}", text);
                } else {
                    print!("{}", text);
                }
            }

            // HTML and other events — pass through
            Event::Html(html) => print!("{}", html),
            Event::InlineHtml(html) => print!("{}", html),
            _ => {}
        }
    }
    // Flush stdout
    let _ = io::stdout().flush();
}

/// Run a single-shot agent task.
pub async fn run_agent(
    prompt: &str,
    provider_config: &ProviderConfig,
    agent_config: &AgentConfig,
    mcp_servers: &[McpServerConfig],
) -> Result<String> {
    let provider = create_provider(provider_config)?;
    let registry = create_registry(mcp_servers).await?;

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
) -> Result<()> {
    let provider = create_provider(provider_config)?;
    let registry = create_registry(mcp_servers).await?;

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
