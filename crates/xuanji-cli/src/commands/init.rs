use anyhow::Result;
use std::collections::HashMap;
use std::io::{self, Write};

use crate::config::{RoleCliConfig, XuanjiConfig};
use xuanji_agent::types::AgentConfig;
use xuanji_budget::BudgetConfig;
use xuanji_llm::config::{LlmConfig, ProviderConfig};
use xuanji_llm::protocol::Protocol;
use xuanji_memory::MemoryConfig;
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
/// Default: write to global config (~/.xuanji/config.toml).
/// With --local: write to project-local ./xuanji.toml.
pub fn run_init(local: bool) -> Result<()> {
    let config_path_label = if local { "./xuanji.toml" } else { "~/.xuanji/config.toml" };

    // Load existing config (if any)
    let existing = if local {
        XuanjiConfig::load_local_only()?
    } else {
        XuanjiConfig::load_global_only()?
    };

    let has_existing = !existing.llm.providers.is_empty();

    if has_existing {
        println!();
        println!("检测到已有配置 ({})", config_path_label);
        if let Some(ref default) = existing.llm.default_provider {
            println!("  当前默认提供商: {}", default);
            let provider_names: Vec<&str> = existing.llm.providers.keys().map(|s| s.as_str()).collect();
            println!("  已配置提供商: {}", provider_names.join(", "));
        }
        println!();
        println!("选择操作:");
        println!("  1) 更新默认提供商 / API Key / 模型");
        println!("  2) 添加新的提供商");
        println!("  3) 完全覆盖（重新配置）");
        println!("  4) 取消");
        println!();

        let action = prompt_usize("请选择", 1, 4)?;
        match action {
            1 => return run_update(&existing, local),
            2 => return run_add_provider(&existing, local),
            3 => { /* fall through to full setup */ }
            _ => {
                println!("已取消。");
                return Ok(());
            }
        }
    }

    // ─── Full setup (no existing config, or user chose overwrite) ───

    println!();
    println!("xuanji init");
    println!("────────────");
    println!();

    let (provider_name, protocol, model, base_url, api_key) = choose_provider()?;

    println!();

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

    let config = XuanjiConfig {
        llm: LlmConfig {
            default_provider: Some(provider_name.clone()),
            providers,
        },
        agent: AgentConfig::default(),
        mcp_servers: vec![],
        trigger: TriggerConfig::default(),
        memory: MemoryConfig::default(),
        budget: BudgetConfig::default(),
        role: RoleCliConfig::default(),
    };

    save_config(&config, local)
}

/// Update existing provider: change API key, model, or base_url.
fn run_update(existing: &XuanjiConfig, local: bool) -> Result<()> {
    let default_name = existing.llm.default_provider.as_deref().unwrap_or("unknown");
    let provider_names: Vec<&str> = existing.llm.providers.keys().map(|s| s.as_str()).collect();

    println!();
    println!("── 更新提供商 ──");
    println!();

    // If multiple providers, ask which to update
    let update_name = if provider_names.len() == 1 {
        provider_names[0].to_string()
    } else {
        println!("选择要更新的提供商:");
        for (i, name) in provider_names.iter().enumerate() {
            let is_default = *name == default_name;
            println!("  {}) {}{}", i + 1, name, if is_default { " (默认)" } else { "" });
        }
        let choice = prompt_usize("请选择", 1, provider_names.len())?;
        provider_names[choice - 1].to_string()
    };

    let current = existing.llm.providers.get(&update_name)
        .ok_or_else(|| anyhow::anyhow!("Provider '{}' not found", update_name))?;

    println!();
    println!("当前配置 ({}):", update_name);
    println!("  模型:     {}", current.model);
    println!("  Base URL: {}", current.base_url.as_deref().unwrap_or("(none)"));
    println!("  API Key:  {}", mask_api_key(&current.api_key));
    println!();

    let new_api_key = prompt_api_key_with_default("API Key", &current.api_key)?;
    let new_model = prompt_default("模型 (Model)", &current.model)?;
    let new_base_url = match &current.base_url {
        Some(url) => Some(prompt_default("Base URL", url)?),
        None => {
            let input = prompt("Base URL (留空跳过)")?;
            if input.is_empty() { current.base_url.clone() } else { Some(input) }
        }
    };

    // Clone config and update
    let mut config = existing.clone_for_save();
    if let Some(provider) = config.llm.providers.get_mut(&update_name) {
        provider.api_key = new_api_key;
        provider.model = new_model;
        provider.base_url = new_base_url;
    }

    save_config(&config, local)
}

/// Add a new provider to existing config.
fn run_add_provider(existing: &XuanjiConfig, local: bool) -> Result<()> {
    println!();
    println!("── 添加提供商 ──");
    println!();

    let (provider_name, protocol, model, base_url, api_key) = choose_provider()?;

    println!();

    let mut config = existing.clone_for_save();
    config.llm.providers.insert(
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

    // Ask if this should be the new default
    let make_default = prompt_yes_no(&format!("设为默认提供商？(当前: {})",
        existing.llm.default_provider.as_deref().unwrap_or("无")), false)?;
    if make_default {
        config.llm.default_provider = Some(provider_name);
    }

    save_config(&config, local)
}

/// Provider selection flow (shared between full setup and add provider).
fn choose_provider() -> Result<(String, Protocol, String, String, String)> {
    println!("选择 LLM 提供商:");
    for (i, preset) in PRESETS.iter().enumerate() {
        println!("  {}) {}", i + 1, preset.name);
    }
    println!("  {}) 自定义 (Custom)", PRESETS.len() + 1);
    println!();

    let choice = prompt_usize("请选择", 1, PRESETS.len() + 1)?;
    println!();

    if choice <= PRESETS.len() {
        let preset = &PRESETS[choice - 1];
        let provider_name = preset.name.to_lowercase();

        println!("── {} ──", preset.name);

        let api_key = prompt_api_key(preset.env_var_hint)?;
        let model = prompt_default("模型 (Model)", preset.default_model)?;
        let base_url = prompt_default("Base URL", preset.default_base_url)?;

        Ok((provider_name, preset.protocol, model, base_url, api_key))
    } else {
        println!("── 自定义提供商 ──");

        let provider_name = prompt_required("提供商名称 (例如 zhipu, ollama)")?;
        let protocol = prompt_protocol()?;
        let api_key = prompt_api_key("API_KEY")?;
        let model = prompt_required("模型 (Model)")?;
        let base_url = prompt_required("Base URL")?;

        Ok((provider_name, protocol, model, base_url, api_key))
    }
}

/// Save config and print result.
fn save_config(config: &XuanjiConfig, local: bool) -> Result<()> {
    // Create directories
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let xuanji_dir = home.join(".xuanji");
    let _ = std::fs::create_dir_all(xuanji_dir.join("workflows"));
    let _ = std::fs::create_dir_all(xuanji_dir.join("memory"));

    if local {
        config.save_local()?;
        println!();
        println!("配置已写入 ./xuanji.toml");
    } else {
        config.save_global()?;
        println!();
        println!("配置已写入 ~/.xuanji/config.toml");
    }

    println!();
    println!("下一步:");
    println!("  xuanji \"你的任务\"        # 单次执行");
    println!("  xuanji chat              # 交互式对话");
    println!("  xuanji mcp list          # 查看可用工具");
    println!();

    Ok(())
}

/// Mask API key for display (show first 6 and last 4 chars, or ${VAR} as-is).
fn mask_api_key(key: &str) -> String {
    if key.starts_with("${") && key.ends_with('}') {
        key.to_string()
    } else if key.len() > 10 {
        format!("{}****{}", &key[..6], &key[key.len()-4..])
    } else {
        "****".to_string()
    }
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
        println!("  此项不能为空，请重新输入。");
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
        println!("  请输入 {} 到 {} 之间的数字。", min, max);
    }
}

fn prompt_api_key(env_var_hint: &str) -> Result<String> {
    let input = prompt(&format!("API Key (直接输入密钥，或输入环境变量名如 {})", env_var_hint))?;
    if input.is_empty() {
        return Ok(format!("${{{}}}", env_var_hint));
    }
    if input.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !input.contains(' ') {
        Ok(format!("${{{}}}", input))
    } else {
        Ok(input)
    }
}

/// Prompt API key with a current value shown. Press Enter to keep current.
fn prompt_api_key_with_default(label: &str, current: &str) -> Result<String> {
    let masked = mask_api_key(current);
    let input = prompt(&format!("{} [{}]", label, masked))?;
    if input.is_empty() {
        Ok(current.to_string())
    } else if input.chars().all(|c| c.is_ascii_uppercase() || c == '_') && !input.contains(' ') {
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
