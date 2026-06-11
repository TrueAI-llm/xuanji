use anyhow::Result;
use std::io::Write;
use std::sync::Arc;
use xuanji_llm::LlmProvider;
use xuanji_role::{CycleResult, Role, RoleStore, Stage, SuggestionKind};

use crate::commands::runtime::{build_agent, create_provider_arc, render_markdown, CliAgentFactory};
use crate::config::XuanjiConfig;

/// God Role name constant.
pub const GOD_NAME: &str = "god";

/// God Role seed purpose.
const GOD_PURPOSE: &str = "统筹管理所有 Role：分解任务、匹配角色、按需 hire/fire，并聚合最终结果";

/// Bootstrap the God Role with a real agent + provider + agent factory.
pub async fn bootstrap_god(config: &XuanjiConfig) -> Result<Role> {
    let (_, provider_config) = crate::main_fns::get_default_provider(config)?;
    let provider: Arc<dyn LlmProvider> = create_provider_arc(&provider_config)?;

    let mut god = Role::new(GOD_NAME, GOD_PURPOSE)?;

    // God's own agent: full MCP registry + persona + memory context + chat mode.
    let persona = god.render_persona();
    let memory_context = god.render_context();
    let god_agent = build_agent(&provider, config, &persona, &memory_context, true).await?;

    let factory = Arc::new(CliAgentFactory::new(
        provider.clone(),
        config.agent.clone(),
        config.trigger.workflows_dir.clone(),
    ));

    god = god
        .with_agent(god_agent)
        .with_provider(provider)
        .with_agent_factory(factory)
        .with_auto_hire(config.role.auto_hire)
        .with_fire_stale_days(config.role.fire_stale_days);

    // God is always Expert.
    let _ = god.set_stage(Stage::Expert);
    god.activate();
    god.persist()?;
    tracing::info!("God Role bootstrapped successfully");
    Ok(god)
}

/// Run a single prompt through God Role (orchestrated).
pub async fn run_prompt(prompt: &str, config: &XuanjiConfig) -> Result<()> {
    let mut god = bootstrap_god(config).await?;

    match god.run_orchestrated_cycle(prompt).await {
        Ok(result) => {
            if let Some(answer) = &result.answer {
                if !answer.trim().is_empty() {
                    println!();
                    render_markdown(answer);
                }
            }
            print_cycle_result(&result);
        }
        Err(e) => {
            anyhow::bail!("God Role error: {}", e);
        }
    }

    Ok(())
}

/// Run interactive chat through God Role (each turn runs an orchestrated cycle).
pub async fn run_chat(config: &XuanjiConfig) -> Result<()> {
    println!("╭──────────────────────────────────╮");
    println!("│  xuanji chat —— God Role          │");
    println!("│  输入 /help 查看命令               │");
    println!("│  输入 /quit 退出                  │");
    println!("╰──────────────────────────────────╯");
    println!();

    let mut god = bootstrap_god(config).await?;

    loop {
        let mut input = String::new();
        print!("> ");
        std::io::stdout().flush()?;
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();

        if input.is_empty() {
            continue;
        }

        match input.as_str() {
            "/quit" | "/exit" | "/q" => {
                println!("再见！");
                break;
            }
            "/help" | "/h" => {
                println!("/quit  — 退出");
                println!("/help  — 显示帮助");
                println!("/roles — 列出所有角色");
                println!("/teachings — 列出教学");
                continue;
            }
            "/roles" => {
                print_roles();
                continue;
            }
            "/teachings" => {
                let teachings = god.teaching_lib.list();
                if teachings.is_empty() {
                    println!("教学库为空。");
                } else {
                    println!("教学库 ({}):", teachings.len());
                    for t in teachings {
                        let preview = if t.content.len() > 60 { &t.content[..60] } else { &t.content };
                        println!(
                            "  [{}] {} - {} (置信度: {:.2})",
                            t.author_role,
                            t.kind.kind_str(),
                            preview,
                            t.confidence
                        );
                    }
                }
                continue;
            }
            _ => match god.run_orchestrated_cycle(&input).await {
                Ok(result) => {
                    println!();
                    if let Some(answer) = &result.answer {
                        if !answer.trim().is_empty() {
                            render_markdown(answer);
                            println!();
                        }
                    }
                    print_cycle_result(&result);
                }
                Err(e) => {
                    eprintln!("错误: {}", e);
                }
            },
        }
    }

    Ok(())
}

fn print_roles() {
    match RoleStore::list_roles() {
        Ok(names) => {
            if names.is_empty() {
                println!("没有活跃的角色。使用 xuanji role hire 创建。");
            } else {
                println!("活跃角色:");
                for name in &names {
                    let marker = if name == "god" { " 👑" } else { "" };
                    if let Ok(store) = RoleStore::new(name) {
                        if let Ok(Some(profile)) = store.load_profile() {
                            println!(
                                "  - {}{} | {:?} | {}",
                                name, marker, profile.evolution_stage, profile.seed_purpose
                            );
                            continue;
                        }
                    }
                    println!("  - {}{}", name, marker);
                }
            }
        }
        Err(e) => println!("无法列出角色: {}", e),
    }
}

/// Pretty-print a CycleResult (dispatch summary + suggestions).
pub(crate) fn print_cycle_result(result: &CycleResult) {
    if !result.dispatched_to.is_empty() {
        println!("\n📦 已派发至:");
        for name in &result.dispatched_to {
            println!("   - {}", name);
        }
    }

    if !result.suggestions.is_empty() {
        println!();
        println!("💡 建议:");
        for s in &result.suggestions {
            match s.kind {
                SuggestionKind::HireRole => {
                    println!("   hire \"{}\"", s.role_name);
                    if let Some(ref purpose) = s.purpose {
                        println!("     purpose: {}", purpose);
                    }
                }
                SuggestionKind::FireRole => {
                    println!("   fire \"{}\"", s.role_name);
                }
                SuggestionKind::RedefinePurpose => {
                    println!("   redefine \"{}\"", s.role_name);
                    if let Some(ref purpose) = s.purpose {
                        println!("     purpose: {}", purpose);
                    }
                }
            }
            println!("     理由: {}", s.reason);
        }
    }
}
