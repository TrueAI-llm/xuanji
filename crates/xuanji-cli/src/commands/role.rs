use anyhow::Result;
use std::io::Write;
use std::sync::Arc;
use xuanji_llm::LlmProvider;
use xuanji_role::{GoalStatus, Role, RoleStore};

use crate::commands::runtime::{build_agent, create_provider_arc, render_markdown, CliAgentFactory};
use crate::config::XuanjiConfig;

/// Handle role subcommands.
pub async fn handle_role(action: &super::super::RoleAction, config: &XuanjiConfig) -> Result<()> {
    match action {
        super::super::RoleAction::Hire { name, purpose } => hire_role(name, purpose)?,
        super::super::RoleAction::Fire { name } => fire_role(name)?,
        super::super::RoleAction::List => list_roles()?,
        super::super::RoleAction::Show { name } => show_role(name)?,
        super::super::RoleAction::Activate { name } => activate_role(name)?,
        super::super::RoleAction::Chat { name } => chat_with_role(name, config).await?,
        super::super::RoleAction::Evolve { name } => evolve_role(name, config).await?,
    }
    Ok(())
}

fn hire_role(name: &str, purpose: &str) -> Result<()> {
    let role = Role::new(name, purpose)?;
    role.persist()?;
    println!("ok 角色 '{}' 已创建", name);
    println!("   purpose: {}", purpose);
    println!("   stage:   {:?}", role.profile.evolution_stage);
    println!("   运行 xuanji role activate {} 启动自驱循环", name);
    Ok(())
}

fn fire_role(name: &str) -> Result<()> {
    if name == "god" {
        anyhow::bail!("不能删除 God Role");
    }
    // Safe fire: archive (recoverable) rather than hard delete.
    RoleStore::archive(name)?;
    println!("ok 角色 '{}' 已归档（可用 restore 恢复）", name);
    Ok(())
}

fn list_roles() -> Result<()> {
    let names = RoleStore::list_roles()?;
    if names.is_empty() {
        println!("没有活跃的角色");
        println!("使用 xuanji role hire <name> --purpose \"...\" 创建");
        return Ok(());
    }
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
    Ok(())
}

fn show_role(name: &str) -> Result<()> {
    let store = RoleStore::new(name)?;
    if let Some(profile) = store.load_profile()? {
        println!("角色: {}", profile.name);
        println!("  purpose: {}", profile.seed_purpose);
        println!("  自我认知: {}", profile.self_description);
        println!("  进化阶段: {:?}", profile.evolution_stage);
        println!("  创建于:   {}", profile.created_at);

        let goals = store.load_goals()?;
        println!("  目标队列: {} 个", goals.len());
        for g in &goals {
            println!(
                "    [{}] {} (priority: {:.2})",
                goal_status_icon(&g.status),
                g.description,
                g.priority
            );
        }

        let rules = store.load_rules()?;
        println!("  规则: {} 条", rules.len());

        let cases = store.load_cases()?;
        println!("  案例: {} 条", cases.len());
    } else {
        println!("角色 '{}' 未找到", name);
    }
    Ok(())
}

fn activate_role(name: &str) -> Result<()> {
    let mut role = Role::new(name, "")?;
    role.activate();
    println!("ok 角色 '{}' 已激活", name);
    Ok(())
}

async fn chat_with_role(name: &str, config: &XuanjiConfig) -> Result<()> {
    if name == "god" {
        super::god::run_chat(config).await?;
        return Ok(());
    }

    println!("与角色 '{}' 对话中（输入 /quit 退出）...\n", name);
    let mut role = build_named_role(name, config).await?;

    loop {
        print!("> ");
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_string();
        if input.is_empty() {
            continue;
        }
        if input == "/quit" || input == "/exit" {
            break;
        }
        match role.run_orchestrated_cycle(&input).await {
            Ok(result) => {
                if let Some(answer) = &result.answer {
                    if !answer.trim().is_empty() {
                        println!();
                        render_markdown(answer);
                        println!();
                    }
                }
            }
            Err(e) => eprintln!("错误: {}", e),
        }
    }
    Ok(())
}

async fn evolve_role(name: &str, config: &XuanjiConfig) -> Result<()> {
    let mut role = build_named_role(name, config).await?;
    match role.run_cycle().await {
        Ok(Some(outcome)) => {
            println!("ok 角色 '{}' 完成一轮进化", name);
            println!("   执行: {}", outcome.summary);
            println!("   成功: {} | 工具调用: {} | tokens: {}", outcome.success, outcome.tool_calls_count, outcome.tokens_used);
        }
        Ok(None) => {
            println!("角色 '{}' 没有待处理的目标", name);
        }
        Err(e) => {
            println!("进化执行出错: {}", e);
        }
    }
    Ok(())
}

/// Build a role with a real agent + provider + factory wired in (for chat/evolve).
async fn build_named_role(name: &str, config: &XuanjiConfig) -> Result<Role> {
    let (_, provider_config) = crate::main_fns::get_default_provider(config)?;
    let provider: Arc<dyn LlmProvider> = create_provider_arc(&provider_config)?;

    let mut role = Role::new(name, "")?;
    let persona = role.render_persona();
    let memory_context = role.render_context();
    let agent = build_agent(&provider, config, &persona, &memory_context, true).await?;

    let factory = Arc::new(CliAgentFactory::new(
        provider.clone(),
        config.agent.clone(),
        config.trigger.workflows_dir.clone(),
    ));

    role = role
        .with_agent(agent)
        .with_provider(provider)
        .with_agent_factory(factory)
        .with_auto_hire(config.role.auto_hire)
        .with_fire_stale_days(config.role.fire_stale_days);
    role.activate();
    Ok(role)
}

fn goal_status_icon(status: &GoalStatus) -> &'static str {
    match status {
        GoalStatus::Pending => "⏳",
        GoalStatus::InProgress => "🔄",
        GoalStatus::Done => "✅",
        GoalStatus::Failed => "❌",
        GoalStatus::Blocked => "🚫",
    }
}
