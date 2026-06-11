use crate::error::RoleError;
use crate::types::*;
use std::path::PathBuf;

/// Persistent storage for a single Role.
pub struct RoleStore {
    role_dir: PathBuf,
}

impl RoleStore {
    const QUEUE_FILE: &'static str = "queue/goals.json";
    const RULES_FILE: &'static str = "learning/rules.json";
    const CASES_FILE: &'static str = "learning/cases.json";
    const PREFERENCES_FILE: &'static str = "learning/preferences.json";
    const PROFILE_FILE: &'static str = "profile.toml";
    const CONTEXT_FILE: &'static str = "context/context.json";

    /// Create or open store for a named role.
    pub fn new(role_name: &str) -> Result<Self, RoleError> {
        let role_dir = roles_dir().join(role_name);
        Self::ensure_dirs(&role_dir)?;
        Ok(Self { role_dir })
    }

    /// List all role names on disk (skips the `.archived` dir and other hidden entries).
    pub fn list_roles() -> Result<Vec<String>, RoleError> {
        let dir = roles_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Skip hidden dirs (notably `.archived`).
                    if !name.starts_with('.') {
                        names.push(name.to_string());
                    }
                }
            }
        }
        Ok(names)
    }

    /// Delete this role's directory.
    pub fn delete(role_name: &str) -> Result<(), RoleError> {
        let dir = roles_dir().join(role_name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    // ─── Profile ───

    pub fn save_profile(&self, profile: &RoleProfile) -> Result<(), RoleError> {
        let content = toml::to_string_pretty(profile)
            .map_err(|e| RoleError::Toml(e.to_string()))?;
        std::fs::write(self.role_dir.join(Self::PROFILE_FILE), content)?;
        Ok(())
    }

    pub fn load_profile(&self) -> Result<Option<RoleProfile>, RoleError> {
        let path = self.role_dir.join(Self::PROFILE_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let p: RoleProfile = toml::from_str(&content)
            .map_err(|e| RoleError::Toml(e.to_string()))?;
        Ok(Some(p))
    }

    // ─── Goals ───

    pub fn save_goals(&self, goals: &[GoalNode]) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(goals)?;
        std::fs::write(self.role_dir.join(Self::QUEUE_FILE), content)?;
        Ok(())
    }

    pub fn load_goals(&self) -> Result<Vec<GoalNode>, RoleError> {
        Self::load_json_or_empty(self.role_dir.join(Self::QUEUE_FILE))
    }

    // ─── Rules ───

    pub fn save_rules(&self, rules: &[Rule]) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(rules)?;
        std::fs::write(self.role_dir.join(Self::RULES_FILE), content)?;
        Ok(())
    }

    pub fn load_rules(&self) -> Result<Vec<Rule>, RoleError> {
        Self::load_json_or_empty(self.role_dir.join(Self::RULES_FILE))
    }

    // ─── Cases ───

    pub fn save_cases(&self, cases: &[CaseEntry]) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(cases)?;
        std::fs::write(self.role_dir.join(Self::CASES_FILE), content)?;
        Ok(())
    }

    pub fn load_cases(&self) -> Result<Vec<CaseEntry>, RoleError> {
        Self::load_json_or_empty(self.role_dir.join(Self::CASES_FILE))
    }

    // ─── Preferences ───

    pub fn save_preferences(
        &self,
        prefs: &std::collections::HashMap<String, ToolPreference>,
    ) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(prefs)?;
        std::fs::write(self.role_dir.join(Self::PREFERENCES_FILE), content)?;
        Ok(())
    }

    pub fn load_preferences(
        &self,
    ) -> Result<std::collections::HashMap<String, ToolPreference>, RoleError> {
        let path = self.role_dir.join(Self::PREFERENCES_FILE);
        if !path.exists() {
            return Ok(std::collections::HashMap::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let prefs = serde_json::from_str(&content)?;
        Ok(prefs)
    }

    // ─── Context (role-scoped free-text notes/focus) ───

    pub fn save_context(&self, ctx: &RoleContext) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(ctx)?;
        std::fs::write(self.role_dir.join(Self::CONTEXT_FILE), content)?;
        Ok(())
    }

    pub fn load_context(&self) -> Result<RoleContext, RoleError> {
        let path = self.role_dir.join(Self::CONTEXT_FILE);
        if !path.exists() {
            return Ok(RoleContext::default());
        }
        let content = std::fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(RoleContext::default());
        }
        Ok(serde_json::from_str(&content)?)
    }

    // ─── Archive (safe fire) ───

    /// Move a role's directory into `~/.xuanji/roles/.archived/<name>/` (overwrites any
    /// prior archive of the same name). Nothing is deleted — restorable via [`restore`].
    pub fn archive(role_name: &str) -> Result<(), RoleError> {
        let src = roles_dir().join(role_name);
        if !src.exists() {
            return Ok(()); // idempotent: nothing to archive
        }
        let dst = archived_dir().join(role_name);
        if dst.exists() {
            std::fs::remove_dir_all(&dst)?;
        }
        std::fs::rename(&src, &dst)?;
        Ok(())
    }

    /// Restore an archived role back to the active roles directory.
    pub fn restore(role_name: &str) -> Result<(), RoleError> {
        let src = archived_dir().join(role_name);
        if !src.exists() {
            return Err(RoleError::NotFound(format!(
                "archived role '{}' not found",
                role_name
            )));
        }
        let dst = roles_dir().join(role_name);
        std::fs::rename(&src, &dst)?;
        Ok(())
    }

    /// List names of archived roles.
    pub fn list_archived() -> Result<Vec<String>, RoleError> {
        let dir = archived_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(name.to_string());
                }
            }
        }
        Ok(names)
    }

    // ─── Helpers ───

    fn ensure_dirs(dir: &PathBuf) -> Result<(), RoleError> {
        std::fs::create_dir_all(dir.join("queue"))?;
        std::fs::create_dir_all(dir.join("learning"))?;
        std::fs::create_dir_all(dir.join("teachings"))?;
        std::fs::create_dir_all(dir.join("sessions"))?;
        std::fs::create_dir_all(dir.join("context"))?;
        Ok(())
    }

    fn load_json_or_empty<T: serde::de::DeserializeOwned>(
        path: PathBuf,
    ) -> Result<Vec<T>, RoleError> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&content)?)
    }
}

fn roles_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xuanji")
        .join("roles")
}

fn archived_dir() -> PathBuf {
    roles_dir().join(".archived")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Helper: create a RoleStore pointing at a temp dir by overriding roles_dir.
    /// Since roles_dir() uses the home dir, we test serialization/deserialization
    /// logic directly with temporary files.
    fn create_test_store() -> (TempDir, RoleStore) {
        let dir = TempDir::new().unwrap();
        let role_dir = dir.path().join("test-role");
        let store = RoleStore {
            role_dir: role_dir.clone(),
        };
        RoleStore::ensure_dirs(&role_dir).unwrap();
        (dir, store)
    }

    #[test]
    fn test_profile_roundtrip() {
        let (_dir, store) = create_test_store();
        let profile = RoleProfile::new("test", "audit security");

        store.save_profile(&profile).unwrap();
        let loaded = store.load_profile().unwrap().unwrap();

        assert_eq!(loaded.name, "test");
        assert_eq!(loaded.seed_purpose, "audit security");
        assert_eq!(loaded.evolution_stage, Stage::Seed);
    }

    #[test]
    fn test_goals_roundtrip() {
        let (_dir, store) = create_test_store();
        let goals = vec![
            GoalNode::new("task 1", 0.9, GoalSource::User),
            GoalNode::new("task 2", 0.5, GoalSource::SelfDiscovered),
        ];

        store.save_goals(&goals).unwrap();
        let loaded = store.load_goals().unwrap();

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].description, "task 1");
        assert_eq!(loaded[0].created_by, GoalSource::User);
    }

    #[test]
    fn test_rules_roundtrip() {
        let (_dir, store) = create_test_store();
        let rules = vec![Rule {
            id: "r1".into(),
            condition: "when building".into(),
            action: "use --release".into(),
            confidence: 0.8,
            source_case_id: None,
            validated_count: 2,
            created_at: "now".into(),
        }];

        store.save_rules(&rules).unwrap();
        let loaded = store.load_rules().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].condition, "when building");
    }

    #[test]
    fn test_cases_roundtrip() {
        let (_dir, store) = create_test_store();
        let cases = vec![CaseEntry {
            id: "c1".into(),
            task_description: "profile build".into(),
            context_tags: vec!["rust".into(), "build".into()],
            strategy_used: "cargo build --timings".into(),
            outcome: CaseOutcome::Success,
            lessons: "Use --timings flag".into(),
            created_at: "now".into(),
        }];

        store.save_cases(&cases).unwrap();
        let loaded = store.load_cases().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].task_description, "profile build");
    }

    #[test]
    fn test_preferences_roundtrip() {
        let (_dir, store) = create_test_store();
        let mut prefs = HashMap::new();
        let mut tp = ToolPreference::new("shell.run");
        tp.record_call(true, 500);
        prefs.insert("shell.run".into(), tp);

        store.save_preferences(&prefs).unwrap();
        let loaded = store.load_preferences().unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["shell.run"].total_calls, 1);
        assert_eq!(loaded["shell.run"].success_rate, 1.0);
    }

    #[test]
    fn test_load_empty_returns_empty() {
        let (_dir, store) = create_test_store();

        let goals = store.load_goals().unwrap();
        assert!(goals.is_empty());

        let rules = store.load_rules().unwrap();
        assert!(rules.is_empty());

        let prefs = store.load_preferences().unwrap();
        assert!(prefs.is_empty());
    }
}
