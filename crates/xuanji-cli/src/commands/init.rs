use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, Write};

use crate::config::XuanjiConfig;
use xuanji_agent::types::AgentConfig;
use xuanji_budget::BudgetConfig;
use xuanji_llm::config::{LlmConfig, ProviderConfig};
use xuanji_llm::protocol::Protocol;
use xuanji_memory::MemoryConfig;
use xuanji_plugin::types::McpServerConfig;
use xuanji_trigger::TriggerConfig;

struct ProviderPreset {
    name: &'static str,
    protocol: Protocol,
    default_model: &'static str,
    default_base_url: &'static str,
    env_var_hint: &'static str,
}

const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        name: "OpenAI",
        protocol: Protocol::OpenAI,
        default_model: "gpt-4o",
        default_base_url: "https://api.openai.com/v1",
        env_var_hint: "OPENAI_API_KEY",
    },
    ProviderPreset {
        name: "Anthropic",
        protocol: Protocol::Anthropic,
        default_model: "claude-sonnet-4-20250514",
        default_base_url: "https://api.anthropic.com",
        env_var_hint: "ANTHROPIC_API_KEY",
    },
    ProviderPreset {
        name: "DeepSeek",
        protocol: Protocol::OpenAI,
        default_model: "deepseek-chat",
        default_base_url: "https://api.deepseek.com/v1",
        env_var_hint: "DEEPSEEK_API_KEY",
    },
];

/// Interactive setup wizard.
pub fn run_init(global: bool) -> Result<()> {
    println!();
    println!("✨ xuanji (璇玑) 初始化向导");
    println!("─────────────────────────────");
    println!();

    // Check for existing config
    let existing = if global {
        XuanjiConfig::load_global_only()?
    } else {
        XuanjiConfig::load_local_only()?
    };

    if !existing.llm.providers.is_empty() {
        let path = if global {
            "~/.xuanji/config.toml"
        } else {
            "./xuanji.toml"
        };
        println!("⚠️  检测到已有配置文件 ({})", path);
        if !prompt_yes_no("是否覆盖？", false)? {
            println!("已取消。");
            return Ok(());
        }
        println!();
    }

    // 1. Choose provider
    println!("选择 LLM 提供商:");
    for (i, preset) in PRESETS.iter().enumerate() {
        println!("  {}) {}", i + 1, preset.name);
    }
    println!("  {}) 自定义 (Custom)", PRESETS.len() + 1);
    println!();

    let choice = prompt_usize("请选择", 1, PRESETS.len() + 1)?;
    println!();

    let (provider_name, protocol, model, base_url, api_key) = if choice <= PRESETS.len() {
        // Preset provider
        let preset = &PRESETS[choice - 1];
        let provider_name = preset.name.to_lowercase();

        println!("── {} ──", preset.name);

        let api_key = prompt_api_key(preset.env_var_hint)?;
        let model = prompt_default("模型 (Model)", preset.default_model)?;
        let base_url = prompt_default("Base URL", preset.default_base_url)?;

        (provider_name, preset.protocol, model, base_url, api_key)
    } else {
        // Custom provider
        println!("── 自定义提供商 ──");

        let provider_name = prompt_required("提供商名称 (例如 zhipu, ollama)")?;
        let protocol = prompt_protocol()?;
        let api_key = prompt_api_key("API_KEY")?;
        let model = prompt_required("模型 (Model)")?;
        let base_url = prompt_required("Base URL")?;

        (provider_name, protocol, model, base_url, api_key)
    };

    println!();

    // 2. MCP servers
    let add_shell = prompt_yes_no("添加内置 Shell 工具 (shell.run)?", true)?;

    // 3. Build config
    let mut providers = HashMap::new();
    providers.insert(
        provider_name.clone(),
        ProviderConfig {
            protocol,
            model,
            api_key,
            base_url: Some(base_url),
            max_tokens: None,
            temperature: None,
        },
    );

    let mut mcp_servers = Vec::new();
    if add_shell {
        mcp_servers.push(McpServerConfig {
            name: "shell".to_string(),
            command: "xuanji-mcp-shell".to_string(),
            args: vec![],
            env: HashMap::new(),
        });
    }

    let config = XuanjiConfig {
        llm: LlmConfig {
            default_provider: Some(provider_name.clone()),
            providers,
        },
        agent: AgentConfig::default(),
        mcp_servers,
        trigger: TriggerConfig::default(),
        memory: MemoryConfig::default(),
        budget: BudgetConfig::default(),
    };

    // 4. Create directories
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let xuanji_dir = home.join(".xuanji");
    let _ = std::fs::create_dir_all(xuanji_dir.join("workflows"));
    let _ = std::fs::create_dir_all(xuanji_dir.join("memory"));

    // 5. Save config
    if global {
        config.save_global()?;
        println!();
        println!("✅ 配置已写入 ~/.xuanji/config.toml");
    } else {
        config.save_local()?;
        println!();
        println!("✅ 配置已写入 ./xuanji.toml");
    }

    println!();
    println!("下一步:");
    println!("  xuanji \"你的任务\"        # 单次执行");
    println!("  xuanji chat              # 交互式对话");
    println!("  xuanji mcp list          # 查看可用工具");
    println!();

    Ok(())
}

// ─── Helper functions ───

fn prompt(text: &str) -> Result<String> {
    print!("{}: ", text);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_default(text: &str, default: &str) -> Result<String> {
    let input = prompt(&format!("{} [{}]", text, default))?;
    Ok(if input.is_empty() {
        default.to_string()
    } else {
        input
    })
}

fn prompt_required(text: &str) -> Result<String> {
    loop {
        let input = prompt(text)?;
        if !input.is_empty() {
            return Ok(input);
        }
        println!("  ⚠️  此项不能为空，请重新输入。");
    }
}

fn prompt_yes_no(text: &str, default_yes: bool) -> Result<bool> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    let input = prompt(&format!("{} ({})", text, hint))?;
    if input.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(input.to_lowercase().as_str(), "y" | "yes"))
}

fn prompt_usize(text: &str, min: usize, max: usize) -> Result<usize> {
    loop {
        let input = prompt(&format!("{} [1-{}]", text, max))?;
        if input.is_empty() {
            return Ok(min);
        }
        if let Ok(n) = input.parse::<usize>() {
            if n >= min && n <= max {
                return Ok(n);
            }
        }
        println!("  ⚠️  请输入 {} 到 {} 之间的数字。", min, max);
    }
}

fn prompt_api_key(env_var_hint: &str) -> Result<String> {
    let input = prompt(&format!("API Key (直接输入密钥，或输入环境变量名如 {})", env_var_hint))?;
    if input.is_empty() {
        // Default to env var pattern
        return Ok(format!("${{{}}}", env_var_hint));
    }
    // If the input looks like an env var name (all uppercase + underscores, no spaces),
    // wrap it in ${...}
    if input.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !input.contains(' ') {
        Ok(format!("${{{}}}", input))
    } else {
        Ok(input)
    }
}

fn prompt_protocol() -> Result<Protocol> {
    println!("  选择协议:");
    println!("    1) openai    (OpenAI 兼容协议)");
    println!("    2) anthropic  (Anthropic 协议)");
    let choice = prompt_usize("  协议", 1, 2)?;
    match choice {
        1 => Ok(Protocol::OpenAI),
        _ => Ok(Protocol::Anthropic),
    }
}
