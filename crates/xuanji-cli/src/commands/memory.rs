use anyhow::Result;
use xuanji_role::{Role, RoleStore, Rule};

/// Memory commands now operate on the active role's RoleStore (default: `god`),
/// replacing the old project-scoped LongTermMemory on the role path.

const ACTIVE_ROLE: &str = "god";

/// Show accumulated knowledge (context, rules, cases, teachings) for the active role.
pub fn show_memory() -> Result<()> {
    let role = Role::new(ACTIVE_ROLE, "")?;
    let ctx = role.render_context();
    if ctx.trim().is_empty() {
        println!("角色 {} 暂无累积记忆。", ACTIVE_ROLE);
    } else {
        println!("{}", ctx);
    }
    Ok(())
}

/// Clear the active role's learned rules/cases/preferences (profile is kept).
pub fn clear_memory() -> Result<()> {
    let store = RoleStore::new(ACTIVE_ROLE)?;
    store.save_rules(&[])?;
    store.save_cases(&[])?;
    store.save_preferences(&std::collections::HashMap::new())?;
    println!("ok 已清空角色 {} 的规则/案例/偏好（profile 保留）", ACTIVE_ROLE);
    Ok(())
}

/// Add a user-defined rule to the active role (low initial confidence).
pub fn add_rule(text: &str) -> Result<()> {
    let store = RoleStore::new(ACTIVE_ROLE)?;
    let mut rules = store.load_rules()?;
    rules.push(Rule {
        id: format!("rule-user-{}", rules.len() + 1),
        condition: "用户指定规则".to_string(),
        action: text.to_string(),
        confidence: 0.5,
        source_case_id: None,
        validated_count: 0,
        created_at: "manual".to_string(),
    });
    store.save_rules(&rules)?;
    println!("ok 已为角色 {} 添加规则: {}", ACTIVE_ROLE, text);
    Ok(())
}
