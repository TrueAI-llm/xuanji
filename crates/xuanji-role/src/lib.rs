pub mod discover;
pub mod error;
pub mod reflect;
pub mod store;
pub mod teaching;
pub mod types;

use std::collections::HashMap;
use xuanji_agent::Agent;

pub use discover::DiscoverEngine;
pub use error::RoleError;
pub use reflect::LearningEngine;
pub use store::RoleStore;
pub use teaching::TeachingLibrary;
pub use types::*;

// ─── Orchestrator ───

/// Handles role matching, hire suggestion, and fire suggestion for the God Role.
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
                // Try to match skill to role purpose
                let best_role = available_roles
                    .iter()
                    .find(|r| {
                        let lower_purpose = r.seed_purpose.to_lowercase();
                        match skill.as_str() {
                            "security" => lower_purpose.contains("安全") || lower_purpose.contains("security"),
                            "performance" => lower_purpose.contains("性能") || lower_purpose.contains("performance"),
                            "documentation" => lower_purpose.contains("文档") || lower_purpose.contains("doc"),
                            "testing" => lower_purpose.contains("测试") || lower_purpose.contains("test"),
                            "devops" => lower_purpose.contains("部署") || lower_purpose.contains("devops"),
                            "build" => lower_purpose.contains("构建") || lower_purpose.contains("build"),
                            _ => lower_purpose.contains(skill),
                        }
                    });

                if let Some(role) = best_role {
                    let mut matched_task = task;
                    matched_task.assignee = Some(role.name.clone());
                    matched.push(matched_task);
                } else {
                    // No matching role → suggest hire
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
                // No skill requirement → God Role handles it
                let mut own_task = task;
                own_task.assignee = Some("god".into());
                matched.push(own_task);
            }
        }

        (matched, unmatched, suggestions)
    }

    /// Evaluate roles and suggest firing low-performing ones.
    pub fn fire_suggest(roles: &[RoleProfile]) -> Vec<OrchestrationSuggestion> {
        let mut suggestions = Vec::new();

        for role in roles {
            if role.name == "god" {
                continue;
            }

            // Check seed stage with no progress
            if role.evolution_stage == Stage::Seed {
                // A seed role that hasn't evolved might need redefinition
                // For now, only suggest fire for very stale roles
                if is_stale(&role.created_at, 7) {
                    suggestions.push(OrchestrationSuggestion {
                        kind: SuggestionKind::FireRole,
                        role_name: role.name.clone(),
                        purpose: None,
                        reason: format!(
                            "角色 '{}' 处于 Seed 阶段超过 7 天且无进化，建议 fire 或重新定义 purpose",
                            role.name
                        ),
                    });
                }
            }
        }

        suggestions
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
    goals: Vec<GoalNode>,
    rules: Vec<Rule>,
    cases: Vec<CaseEntry>,
    preferences: HashMap<String, ToolPreference>,
    pub teaching_lib: TeachingLibrary,
    active: bool,
    outcomes: Vec<GoalOutcome>,
}

impl Role {
    pub fn new(name: &str, seed_purpose: &str) -> Result<Self, RoleError> {
        let profile = RoleProfile::new(name, seed_purpose);
        let store = RoleStore::new(name)?;

        let goals = store.load_goals()?;
        let rules = store.load_rules()?;
        let cases = store.load_cases()?;
        let preferences = store.load_preferences()?;
        let teaching_lib = TeachingLibrary::load()?;

        store.save_profile(&profile)?;

        Ok(Self {
            profile,
            store,
            agent: None,
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

    // ─── Orchestrated Cycle (God Role) ───

    /// Run an orchestrated cycle: decompose → match → dispatch → aggregate.
    /// Used by God Role to coordinate other roles.
    pub async fn run_orchestrated_cycle(
        &mut self,
        goal_description: &str,
    ) -> Result<CycleResult, RoleError> {
        if !self.active {
            self.activate();
        }

        let mut suggestions = Vec::new();

        // Load all available roles
        let role_names = RoleStore::list_roles().unwrap_or_default();
        let other_roles: Vec<RoleProfile> = role_names
            .iter()
            .filter(|n| *n != &self.profile.name)
            .filter_map(|n| {
                RoleStore::new(n).ok()?.load_profile().ok()?
            })
            .collect();

        // Fire suggestions
        suggestions.extend(Orchestrator::fire_suggest(&other_roles));

        // Decompose
        let subtasks = DiscoverEngine::decompose(goal_description);

        // Match
        let (matched, unmatched, role_suggestions) =
            Orchestrator::match_roles(subtasks, &other_roles);
        suggestions.extend(role_suggestions);

        // Dispatch matched subtasks to assigned roles
        let mut dispatched = Vec::new();
        let mut success = true;

        for task in &matched {
            if let Some(ref assignee) = task.assignee {
                if assignee == "god" || assignee == &self.profile.name {
                    // Execute locally
                    if let Some(ref mut agent) = self.agent {
                        match agent.run(task.description.clone()).await {
                            Ok(_) => {
                                tracing::info!(
                                    "God Role executed: {}",
                                    task.description
                                );
                            }
                            Err(e) => {
                                success = false;
                                tracing::warn!(
                                    "God Role task failed: {}",
                                    e
                                );
                            }
                        }
                    }
                    dispatched.push("god".to_string());
                } else {
                    // Delegate to another role
                    let mut worker =
                        Role::new(assignee, "")?;
                    worker.activate();
                    worker.add_user_goal(&task.description);
                    match worker.run_cycle().await {
                        Ok(Some(outcome)) => {
                            if outcome.success {
                                dispatched.push(assignee.clone());
                            } else {
                                success = false;
                            }
                            worker.persist()?;
                        }
                        Ok(None) => {
                            // Still dispatched, just no goal executed
                        }
                        Err(e) => {
                            success = false;
                            tracing::warn!(
                                "Role '{}' failed: {}",
                                assignee,
                                e
                            );
                        }
                    }
                }
            }
        }

        // Store unmatched as suggestions
        for task in &unmatched {
            suggestions.push(OrchestrationSuggestion {
                kind: SuggestionKind::HireRole,
                role_name: task
                    .required_skill
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string(),
                purpose: task
                    .required_skill
                    .as_deref()
                    .map(|s| skill_translate(s)),
                reason: format!(
                    "未匹配的子任务: {}（需要创建对应角色来执行）",
                    task.description
                ),
            });
        }

        let outcome = GoalOutcome {
            goal_id: format!("goal-outcome-{}", chrono_now_compact()),
            success,
            summary: goal_description.to_string(),
            tool_calls_count: matched.len() as u32,
            tokens_used: 0,
            lessons: if unmatched.is_empty() {
                String::new()
            } else {
                format!("{} 个子任务无匹配角色，已生成 hire 建议", unmatched.len())
            },
        };

        self.outcomes.push(outcome.clone());

        // REFLECT + LEARN
        for o in self.outcomes.drain(..) {
            LearningEngine::reflect_on_goal(
                &o,
                &mut self.rules,
                &mut self.cases,
                &mut self.preferences,
            );
        }

        let _published = LearningEngine::generate_teaching(
            &self.profile.name,
            &self.rules,
            &mut self.teaching_lib,
        )?;

        self.persist()?;

        Ok(CycleResult {
            outcome: Some(outcome),
            suggestions,
            dispatched_to: dispatched,
        })
    }

    // ─── Simple Cycle (non-orchestrating roles) ───

    pub async fn run_cycle(&mut self) -> Result<Option<GoalOutcome>, RoleError> {
        if !self.active {
            self.activate();
        }

        // Phase 1: REFLECT
        for outcome in self.outcomes.drain(..) {
            LearningEngine::reflect_on_goal(
                &outcome,
                &mut self.rules,
                &mut self.cases,
                &mut self.preferences,
            );
        }

        let _published = LearningEngine::generate_teaching(
            &self.profile.name,
            &self.rules,
            &mut self.teaching_lib,
        )?;

        // Phase 2: DISCOVER
        let candidates =
            DiscoverEngine::discover(&self.profile, &self.rules, &self.cases);

        // Phase 3: PRIORITIZE
        let scored = DiscoverEngine::prioritize(candidates, 0);

        // Phase 4: DECOMPOSE + EXECUTE
        if let Some(top) = scored.first() {
            let goal =
                GoalNode::new(&top.description, top.score, GoalSource::SelfDiscovered);
            self.goals.push(goal);

            let subtasks = DiscoverEngine::decompose(&top.description);

            let mut success = true;
            let mut total_calls = 0u32;

            for subtask in &subtasks {
                if let Some(ref mut agent) = self.agent {
                    match agent.run(subtask.description.clone()).await {
                        Ok(_) => {
                            total_calls += 1;
                        }
                        Err(e) => {
                            success = false;
                            tracing::warn!(
                                "Role '{}' subtask failed: {}",
                                self.profile.name,
                                e
                            );
                        }
                    }
                }
            }

            let outcome = GoalOutcome {
                goal_id: format!("goal-outcome-{}", chrono_now_compact()),
                success,
                summary: top.description.clone(),
                tool_calls_count: total_calls,
                tokens_used: 0,
                lessons: if success {
                    String::new()
                } else {
                    "Task execution failed".into()
                },
            };

            self.outcomes.push(outcome.clone());
            self.profile.evolution_stage = self.profile.evolution_stage.promote(
                self.cases.len(),
                self.rules.iter().filter(|r| r.confidence >= 0.7).count(),
            );
            self.store.save_profile(&self.profile)?;
            self.persist()?;

            return Ok(Some(outcome));
        }

        self.persist()?;
        Ok(None)
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

fn chrono_now_compact() -> String {
    chrono::Local::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
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

    // ─── Orchestrator tests ───

    #[test]
    fn test_match_roles_security() {
        let roles = vec![
            RoleProfile::new("sec-auditor", "审计安全漏洞和检查依赖"),
        ];
        let subtasks = vec![SubTask {
            description: "检查代码安全".into(),
            depends_on: vec![],
            required_skill: Some("security".into()),
            assignee: None,
            result: None,
        }];

        let (matched, unmatched, suggestions) =
            Orchestrator::match_roles(subtasks, &roles);

        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].assignee.as_deref(), Some("sec-auditor"));
        assert!(unmatched.is_empty());
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_match_roles_no_match_generates_hire_suggestion() {
        let roles = vec![
            RoleProfile::new("doc-gen", "生成文档"),
        ];
        let subtasks = vec![SubTask {
            description: "审计安全".into(),
            depends_on: vec![],
            required_skill: Some("security".into()),
            assignee: None,
            result: None,
        }];

        let (matched, unmatched, suggestions) =
            Orchestrator::match_roles(subtasks, &roles);

        assert!(matched.is_empty());
        assert_eq!(unmatched.len(), 1);
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].kind, SuggestionKind::HireRole);
        assert!(suggestions[0].role_name.contains("security"));
    }

    #[test]
    fn test_fire_suggest_seed_role_stale() {
        let roles = vec![
            RoleProfile {
                name: "stale-role".into(),
                seed_purpose: "test".into(),
                self_description: "".into(),
                created_at: "2020-01-01 00:00:00".into(),
                evolution_stage: Stage::Seed,
            },
        ];

        let suggestions = Orchestrator::fire_suggest(&roles);
        assert!(!suggestions.is_empty());
        assert_eq!(suggestions[0].kind, SuggestionKind::FireRole);
    }

    #[test]
    fn test_fire_suggest_skips_god() {
        let roles = vec![
            RoleProfile {
                name: "god".into(),
                seed_purpose: "manage".into(),
                self_description: "".into(),
                created_at: "2020-01-01 00:00:00".into(),
                evolution_stage: Stage::Expert,
            },
        ];

        let suggestions = Orchestrator::fire_suggest(&roles);
        assert!(suggestions.is_empty());
    }
}
