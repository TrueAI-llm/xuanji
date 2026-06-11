pub mod discover;
pub mod error;
pub mod orchestrate;
pub mod reflect;
pub mod store;
pub mod teaching;
pub mod types;

use std::collections::HashMap;
use std::sync::Arc;
use xuanji_agent::types::ExecutionStats;
use xuanji_agent::Agent;
use xuanji_llm::LlmProvider;

pub use discover::DiscoverEngine;
pub use error::RoleError;
pub use orchestrate::{heuristic_assignments, RoleOrchestrator};
pub use reflect::LearningEngine;
pub use store::RoleStore;
pub use teaching::TeachingLibrary;
pub use types::*;

// ─── AgentFactory ───

/// Builds a ready-to-run [`Agent`] for a role, with that role's persona and rendered
/// memory context injected.
///
/// Implemented by the CLI layer (which owns provider/tool construction). `xuanji-role`
/// depends only on this trait, never on provider/registry crates — keeping the
/// `xuanji-role → xuanji-agent` dependency one-directional (no cycle).
pub trait AgentFactory: Send + Sync {
    fn build(&self, role_name: &str, persona: &str, memory_context: &str) -> Agent;
}

// ─── Heuristic orchestrator (fallback + analysis) ───

/// Heuristic role matching / fire analysis. Used as a fallback when the LLM
/// orchestrator is unavailable, and to compute fire suggestions.
pub struct Orchestrator;

impl Orchestrator {
    /// Match subtasks to available roles based on skill tags.
    /// Returns tuples of (matched_tasks, unmatched_tasks, suggestions).
    pub fn match_roles(
        subtasks: Vec<SubTask>,
        available_roles: &[RoleProfile],
    ) -> (Vec<SubTask>, Vec<SubTask>, Vec<OrchestrationSuggestion>) {
        let mut matched = Vec::new();
        let mut unmatched = Vec::new();
        let mut suggestions = Vec::new();

        for task in subtasks {
            if let Some(ref skill) = task.required_skill {
                let best_role = available_roles.iter().find(|r| {
                    let lower_purpose = r.seed_purpose.to_lowercase();
                    skill_matches(skill, &lower_purpose)
                });

                if let Some(role) = best_role {
                    let mut matched_task = task;
                    matched_task.assignee = Some(role.name.clone());
                    matched.push(matched_task);
                } else {
                    suggestions.push(OrchestrationSuggestion {
                        kind: SuggestionKind::HireRole,
                        role_name: format!("{}-specialist", skill),
                        purpose: Some(skill_translate(skill)),
                        reason: format!(
                            "当前无角色覆盖 '{}' 领域（需要执行: {}）",
                            skill, task.description
                        ),
                    });
                    unmatched.push(task);
                }
            } else {
                let mut own_task = task;
                own_task.assignee = Some("god".into());
                matched.push(own_task);
            }
        }

        (matched, unmatched, suggestions)
    }

    /// Evaluate roles and suggest firing low-performing ones.
    /// `fire_stale_days` configures how old a stuck `Seed` role must be to flag.
    pub fn fire_suggest(roles: &[RoleProfile], fire_stale_days: i64) -> Vec<OrchestrationSuggestion> {
        let mut suggestions = Vec::new();

        for role in roles {
            if role.name == "god" {
                continue;
            }
            if role.evolution_stage == Stage::Seed && is_stale(&role.created_at, fire_stale_days) {
                suggestions.push(OrchestrationSuggestion {
                    kind: SuggestionKind::FireRole,
                    role_name: role.name.clone(),
                    purpose: None,
                    reason: format!(
                        "角色 '{}' 处于 Seed 阶段超过 {} 天且无进化，建议 fire 或重新定义 purpose",
                        role.name, fire_stale_days
                    ),
                });
            }
        }

        suggestions
    }
}

/// Case-insensitive skill → purpose matching (shared by heuristic fallback and analysis).
pub(crate) fn skill_matches(skill: &str, lower_purpose: &str) -> bool {
    match skill {
        "security" => lower_purpose.contains("安全") || lower_purpose.contains("security"),
        "performance" => lower_purpose.contains("性能") || lower_purpose.contains("performance"),
        "documentation" => lower_purpose.contains("文档") || lower_purpose.contains("doc"),
        "testing" => lower_purpose.contains("测试") || lower_purpose.contains("test"),
        "devops" => lower_purpose.contains("部署") || lower_purpose.contains("devops"),
        "build" => lower_purpose.contains("构建") || lower_purpose.contains("build"),
        _ => lower_purpose.contains(skill),
    }
}

/// Check if a date string is older than N days.
fn is_stale(date_str: &str, days: i64) -> bool {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S") {
        let now = chrono::Local::now().naive_local();
        let diff = now - dt;
        diff.num_days() >= days
    } else {
        false
    }
}

fn skill_translate(skill: &str) -> String {
    match skill {
        "security" => "审计安全漏洞、检查依赖项安全、监控安全风险",
        "performance" => "分析性能瓶颈、优化构建速度、监控资源使用",
        "documentation" => "生成项目文档、维护 API 文档、编写开发指南",
        "testing" => "编写和运行测试、分析测试覆盖率、改进测试策略",
        "devops" => "管理部署流程、配置 CI/CD、监控服务状态",
        "build" => "管理构建过程、优化编译速度、维护构建配置",
        _ => "unknown",
    }
    .to_string()
}

// ─── Role ───

/// An autonomous, self-evolving role.
pub struct Role {
    pub profile: RoleProfile,
    store: RoleStore,
    pub agent: Option<Agent>,
    /// Builds agents for worker roles this role dispatches to (God Role only, typically).
    agent_factory: Option<Arc<dyn AgentFactory>>,
    /// Shared LLM provider for orchestration calls (decompose/match/aggregate).
    provider: Option<Arc<dyn LlmProvider>>,
    /// Auto-hire specialists on skill gaps (default true).
    auto_hire: bool,
    /// Days after which a stuck Seed role is auto-archived (default 7).
    fire_stale_days: i64,
    goals: Vec<GoalNode>,
    rules: Vec<Rule>,
    cases: Vec<CaseEntry>,
    preferences: HashMap<String, ToolPreference>,
    pub teaching_lib: TeachingLibrary,
    active: bool,
    outcomes: Vec<GoalOutcome>,
}

impl Role {
    /// Open (or create) a role by name. Preserves an existing profile/evolution rather
    /// than overwriting it — fixing the previous "Role::new empties the profile" bug.
    pub fn new(name: &str, seed_purpose: &str) -> Result<Self, RoleError> {
        let store = RoleStore::new(name)?;
        let profile = match store.load_profile()? {
            Some(existing) => existing,
            None => {
                let p = RoleProfile::new(name, seed_purpose);
                store.save_profile(&p)?;
                p
            }
        };

        let goals = store.load_goals()?;
        let rules = store.load_rules()?;
        let cases = store.load_cases()?;
        let preferences = store.load_preferences()?;
        let teaching_lib = TeachingLibrary::load()?;

        Ok(Self {
            profile,
            store,
            agent: None,
            agent_factory: None,
            provider: None,
            auto_hire: true,
            fire_stale_days: 7,
            goals,
            rules,
            cases,
            preferences,
            teaching_lib,
            active: false,
            outcomes: Vec::new(),
        })
    }

    pub fn with_agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    pub fn with_agent_factory(mut self, factory: Arc<dyn AgentFactory>) -> Self {
        self.agent_factory = Some(factory);
        self
    }

    pub fn with_provider(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn with_auto_hire(mut self, auto_hire: bool) -> Self {
        self.auto_hire = auto_hire;
        self
    }

    pub fn with_fire_stale_days(mut self, days: i64) -> Self {
        self.fire_stale_days = days;
        self
    }

    pub fn activate(&mut self) {
        self.active = true;
        tracing::info!("Role '{}' activated", self.profile.name);
    }

    pub fn deactivate(&mut self) {
        self.active = false;
        tracing::info!("Role '{}' deactivated", self.profile.name);
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set the evolution stage and persist the profile (e.g. bootstrap God as Expert).
    pub fn set_stage(&mut self, stage: Stage) -> Result<(), RoleError> {
        self.profile.evolution_stage = stage;
        self.store.save_profile(&self.profile)
    }

    // ─── Persona / context rendering (injected into the agent's prompt) ───

    /// Render this role's persona — what gets prepended to the system prompt so that
    /// different roles actually behave differently.
    pub fn render_persona(&self) -> String {
        format!(
            "你是 {}。\n{}\n专精方向：{}。\n当前进化阶段：{:?}。",
            self.profile.name,
            self.profile.self_description,
            self.profile.seed_purpose,
            self.profile.evolution_stage,
        )
    }

    /// Render accumulated knowledge (context notes, validated rules, recent cases,
    /// relevant teachings) as a markdown block for prompt injection.
    pub fn render_context(&self) -> String {
        let mut s = String::new();

        let ctx = self.store.load_context().unwrap_or_default();
        if !ctx.focus.is_empty() {
            s.push_str(&format!("## 当前聚焦\n{}\n", ctx.focus));
        }
        if !ctx.notes.is_empty() {
            s.push_str(&format!("## 角色笔记\n{}\n", ctx.notes));
        }

        let high_conf: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| r.confidence >= 0.6)
            .take(10)
            .collect();
        if !high_conf.is_empty() {
            s.push_str("## 已验证规则\n");
            for r in high_conf {
                s.push_str(&format!("- 当「{}」：则「{}」\n", r.condition, r.action));
            }
        }

        if !self.cases.is_empty() {
            s.push_str(&format!("## 历史案例（共 {} 条）\n", self.cases.len()));
            for c in self.cases.iter().rev().take(5) {
                s.push_str(&format!("- {}（策略：{}，结果：{:?}）\n", c.task_description, c.strategy_used, c.outcome));
            }
        }

        let tags: Vec<String> = self
            .cases
            .iter()
            .rev()
            .take(3)
            .flat_map(|c| c.context_tags.clone())
            .collect();
        let teachings = self.teaching_lib.query_by_tags(&tags);
        if !teachings.is_empty() {
            s.push_str("## 跨角色经验\n");
            for t in teachings.iter().take(5) {
                s.push_str(&format!("- [{}]: {}\n", t.author_role, t.content.replace('\n', " ")));
            }
        }

        s
    }

    // ─── Execution ───

    /// Execute a single task via this role's agent, then reflect + persist.
    /// Returns the agent's execution stats.
    pub async fn run_task(&mut self, description: &str) -> Result<ExecutionStats, RoleError> {
        let stats = {
            let agent = self
                .agent
                .as_mut()
                .ok_or_else(|| RoleError::Agent(format!("role '{}' has no agent attached", self.profile.name)))?;
            agent
                .run(description.to_string())
                .await
                .map_err(|e| RoleError::Agent(e.to_string()))?
        };

        let outcome = GoalOutcome {
            goal_id: format!("goal-outcome-{}", chrono_now_compact()),
            success: stats.success,
            summary: description.to_string(),
            tool_calls_count: stats.tool_calls,
            tokens_used: stats.tokens,
            lessons: if stats.success {
                String::new()
            } else {
                "execution completed with errors".to_string()
            },
        };
        self.outcomes.push(outcome);
        self.reflect_and_persist()?;

        Ok(stats)
    }

    /// Drain pending outcomes into learning artifacts, generate teachings, evolve stage.
    fn reflect_and_persist(&mut self) -> Result<(), RoleError> {
        for o in self.outcomes.drain(..) {
            LearningEngine::reflect_on_goal(&o, &mut self.rules, &mut self.cases, &mut self.preferences);
        }
        let _ = LearningEngine::generate_teaching(&self.profile.name, &self.rules, &mut self.teaching_lib)?;
        self.profile.evolution_stage = self.profile.evolution_stage.promote(
            self.cases.len(),
            self.rules.iter().filter(|r| r.confidence >= 0.7).count(),
        );
        self.store.save_profile(&self.profile)?;
        self.persist()?;
        Ok(())
    }

    // ─── Orchestrated cycle (God Role) ───

    /// Run an orchestrated cycle: tiered-fire → decompose → match → dispatch
    /// (with auto-hire) → aggregate → reflect. Returns the final answer + suggestions.
    pub async fn run_orchestrated_cycle(
        &mut self,
        goal_description: &str,
    ) -> Result<CycleResult, RoleError> {
        if !self.active {
            self.activate();
        }

        let mut suggestions = Vec::new();

        // 0. Tiered fire: auto-archive empty stale roles; suggest for the rest.
        self.tiered_fire(&mut suggestions).await;

        // 1. Decompose (LLM, fallback heuristic).
        let subtasks = match &self.provider {
            Some(p) => RoleOrchestrator::new(p.clone()).decompose(goal_description).await,
            None => DiscoverEngine::decompose(goal_description),
        };

        // 2. Match against live roles (LLM, fallback heuristic).
        let live_roles = self.other_role_profiles();
        let assignments = match &self.provider {
            Some(p) => RoleOrchestrator::new(p.clone()).assign(&subtasks, &live_roles).await,
            None => heuristic_assignments(&subtasks, &live_roles),
        };

        // 3. Dispatch each assignment.
        let mut results: Vec<(String, String)> = Vec::new();
        let mut dispatched = Vec::new();
        let mut total_calls = 0u32;
        let mut total_tokens = 0u32;
        let mut all_success = true;

        for a in &assignments {
            if let Some(purpose) = a.hire.clone() {
                if self.auto_hire {
                    match self.dispatch_to_new_role(&purpose, &a.description).await {
                        Ok(stats) => {
                            total_calls += stats.tool_calls;
                            total_tokens += stats.tokens;
                            if !stats.success {
                                all_success = false;
                            }
                            let name = derive_role_name(&purpose);
                            results.push((a.description.clone(), stats.text));
                            dispatched.push(name);
                        }
                        Err(e) => {
                            tracing::warn!("auto-hire dispatch failed: {}", e);
                            all_success = false;
                            results.push((a.description.clone(), format!("（执行失败：{}）", e)));
                        }
                    }
                } else {
                    suggestions.push(OrchestrationSuggestion {
                        kind: SuggestionKind::HireRole,
                        role_name: derive_role_name(&purpose),
                        purpose: Some(purpose),
                        reason: format!("需新建角色处理子任务：{}", a.description),
                    });
                    results.push((a.description.clone(), "（未执行：缺少对应角色，已生成 hire 建议）".to_string()));
                    all_success = false;
                }
                continue;
            }

            let assignee = a.assignee.as_deref();
            let is_self = assignee.is_none()
                || assignee == Some("god")
                || assignee == Some(self.profile.name.as_str());

            if is_self {
                match self.run_task(&a.description).await {
                    Ok(stats) => {
                        total_calls += stats.tool_calls;
                        total_tokens += stats.tokens;
                        if !stats.success {
                            all_success = false;
                        }
                        results.push((a.description.clone(), stats.text));
                        dispatched.push("god".to_string());
                    }
                    Err(e) => {
                        all_success = false;
                        results.push((a.description.clone(), format!("（执行失败：{}）", e)));
                    }
                }
            } else if let Some(name) = assignee {
                match self.dispatch_to_existing(name, &a.description).await {
                    Ok(stats) => {
                        total_calls += stats.tool_calls;
                        total_tokens += stats.tokens;
                        if !stats.success {
                            all_success = false;
                        }
                        results.push((a.description.clone(), stats.text));
                        dispatched.push(name.to_string());
                    }
                    Err(e) => {
                        all_success = false;
                        results.push((a.description.clone(), format!("（委派失败：{}）", e)));
                    }
                }
            }
        }

        // 4. Aggregate.
        let answer = match &self.provider {
            Some(p) => RoleOrchestrator::new(p.clone()).aggregate(goal_description, &results).await,
            None => results
                .iter()
                .map(|(d, r)| format!("### {}\n{}", d, r))
                .collect::<Vec<_>>()
                .join("\n\n"),
        };

        // 5. God's aggregate outcome + reflect.
        let outcome = GoalOutcome {
            goal_id: format!("goal-outcome-{}", chrono_now_compact()),
            success: all_success,
            summary: goal_description.to_string(),
            tool_calls_count: total_calls,
            tokens_used: total_tokens,
            lessons: if all_success {
                String::new()
            } else {
                "部分子任务未成功完成".to_string()
            },
        };
        self.outcomes.push(outcome.clone());
        self.reflect_and_persist()?;

        Ok(CycleResult {
            outcome: Some(outcome),
            answer: Some(answer),
            suggestions,
            dispatched_to: dispatched,
        })
    }

    /// Tiered fire: auto-archive Seed roles with zero cases past the staleness threshold;
    /// emit FireRole suggestions for other low-value roles.
    async fn tiered_fire(&self, suggestions: &mut Vec<OrchestrationSuggestion>) {
        let names = RoleStore::list_roles().unwrap_or_default();
        for name in &names {
            if name == "god" || name == &self.profile.name {
                continue;
            }
            let Ok(store) = RoleStore::new(name) else { continue };
            let Ok(Some(profile)) = store.load_profile() else { continue };
            let cases = store.load_cases().unwrap_or_default();

            if profile.evolution_stage == Stage::Seed
                && cases.is_empty()
                && is_stale(&profile.created_at, self.fire_stale_days)
            {
                // Safe to archive — nothing learned yet.
                let _ = RoleStore::archive(name);
                tracing::info!("auto-archived stale empty role '{}'", name);
            } else if profile.evolution_stage == Stage::Seed
                && is_stale(&profile.created_at, self.fire_stale_days)
            {
                suggestions.push(OrchestrationSuggestion {
                    kind: SuggestionKind::FireRole,
                    role_name: name.clone(),
                    purpose: None,
                    reason: format!(
                        "角色 '{}' 处于 Seed 阶段超过 {} 天，建议 fire 或重新定义 purpose",
                        name, self.fire_stale_days
                    ),
                });
            }
        }
    }

    /// Dispatch a subtask to an existing role: load it, build its agent via the factory,
    /// execute, reflect into that role's own memory.
    async fn dispatch_to_existing(
        &self,
        name: &str,
        description: &str,
    ) -> Result<ExecutionStats, RoleError> {
        let purpose = RoleStore::new(name)
            .ok()
            .and_then(|s| s.load_profile().ok().flatten())
            .map(|p| p.seed_purpose)
            .unwrap_or_else(|| description.to_string());

        let persona;
        let memory_context;
        let mut worker = Role::new(name, &purpose)?;
        persona = worker.render_persona();
        memory_context = worker.render_context();

        let factory = self
            .agent_factory
            .as_ref()
            .ok_or_else(|| RoleError::Agent("no agent factory available to dispatch".into()))?;
        let agent = factory.build(name, &persona, &memory_context);
        worker = worker.with_agent(agent);
        worker.activate();
        worker.run_task(description).await
    }

    /// Hire a brand-new specialist role on the fly and dispatch the subtask to it.
    async fn dispatch_to_new_role(
        &self,
        purpose: &str,
        description: &str,
    ) -> Result<ExecutionStats, RoleError> {
        let name = derive_role_name(purpose);
        let mut worker = Role::new(&name, purpose)?;
        let persona = worker.render_persona();
        let memory_context = worker.render_context();

        let factory = self
            .agent_factory
            .as_ref()
            .ok_or_else(|| RoleError::Agent("no agent factory available to hire".into()))?;
        let agent = factory.build(&name, &persona, &memory_context);
        worker = worker.with_agent(agent);
        worker.activate();
        tracing::info!("auto-hired new role '{}' for: {}", name, description);
        worker.run_task(description).await
    }

    /// Profiles of all other (non-self, non-god-matching) roles currently on disk.
    fn other_role_profiles(&self) -> Vec<RoleProfile> {
        RoleStore::list_roles()
            .unwrap_or_default()
            .iter()
            .filter(|n| *n != &self.profile.name)
            .filter_map(|n| RoleStore::new(n).ok()?.load_profile().ok()?)
            .collect()
    }

    // ─── Simple self-driven cycle (role activate / evolve) ───

    pub async fn run_cycle(&mut self) -> Result<Option<GoalOutcome>, RoleError> {
        if !self.active {
            self.activate();
        }

        // REFLECT + publish teachings on any pending outcomes.
        self.reflect_and_persist()?;

        // DISCOVER + PRIORITIZE the next goal.
        let candidates = DiscoverEngine::discover(&self.profile, &self.rules, &self.cases);
        let scored = DiscoverEngine::prioritize(candidates, 0);

        let Some(top) = scored.first() else {
            self.persist()?;
            return Ok(None);
        };

        let goal = GoalNode::new(&top.description, top.score, GoalSource::SelfDiscovered);
        self.goals.push(goal);

        // Need an agent to execute.
        if self.agent.is_none() {
            self.persist()?;
            return Ok(None);
        }

        let stats = self.run_task(&top.description).await?;
        let outcome = GoalOutcome {
            goal_id: format!("goal-outcome-{}", chrono_now_compact()),
            success: stats.success,
            summary: top.description.clone(),
            tool_calls_count: stats.tool_calls,
            tokens_used: stats.tokens,
            lessons: if stats.success {
                String::new()
            } else {
                "task execution had errors".to_string()
            },
        };
        Ok(Some(outcome))
    }

    pub fn persist(&self) -> Result<(), RoleError> {
        self.store.save_goals(&self.goals)?;
        self.store.save_rules(&self.rules)?;
        self.store.save_cases(&self.cases)?;
        self.store.save_preferences(&self.preferences)?;
        Ok(())
    }

    pub fn goals(&self) -> &[GoalNode] {
        &self.goals
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn cases(&self) -> &[CaseEntry] {
        &self.cases
    }

    pub fn preferences(&self) -> &HashMap<String, ToolPreference> {
        &self.preferences
    }

    pub fn add_user_goal(&mut self, description: &str) {
        let goal = GoalNode::new(description, 1.0, GoalSource::User);
        self.goals.push(goal);
    }
}

/// Derive a filesystem-safe role name from a free-text purpose.
fn derive_role_name(purpose: &str) -> String {
    let slug: String = purpose
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .to_lowercase();
    let first = slug.split_whitespace().next().unwrap_or("");
    let cleaned: String = first.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if cleaned.len() >= 2 {
        cleaned
    } else {
        format!("specialist-{}", chrono_now_compact())
    }
}

fn chrono_now_compact() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_creation() {
        let role = Role::new("test-integration", "test purpose").unwrap();
        assert_eq!(role.profile.name, "test-integration");
        assert_eq!(role.profile.evolution_stage, Stage::Seed);
        assert!(role.goals().is_empty());
        RoleStore::delete("test-integration").ok();
    }

    #[test]
    fn test_role_activation() {
        let mut role = Role::new("test-activate", "test").unwrap();
        assert!(!role.is_active());
        role.activate();
        assert!(role.is_active());
        role.deactivate();
        assert!(!role.is_active());
        RoleStore::delete("test-activate").ok();
    }

    #[test]
    fn test_add_user_goal() {
        let mut role = Role::new("test-goal", "test").unwrap();
        role.add_user_goal("manual task");
        assert_eq!(role.goals().len(), 1);
        assert_eq!(role.goals()[0].created_by, GoalSource::User);
        RoleStore::delete("test-goal").ok();
    }

    #[test]
    fn test_persist_and_restore() {
        let mut role = Role::new("test-persist", "test").unwrap();
        role.add_user_goal("goal 1");
        role.persist().unwrap();

        let restored = Role::new("test-persist", "test").unwrap();
        assert_eq!(restored.goals().len(), 1);
        RoleStore::delete("test-persist").ok();
    }

    #[test]
    fn test_new_preserves_existing_profile() {
        // First create with a purpose.
        {
            let role = Role::new("test-preserve", "original purpose").unwrap();
            assert_eq!(role.profile.seed_purpose, "original purpose");
        }
        // Re-open with a DIFFERENT purpose arg — must NOT overwrite the stored one.
        let reopened = Role::new("test-preserve", "should be ignored").unwrap();
        assert_eq!(reopened.profile.seed_purpose, "original purpose");
        RoleStore::delete("test-preserve").ok();
    }

    #[test]
    fn test_render_persona_contains_name_and_purpose() {
        let role = Role::new("test-persona", "审计安全").unwrap();
        let persona = role.render_persona();
        assert!(persona.contains("test-persona"));
        assert!(persona.contains("审计安全"));
        RoleStore::delete("test-persona").ok();
    }

    #[test]
    fn test_derive_role_name_ascii() {
        assert_eq!(derive_role_name("Frontend engineering"), "frontend");
        assert_eq!(derive_role_name("database optimization"), "database");
        // Non-ascii falls back to a specialist-<ts> slug.
        let cn = derive_role_name("前端工程");
        assert!(cn.starts_with("specialist-"), "got: {cn}");
    }

    // ─── Orchestrator tests ───

    #[test]
    fn test_match_roles_security() {
        let roles = vec![RoleProfile::new("sec-auditor", "审计安全漏洞和检查依赖")];
        let subtasks = vec![SubTask {
            description: "检查代码安全".into(),
            depends_on: vec![],
            required_skill: Some("security".into()),
            assignee: None,
            result: None,
        }];

        let (matched, unmatched, suggestions) = Orchestrator::match_roles(subtasks, &roles);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].assignee.as_deref(), Some("sec-auditor"));
        assert!(unmatched.is_empty());
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_match_roles_no_match_generates_hire_suggestion() {
        let roles = vec![RoleProfile::new("doc-gen", "生成文档")];
        let subtasks = vec![SubTask {
            description: "审计安全".into(),
            depends_on: vec![],
            required_skill: Some("security".into()),
            assignee: None,
            result: None,
        }];

        let (matched, unmatched, suggestions) = Orchestrator::match_roles(subtasks, &roles);

        assert!(matched.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].kind, SuggestionKind::HireRole);
        assert!(suggestions[0].role_name.contains("security"));
    }

    #[test]
    fn test_fire_suggest_seed_role_stale() {
        let roles = vec![RoleProfile {
            name: "stale-role".into(),
            seed_purpose: "test".into(),
            self_description: "".into(),
            created_at: "2020-01-01 00:00:00".into(),
            evolution_stage: Stage::Seed,
        }];

        let suggestions = Orchestrator::fire_suggest(&roles, 7);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, SuggestionKind::FireRole);
    }

    #[test]
    fn test_fire_suggest_skips_god() {
        let roles = vec![RoleProfile {
            name: "god".into(),
            seed_purpose: "manage".into(),
            self_description: "".into(),
            created_at: "2020-01-01 00:00:00".into(),
            evolution_stage: Stage::Expert,
        }];

        let suggestions = Orchestrator::fire_suggest(&roles, 7);
        assert!(suggestions.is_empty());
    }
}
