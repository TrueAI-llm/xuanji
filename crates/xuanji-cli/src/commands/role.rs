use anyhow::Result;
use xuanji_role::{GoalStatus, RoleStore};

/// Handle role subcommands.
pub async fn handle_role(action: &super::super::RoleAction) -> Result<()> {
    match action {
        super::super::RoleAction::Hire { name, purpose } => hire_role(name, purpose)?,
        super::super::RoleAction::Fire { name } => fire_role(name)?,
        super::super::RoleAction::List => list_roles()?,
        super::super::RoleAction::Show { name } => show_role(name)?,
        super::super::RoleAction::Activate { name } => activate_role(name)?,
        super::super::RoleAction::Chat { name } => {
            chat_with_role(name).await?
        }
        super::super::RoleAction::Evolve { name } => evolve_role(name)?,
    }
    Ok(())
}

fn hire_role(name: &str, purpose: &str) -> Result<()> {
    let role = xuanji_role::Role::new(name, purpose)?;
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
    RoleStore::delete(name)?;
    println!("ok 角色 '{}' 已销毁", name);
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
        let marker = if name == "god" { " \u{1f451}" } else { "" };
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
    let mut role = xuanji_role::Role::new(name, "")?;
    role.activate();
    println!("ok 角色 '{}' 已激活", name);
    Ok(())
}

async fn chat_with_role(name: &str) -> Result<()> {
    println!("与角色 '{}' 对话中...", name);
    if name == "god" {
        super::god::run_chat().await?;
    } else {
        let mut role = xuanji_role::Role::new(name, "")?;
        role.activate();
        role.add_user_goal("chat初始化");
        match role.run_cycle().await {
            Ok(_) => println!("Chat initialized"),
            Err(e) => {
                println!("初始化错误: {}", e);
            }
        }
    }
    Ok(())
}

fn evolve_role(name: &str) -> Result<()> {
    let mut role = xuanji_role::Role::new(name, "")?;
    role.activate();
    let rt = tokio::runtime::Runtime::new()?;
    match rt.block_on(role.run_cycle()) {
        Ok(Some(outcome)) => {
            println!("ok 角色 '{}' 完成一轮进化", name);
            println!("   执行: {}", outcome.summary);
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

fn goal_status_icon(status: &GoalStatus) -> &'static str {
    match status {
        GoalStatus::Pending => "\u{23f3}",
        GoalStatus::InProgress => "\u{1f504}",
        GoalStatus::Done => "\u{2705}",
        GoalStatus::Failed => "\u{274c}",
        GoalStatus::Blocked => "\u{1f6ab}",
    }
}
