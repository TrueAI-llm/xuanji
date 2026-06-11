use anyhow::Result;
use std::io::Write;
use xuanji_role::RoleStore;

/// God Role name constant.
pub const GOD_NAME: &str = "god";

/// God Role seed purpose.
const GOD_PURPOSE: &str =
    "统筹管理所有 Role，发现协作机会，优化整体效率";

/// Bootstrap the God Role (idempotent).
pub fn bootstrap_god() -> Result<xuanji_role::Role> {
    match xuanji_role::Role::new(GOD_NAME, GOD_PURPOSE) {
        Ok(mut role) => {
            role.activate();
            role.persist()?;
            tracing::info!("God Role bootstrapped successfully");
            Ok(role)
        }
        Err(e) => {
            tracing::warn!("God Role bootstrap skipped: {}", e);
            let mut role = xuanji_role::Role::new(GOD_NAME, GOD_PURPOSE)?;
            role.activate();
            Ok(role)
        }
    }
}

/// Run a single prompt through God Role.
pub async fn run_prompt(prompt: &str) -> Result<()> {
    let mut god = bootstrap_god()?;
    god.add_user_goal(prompt);

    match god.run_cycle().await {
        Ok(Some(outcome)) => {
            if outcome.success {
                println!("{}", outcome.summary);
            } else {
                println!("任务执行遇到问题: {}", outcome.lessons);
            }
        }
        Ok(None) => {
            println!("(no action taken)");
        }
        Err(e) => {
            anyhow::bail!("God Role error: {}", e);
        }
    }

    Ok(())
}

/// Run interactive chat through God Role.
pub async fn run_chat() -> Result<()> {
    println!("\u{2554}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2557}");
    println!("\u{2551}  xuanji chat \u{2014}\u{2014} God Role          \u{2551}");
    println!("\u{2551}  输入 /help 查看命令               \u{2551}");
    println!("\u{2551}  输入 /quit 退出                  \u{2551}");
    println!("\u{255a}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{255d}");
    println!();

    let mut god = bootstrap_god()?;

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
                match RoleStore::list_roles() {
                    Ok(names) => {
                        if names.is_empty() {
                            println!("没有活跃的角色。使用 xuanji role hire 创建。");
                        } else {
                            println!("活跃角色:");
                            for name in &names {
                                let marker = if name == "god" { " \u{1f451}" } else { "" };
                                println!("  - {}{}", name, marker);
                            }
                        }
                    }
                    Err(e) => {
                        println!("无法列出角色: {}", e);
                    }
                }
                continue;
            }
            "/teachings" => {
                let teachings = god.teaching_lib.list();
                if teachings.is_empty() {
                    println!("教学库为空。");
                } else {
                    println!("教学库 ({}):", teachings.len());
                    for t in teachings {
                        let preview = if t.content.len() > 60 {
                            &t.content[..60]
                        } else {
                            &t.content
                        };
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
            _ => {
                god.add_user_goal(&input);
                match god.run_cycle().await {
                    Ok(Some(outcome)) => {
                        println!();
                        if outcome.success {
                            println!("目标已执行: {}", outcome.summary);
                        } else {
                            println!("执行遇到问题: {}", outcome.lessons);
                        }
                    }
                    Ok(None) => {
                        println!("(no action)");
                    }
                    Err(e) => {
                        eprintln!("错误: {}", e);
                    }
                }
            }
        }
    }

    Ok(())
}
