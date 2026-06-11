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

/// Markdown renderer state tracker for proper nesting.
struct RenderState {
    /// Stack of active styles (bold, italic, color, etc.)
    style_stack: Vec<&'static str>,
    in_code_block: bool,
    in_table_head: bool,
    first_cell_in_row: bool,
    list_depth: usize,
}

impl RenderState {
    fn new() -> Self {
        Self {
            style_stack: Vec::new(),
            in_code_block: false,
            in_table_head: false,
            first_cell_in_row: true,
            list_depth: 0,
        }
    }

    /// Push a style and emit it.
    fn push_style(&mut self, style: &'static str) {
        self.style_stack.push(style);
        print!("{}", style);
    }

    /// Pop the top style, emit RESET, then re-emit all remaining styles.
    fn pop_style(&mut self) {
        self.style_stack.pop();
        print!("{}", RESET);
        for s in &self.style_stack {
            print!("{}", s);
        }
    }
}

/// Render markdown text to the terminal with colors and formatting.
/// Uses pulldown-cmark for parsing and direct ANSI codes for styling.
/// No terminal state manipulation — safe for all terminals.
pub fn render_markdown(text: &str) {
    let parser = Parser::new(text);
    let mut st = RenderState::new();

    for event in parser {
        match event {
            // ── Headings ──
            Event::Start(Tag::Heading { level, .. }) => {
                print!("\n");
                match level {
                    HeadingLevel::H1 => { st.push_style(BOLD); st.push_style(CYAN); }
                    HeadingLevel::H2 => { st.push_style(BOLD); st.push_style(YELLOW); }
                    HeadingLevel::H3 => { st.push_style(BOLD); st.push_style(GREEN); }
                    _ => { st.push_style(BOLD); }
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                // Pop all styles pushed by heading (color + bold)
                while st.style_stack.len() > 0 {
                    st.pop_style();
                }
                print!("\n");
            }

            // ── Code blocks ──
            Event::Start(Tag::CodeBlock(kind)) => {
                st.in_code_block = true;
                print!("\n{DIM}┌");
                if let pulldown_cmark::CodeBlockKind::Fenced(lang) = kind {
                    if !lang.is_empty() {
                        print!(" {lang} ");
                    }
                }
                println!("─{}", "─".repeat(58));
            }
            Event::End(TagEnd::CodeBlock) => {
                st.in_code_block = false;
                println!("{DIM}└{}{RESET}", "─".repeat(60));
            }

            // ── Inline code ──
            Event::Code(code) => {
                print!("{YELLOW}`{}`{RESET}", code);
                // Re-emit active styles after inline code reset
                for s in &st.style_stack {
                    print!("{}", s);
                }
            }

            // ── Bold ──
            Event::Start(Tag::Strong) => st.push_style(BOLD),
            Event::End(TagEnd::Strong) => st.pop_style(),

            // ── Italic ──
            Event::Start(Tag::Emphasis) => st.push_style(ITALIC),
            Event::End(TagEnd::Emphasis) => st.pop_style(),

            // ── Strikethrough ──
            Event::Start(Tag::Strikethrough) => st.push_style(DIM),
            Event::End(TagEnd::Strikethrough) => st.pop_style(),

            // ── Links ──
            Event::Start(Tag::Link { .. }) => {
                st.push_style(UNDERLINE);
                st.push_style(BLUE);
            }
            Event::End(TagEnd::Link) => {
                st.pop_style(); // blue
                st.pop_style(); // underline
            }

            // ── Lists ──
            Event::Start(Tag::List(_)) => {
                st.list_depth += 1;
            }
            Event::End(TagEnd::List(_)) => {
                st.list_depth = st.list_depth.saturating_sub(1);
                if st.list_depth == 0 {
                    print!("\n");
                }
            }
            Event::Start(Tag::Item) => {
                let indent = "  ".repeat(st.list_depth.saturating_sub(1));
                print!("{}{GREEN}•{RESET} ", indent);
            }
            Event::End(TagEnd::Item) => {
                print!("\n");
            }

            // ── Paragraphs ──
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                print!("\n");
            }

            // ── Block quotes ──
            Event::Start(Tag::BlockQuote(_)) => {
                st.push_style(DIM);
                print!("│ ");
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                st.pop_style();
                print!("\n");
            }

            // ── Horizontal rule ──
            Event::Rule => {
                println!("{DIM}{}{RESET}", "─".repeat(60));
            }

            // ── Soft/hard breaks ──
            Event::SoftBreak => print!(" "),
            Event::HardBreak => print!("\n"),

            // ── Tables ──
            Event::Start(Tag::Table(_)) => {}
            Event::End(TagEnd::Table) => {
                print!("\n");
            }
            Event::Start(Tag::TableHead) => {
                st.in_table_head = true;
                st.first_cell_in_row = true;
                st.push_style(BOLD);
            }
            Event::End(TagEnd::TableHead) => {
                st.pop_style();
                st.in_table_head = false;
                // Print separator line after header
                print!("\n{DIM}{}{RESET}", "─".repeat(60));
            }
            Event::Start(Tag::TableRow) => {
                st.first_cell_in_row = true;
            }
            Event::End(TagEnd::TableRow) => {}
            Event::Start(Tag::TableCell) => {
                if st.first_cell_in_row {
                    st.first_cell_in_row = false;
                    print!(" ");
                } else {
                    print!(" │ ");
                }
            }
            Event::End(TagEnd::TableCell) => {}

            // ── Plain text ──
            Event::Text(text) => {
                print!("{}", text);
            }

            // ── FootnoteReference ──
            Event::FootnoteReference(name) => {
                print!("{DIM}[{}]{RESET}", name);
            }

            // ── HTML — pass through ──
            Event::Html(html) | Event::InlineHtml(html) => {
                print!("{}", html);
            }

            // ── Ignore unknown events ──
            _ => {}
        }
    }
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
