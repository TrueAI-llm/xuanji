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

        // Generate teachings
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
                        Ok(_result) => {
                            tracing::info!(
                                "Role '{}' subtask done",
                                self.profile.name
                            );
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

            // Update evolution stage
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_creation() {
        let role = Role::new("test-integration", "test purpose").unwrap();
        assert_eq!(role.profile.name, "test-integration");
        assert_eq!(role.profile.evolution_stage, Stage::Seed);
        assert!(role.goals().is_empty());
        assert!(role.rules().is_empty());
        assert!(role.cases().is_empty());
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
        assert_eq!(role.goals()[0].description, "manual task");
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
        assert_eq!(restored.goals()[0].description, "goal 1");
        RoleStore::delete("test-persist").ok();
    }
}
