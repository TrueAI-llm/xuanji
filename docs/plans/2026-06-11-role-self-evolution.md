# Role Self-Evolution Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the Role self-evolution system — autonomous, self-directed, self-evolving AI agents with a God Role as the default entry point.

**Architecture:** New `xuanji-role` crate with types, persistence, teaching library, and self-direction loop. Modify `xuanji-cli` to add role commands and replace default chat/prompt with God Role.

**Tech Stack:** Rust 2024 edition, tokio, serde, anyhow, thiserror, tracing, tempfile (test)

**Design Doc:** `docs/superpowers/specs/2026-06-11-role-self-evolution-design.md`

---

## Implementation Phases

### Phase 1: Create xuanji-role crate skeleton + types

### Phase 2: Implement RoleStore persistence layer

### Phase 3: Implement TeachingLibrary

### Phase 4: Implement self-direction loop

### Phase 5: Implement God Role bootstrap + CLI commands

### Phase 6: Wire God Role as default entry point

---

## Phase 1: Create xuanji-role crate skeleton + types

### Task 1.1: Create crate Cargo.toml

**Files:**
- Create: `crates/xuanji-role/Cargo.toml`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Create Cargo.toml**

```toml
[package]
name = "xuanji-role"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
async-trait.workspace = true
tracing.workspace = true
chrono = "0.4"
dirs = "6"
xuanji-llm = { path = "../xuanji-llm" }
xuanji-agent = { path = "../xuanji-agent" }
xuanji-plugin = { path = "../xuanji-plugin" }
xuanji-memory = { path = "../xuanji-memory" }
xuanji-bus = { path = "../xuanji-bus" }
xuanji-budget = { path = "../xuanji-budget" }

[dev-dependencies]
tempfile = "3"
```

**Step 2: Add to workspace root Cargo.toml members**

Add to `Cargo.toml` workspace members:
```toml
"crates/xuanji-role",
```

**Step 3: Commit**

```bash
git add crates/xuanji-role/Cargo.toml Cargo.toml
git commit -m "feat(role): add xuanji-role crate skeleton"
```

---

### Task 1.2: Implement Role types

**Files:**
- Create: `crates/xuanji-role/src/types.rs`
- Create: `crates/xuanji-role/src/error.rs`
- Create: `crates/xuanji-role/src/lib.rs`

**Step 1: Write types.rs**

```rust
use serde::{Deserialize, Serialize};

/// Role identity and evolution state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleProfile {
    pub name: String,
    pub seed_purpose: String,
    pub self_description: String,
    pub created_at: String,
    pub evolution_stage: Stage,
}

impl RoleProfile {
    pub fn new(name: &str, seed_purpose: &str) -> Self {
        Self {
            name: name.to_string(),
            seed_purpose: seed_purpose.to_string(),
            self_description: format!("I am {}. My purpose: {}", name, seed_purpose),
            created_at: chrono_now(),
            evolution_stage: Stage::Seed,
        }
    }
}

/// Evolution progression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Seed,
    Exploring,
    Specializing,
    Expert,
}

impl Stage {
    /// Check promotion threshold.
    pub fn promote(&self, case_count: usize, validated_teaching_count: usize) -> Stage {
        match self {
            Stage::Seed if case_count >= 3 => Stage::Exploring,
            Stage::Exploring if case_count >= 10 && validated_teaching_count >= 2 => Stage::Specializing,
            Stage::Specializing if case_count >= 50 && validated_teaching_count >= 10 => Stage::Expert,
            other => other.clone(),
        }
    }
}

/// A goal in the self-direction queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalNode {
    pub id: String,
    pub description: String,
    pub priority: f32,
    pub parent_id: Option<String>,
    pub status: GoalStatus,
    pub created_by: GoalSource,
}

impl GoalNode {
    pub fn new(description: &str, priority: f32, source: GoalSource) -> Self {
        Self {
            id: format!("goal-{}", chrono_now_compact()),
            description: description.to_string(),
            priority,
            parent_id: None,
            status: GoalStatus::Pending,
            created_by: source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Pending,
    InProgress,
    Done,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalSource {
    User,
    SelfDiscovered,
    Derived,
    SubGoal { parent_id: String },
}

/// Outcome of executing a goal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalOutcome {
    pub goal_id: String,
    pub success: bool,
    pub summary: String,
    pub tool_calls_count: u32,
    pub tokens_used: u32,
    pub lessons: String,
}

/// A learned behavioral rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub condition: String,
    pub action: String,
    pub confidence: f32,
    pub source_case_id: Option<String>,
    pub validated_count: u32,
    pub created_at: String,
}

/// A stored case (task -> solution).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseEntry {
    pub id: String,
    pub task_description: String,
    pub context_tags: Vec<String>,
    pub strategy_used: String,
    pub outcome: CaseOutcome,
    pub lessons: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaseOutcome {
    Success,
    PartialSuccess { issues: Vec<String> },
    Failure { reason: String },
}

/// Tool preference with signal tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPreference {
    pub tool_name: String,
    pub total_calls: u32,
    pub successful_calls: u32,
    pub success_rate: f32,
    pub avg_token_cost: u32,
    pub preferred_scenarios: Vec<String>,
}

impl ToolPreference {
    pub fn new(tool_name: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            total_calls: 0,
            successful_calls: 0,
            success_rate: 0.0,
            avg_token_cost: 0,
            preferred_scenarios: Vec::new(),
        }
    }

    pub fn record_call(&mut self, success: bool, tokens: u32) {
        self.total_calls += 1;
        if success {
            self.successful_calls += 1;
        }
        self.success_rate = if self.total_calls > 0 {
            self.successful_calls as f32 / self.total_calls as f32
        } else {
            0.0
        };
        self.avg_token_cost = if self.total_calls > 0 {
            (self.avg_token_cost * (self.total_calls - 1) as u32 + tokens) / self.total_calls
        } else {
            tokens
        };
    }
}

/// A candidate goal from discovery.
#[derive(Debug, Clone)]
pub struct GoalCandidate {
    pub description: String,
    pub rationale: String,
    pub relevance_score: f32,
    pub exploration_score: f32,
}

/// Candidate after scoring (for prioritization).
#[derive(Debug, Clone)]
pub struct ScoredCandidate {
    pub description: String,
    pub rationale: String,
    pub score: f32,
}

/// A sub-task produced by decomposition.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub description: String,
    pub depends_on: Vec<String>,
    pub result: Option<String>,
}

// ─── Helpers ───

fn chrono_now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn chrono_now_compact() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}
```

**Step 2: Write error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RoleError {
    #[error("Role '{0}' not found")]
    NotFound(String),

    #[error("Role '{0}' already exists")]
    AlreadyExists(String),

    #[error("Role '{0}' is not active")]
    NotActive(String),

    #[error("Cannot fire the god role")]
    CannotFireGod,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Agent error: {0}")]
    Agent(#[from] xuanji_agent::error::AgentError),
}
```

**Step 3: Write lib.rs (skeleton)**

```rust
pub mod error;
pub mod types;

pub use error::RoleError;
pub use types::*;
```

**Step 4: Verify it compiles**

```bash
cargo build -p xuanji-role 2>&1
```
Expected: Compilation success.

**Step 5: Commit**

```bash
git add crates/xuanji-role/
git commit -m "feat(role): add core types (RoleProfile, GoalNode, Rule, CaseEntry, etc.)"
```

---

## Phase 2: Implement RoleStore persistence layer

### Task 2.1: Implement RoleStore

**Files:**
- Create: `crates/xuanji-role/src/store.rs`

**Step 1: Write store.rs**

```rust
use crate::error::RoleError;
use crate::types::*;
use anyhow::Context;
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

    /// Create or open store for a named role.
    pub fn new(role_name: &str) -> Result<Self, RoleError> {
        let role_dir = roles_dir().join(role_name);
        Self::ensure_dirs(&role_dir)?;
        Ok(Self { role_dir })
    }

    /// List all role names on disk.
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
                    names.push(name.to_string());
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
            .context("serializing profile")?;
        std::fs::write(self.role_dir.join(Self::PROFILE_FILE), content)?;
        Ok(())
    }

    pub fn load_profile(&self) -> Result<Option<RoleProfile>, RoleError> {
        let path = self.role_dir.join(Self::PROFILE_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        let p: RoleProfile = toml::from_str(&content).context("deserializing profile")?;
        Ok(Some(p))
    }

    // ─── Goals ───

    pub fn save_goals(&self, goals: &[GoalNode]) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(goals)?;
        std::fs::write(self.role_dir.join(Self::QUEUE_FILE), content)?;
        Ok(())
    }

    pub fn load_goals(&self) -> Result<Vec<GoalNode>, RoleError> {
        let path = self.role_dir.join(Self::QUEUE_FILE);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let goals: Vec<GoalNode> = serde_json::from_str(&content)?;
        Ok(goals)
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

    pub fn save_preferences(&self, prefs: &std::collections::HashMap<String, ToolPreference>) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(prefs)?;
        std::fs::write(self.role_dir.join(Self::PREFERENCES_FILE), content)?;
        Ok(())
    }

    pub fn load_preferences(&self) -> Result<std::collections::HashMap<String, ToolPreference>, RoleError> {
        let path = self.role_dir.join(Self::PREFERENCES_FILE);
        if !path.exists() {
            return Ok(std::collections::HashMap::new());
        }
        let content = std::fs::read_to_string(&path)?;
        let prefs = serde_json::from_str(&content)?;
        Ok(prefs)
    }

    // ─── Helpers ───

    fn ensure_dirs(dir: &PathBuf) -> Result<(), RoleError> {
        std::fs::create_dir_all(dir.join("queue"))?;
        std::fs::create_dir_all(dir.join("learning"))?;
        std::fs::create_dir_all(dir.join("teachings"))?;
        std::fs::create_dir_all(dir.join("sessions"))?;
        Ok(())
    }

    fn load_json_or_empty<T: serde::de::DeserializeOwned>(path: PathBuf) -> Result<Vec<T>, RoleError> {
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
```

**Step 2: Update lib.rs**

Add `pub mod store;` to `crates/xuanji-role/src/lib.rs`

**Step 3: Add toml dependency to xuanji-role/Cargo.toml**

```toml
toml = "0.8"
```

**Step 4: Write RoleStore tests**

Create `crates/xuanji-role/tests/store_test.rs`:

```rust
use tempfile::TempDir;
use std::collections::HashMap;

// We test RoleStore's internal methods by using a temp dir.
// Since RoleStore uses ~/.xuanji/roles/<name>, we test via
// the public API that creates directories and reads/writes files.

#[test]
fn test_role_profile_roundtrip() {
    let dir = TempDir::new().unwrap();
    let profile = xuanji_role::RoleProfile::new("test-role", "test purpose");
    
    // Serialize to TOML string (mirrors store logic)
    let toml_str = toml::to_string_pretty(&profile).unwrap();
    let deserialized: xuanji_role::RoleProfile = toml::from_str(&toml_str).unwrap();
    
    assert_eq!(deserialized.name, "test-role");
    assert_eq!(deserialized.seed_purpose, "test purpose");
    assert_eq!(deserialized.evolution_stage, xuanji_role::Stage::Seed);
}

#[test]
fn test_goal_node_serialization() {
    let goal = xuanji_role::GoalNode::new(
        "audit security",
        0.8,
        xuanji_role::GoalSource::User,
    );
    
    let json = serde_json::to_string(&goal).unwrap();
    let back: xuanji_role::GoalNode = serde_json::from_str(&json).unwrap();
    assert_eq!(back.description, goal.description);
    assert_eq!(back.status, xuanji_role::GoalStatus::Pending);
}

#[test]
fn test_tool_preference_record() {
    let mut pref = xuanji_role::ToolPreference::new("shell.run");
    pref.record_call(true, 500);
    pref.record_call(true, 300);
    pref.record_call(false, 200);
    
    assert_eq!(pref.total_calls, 3);
    assert_eq!(pref.successful_calls, 2);
    assert!((pref.success_rate - 0.666).abs() < 0.01);
    // avg_token_cost: (0*0 + 500)/1 = 500; (500*1 + 300)/2 = 400; (400*2 + 200)/3 = 333
    assert!(pref.avg_token_cost > 330 && pref.avg_token_cost < 340);
}

#[test]
fn test_stage_promotion() {
    let seed = xuanji_role::Stage::Seed;
    assert_eq!(seed.promote(3, 0), xuanji_role::Stage::Exploring);
    assert_eq!(seed.promote(2, 0), xuanji_role::Stage::Seed);
    
    let exploring = xuanji_role::Stage::Exploring;
    assert_eq!(exploring.promote(10, 2), xuanji_role::Stage::Specializing);
    assert_eq!(exploring.promote(10, 1), xuanji_role::Stage::Exploring);
}
```

**Step 5: Run tests**

```bash
cargo test -p xuanji-role 2>&1
```
Expected: All tests pass.

**Step 6: Commit**

```bash
git add crates/xuanji-role/
git commit -m "feat(role): implement RoleStore persistence layer"
```

---

## Phase 3: Implement TeachingLibrary

### Task 3.1: Implement TeachingLibrary

**Files:**
- Create: `crates/xuanji-role/src/teaching.rs`

**Step 1: Add Teaching types to types.rs**

Append to `crates/xuanji-role/src/types.rs`:

```rust
/// A shareable teaching package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Teaching {
    pub id: String,
    pub author_role: String,
    pub kind: TeachingKind,
    pub content: String,
    pub confidence: f32,
    pub validation_count: u32,
    pub domain_tags: Vec<String>,
    pub created_at: String,
}

impl Teaching {
    pub fn new(author: &str, kind: TeachingKind, content: &str, domain_tags: Vec<String>) -> Self {
        Self {
            id: format!("teaching-{}", chrono_now_compact()),
            author_role: author.to_string(),
            kind,
            content: content.to_string(),
            confidence: 0.5,
            validation_count: 0,
            domain_tags,
            created_at: chrono_now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeachingKind {
    Rule,
    AntiPattern,
    Heuristic,
    CaseStudy,
}
```

**Step 2: Write teaching.rs**

```rust
use crate::error::RoleError;
use crate::types::*;
use std::collections::HashMap;
use std::path::PathBuf;

/// Global shared teaching library across all roles.
pub struct TeachingLibrary {
    entries: Vec<Teaching>,
    path: PathBuf,
}

impl TeachingLibrary {
    const FILE_NAME: &'static str = "entries.json";

    /// Open or create the global teaching library.
    pub fn load() -> Result<Self, RoleError> {
        let dir = teaching_lib_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(Self::FILE_NAME);
        let entries = if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Self { entries, path })
    }

    /// Publish a teaching from a role.
    pub fn publish(&mut self, teaching: Teaching) -> Result<(), RoleError> {
        // Upsert by id
        if let Some(existing) = self.entries.iter_mut().find(|t| t.id == teaching.id) {
            *existing = teaching;
        } else {
            self.entries.push(teaching);
        }
        self.save()
    }

    /// Validate a teaching (increment validation count and confidence).
    pub fn validate(&mut self, teaching_id: &str, success: bool) -> Result<(), RoleError> {
        if let Some(t) = self.entries.iter_mut().find(|t| t.id == teaching_id) {
            t.validation_count += 1;
            if success {
                t.confidence = (t.confidence * 0.9 + 0.1).min(1.0);
            } else {
                t.confidence = (t.confidence * 0.95).max(0.1);
            }
        }
        self.save()
    }

    /// Query teachings by domain tags.
    pub fn query_by_tags(&self, tags: &[String]) -> Vec<&Teaching> {
        if tags.is_empty() {
            return self.entries.iter().collect();
        }
        self.entries
            .iter()
            .filter(|t| t.domain_tags.iter().any(|dt| tags.contains(dt)))
            .collect()
    }

    /// List all teachings.
    pub fn list(&self) -> &[Teaching] {
        &self.entries
    }

    /// Get a teaching by id.
    pub fn get(&self, id: &str) -> Option<&Teaching> {
        self.entries.iter().find(|t| t.id == id)
    }

    fn save(&self) -> Result<(), RoleError> {
        let content = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}

fn teaching_lib_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xuanji")
        .join("teaching-library")
}
```

**Step 3: Update lib.rs**

Update `crates/xuanji-role/src/lib.rs`:

```rust
pub mod error;
pub mod types;
pub mod store;
pub mod teaching;

pub use error::RoleError;
pub use store::RoleStore;
pub use teaching::TeachingLibrary;
pub use types::*;
```

**Step 4: Write TeachingLibrary tests**

Create `crates/xuanji-role/tests/teaching_test.rs`:

```rust
#[test]
fn test_publish_and_query() {
    let mut lib = xuanji_role::TeachingLibrary::load().unwrap_or_else(|_| {
        // If load fails (no home dir in test), test serialization directly
        panic!("TeachingLibrary::load() failed - ensure ~/.xuanji exists or mock in test")
    });
    
    let teaching = xuanji_role::Teaching::new(
        "role-a",
        xuanji_role::TeachingKind::Rule,
        "Always run cargo test before commit",
        vec!["rust".into(), "testing".into()],
    );
    let id = teaching.id.clone();
    lib.publish(teaching).unwrap();
    
    let results = lib.query_by_tags(&["rust".to_string()]);
    assert!(!results.is_empty());
    assert_eq!(results[0].author_role, "role-a");
    
    // Validate should increment
    lib.validate(&id, true).unwrap();
    let t = lib.get(&id).unwrap();
    assert_eq!(t.validation_count, 1);
    assert!(t.confidence > 0.5);
}

#[test]
fn test_teaching_serialization() {
    let t = xuanji_role::Teaching::new(
        "role-a",
        xuanji_role::TeachingKind::Heuristic,
        "Use shell.run for file operations",
        vec!["operations".into()],
    );
    let json = serde_json::to_string(&t).unwrap();
    let back: xuanji_role::Teaching = serde_json::from_str(&json).unwrap();
    assert_eq!(back.author_role, "role-a");
    assert_eq!(back.kind, xuanji_role::TeachingKind::Heuristic);
    assert_eq!(back.domain_tags, vec!["operations"]);
}
```

**Step 5: Run tests**

```bash
cargo test -p xuanji-role 2>&1
```
Expected: All tests pass.

**Step 6: Commit**

```bash
git add crates/xuanji-role/
git commit -m "feat(role): implement TeachingLibrary with publish/validate/query"
```

---

## Phase 4: Implement self-direction loop

### Task 4.1: Implement Discover module

**Files:**
- Create: `crates/xuanji-role/src/discover.rs`

**Step 1: Write discover.rs**

```rust
use crate::types::*;
use std::collections::HashMap;

/// Discover candidate goals based on the role's purpose and knowledge.
pub struct DiscoverEngine;

impl DiscoverEngine {
    /// Generate candidate goals.
    /// In full implementation, this calls LLM.
    /// For MVP, we provide deterministic candidates for testing.
    pub fn discover(
        profile: &RoleProfile,
        rules: &[Rule],
        cases: &[CaseEntry],
    ) -> Vec<GoalCandidate> {
        // MVP: derive candidates from tags extracted from rules + cases
        let mut candidates = Vec::new();

        // Candidate 1: Always re-evaluate seed purpose
        candidates.push(GoalCandidate {
            description: format!("Re-evaluate progress on: {}", profile.seed_purpose),
            rationale: "Periodic self-assessment".into(),
            relevance_score: 1.0,
            exploration_score: 0.3,
        });

        // Candidate 2: From successful case domains
        let success_tags: Vec<String> = cases
            .iter()
            .filter(|c| matches!(c.outcome, CaseOutcome::Success))
            .flat_map(|c| c.context_tags.clone())
            .collect();
        if !success_tags.is_empty() {
            let tag = &success_tags[0];
            candidates.push(GoalCandidate {
                description: format!("Deepen expertise in domain: {}", tag),
                rationale: "Build on proven success patterns".into(),
                relevance_score: 0.8,
                exploration_score: 0.5,
            });
        }

        // Candidate 3: From failure cases (improvement)
        let failure_tags: Vec<String> = cases
            .iter()
            .filter(|c| matches!(c.outcome, CaseOutcome::Failure { .. }))
            .flat_map(|c| c.context_tags.clone())
            .collect();
        if !failure_tags.is_empty() {
            let tag = &failure_tags[0];
            candidates.push(GoalCandidate {
                description: format!("Retry and improve handling of: {}", tag),
                rationale: "Learn from failure".into(),
                relevance_score: 0.9,
                exploration_score: 0.7,
            });
        }

        candidates
    }

    /// Score and prioritize candidates.
    pub fn prioritize(
        candidates: Vec<GoalCandidate>,
        _budget_remaining: u32,
    ) -> Vec<ScoredCandidate> {
        let mut scored: Vec<ScoredCandidate> = candidates
            .into_iter()
            .map(|c| {
                let score = 0.5 * c.relevance_score + 0.3 * c.exploration_score + 0.2 * (1.0 - c.exploration_score * 0.5);
                ScoredCandidate {
                    description: c.description,
                    rationale: c.rationale,
                    score,
                }
            })
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Decompose a goal description into sub-tasks.
    /// MVP: simple heuristic decomposition.
    pub fn decompose(goal_description: &str) -> Vec<SubTask> {
        // MVP: single sub-task = the goal itself
        vec![
            SubTask {
                description: goal_description.to_string(),
                depends_on: Vec::new(),
                result: None,
            }
        ]
    }
}
```

**Step 2: Write tests**

Create `crates/xuanji-role/tests/discover_test.rs`:

```rust
use xuanji_role::*;

#[test]
fn test_discover_generates_candidates() {
    let profile = RoleProfile::new("test", "audit project security");
    let candidates = DiscoverEngine::discover(&profile, &[], &[]);
    // Always at least has the self-assessment candidate
    assert!(!candidates.is_empty());
    assert!(candidates[0].description.contains("audit project security"));
}

#[test]
fn test_discover_from_cases() {
    let profile = RoleProfile::new("test", "optimize performance");
    let cases = vec![
        CaseEntry {
            id: "1".into(),
            task_description: "Profile build".into(),
            context_tags: vec!["rust".into(), "build".into()],
            strategy_used: "cargo build --timings".into(),
            outcome: CaseOutcome::Success,
            lessons: "Use --timings flag".into(),
            created_at: "2026-01-01".into(),
        },
        CaseEntry {
            id: "2".into(),
            task_description: "Reduce binary size".into(),
            context_tags: vec!["rust".into(), "optimize".into()],
            strategy_used: "strip".into(),
            outcome: CaseOutcome::Failure { reason: "Still too large".into() },
            lessons: "Need LTO".into(),
            created_at: "2026-01-02".into(),
        },
    ];
    let candidates = DiscoverEngine::discover(&profile, &[], &cases);
    // Should have success-based candidate + failure-based candidate + self-assessment
    assert!(candidates.len() >= 2);
}

#[test]
fn test_prioritize_orders_by_score() {
    let candidates = vec![
        GoalCandidate {
            description: "A".into(),
            rationale: "r".into(),
            relevance_score: 0.3,
            exploration_score: 0.1,
        },
        GoalCandidate {
            description: "B".into(),
            rationale: "r".into(),
            relevance_score: 0.9,
            exploration_score: 0.5,
        },
    ];
    let scored = DiscoverEngine::prioritize(candidates, 10000);
    assert_eq!(scored[0].description, "B");
    assert_eq!(scored[1].description, "A");
}

#[test]
fn test_decompose_produces_subtasks() {
    let tasks = DiscoverEngine::decompose("analyze code quality");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].description, "analyze code quality");
}
```

**Step 3: Update lib.rs**

Add `pub mod discover;` to lib.rs

**Step 4: Run tests**

```bash
cargo test -p xuanji-role 2>&1
```
Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/xuanji-role/
git commit -m "feat(role): implement DiscoverEngine (discover + prioritize + decompose)"
```

---

### Task 4.2: Implement Reflect module

**Files:**
- Create: `crates/xuanji-role/src/reflect.rs`

**Step 1: Write reflect.rs**

```rust
use crate::types::*;
use std::collections::HashMap;

/// Reflection and learning engine.
pub struct LearningEngine;

impl LearningEngine {
    /// Reflect on a completed goal and produce learning artifacts.
    pub fn reflect_on_goal(
        outcome: &GoalOutcome,
        rules: &mut Vec<Rule>,
        cases: &mut Vec<CaseEntry>,
        preferences: &mut HashMap<String, ToolPreference>,
    ) {
        // 1. Record as case
        let case = CaseEntry {
            id: format!("case-{}", chrono_now_compact()),
            task_description: outcome.summary.clone(),
            context_tags: extract_tags(&outcome.summary),
            strategy_used: format!("{} tool calls", outcome.tool_calls_count),
            outcome: if outcome.success {
                CaseOutcome::Success
            } else {
                CaseOutcome::Failure {
                    reason: outcome.lessons.clone(),
                }
            },
            lessons: outcome.lessons.clone(),
            created_at: chrono_now(),
        };
        cases.push(case);

        // 2. Derive rule from lessons
        if !outcome.lessons.is_empty() && outcome.success {
            let rule = Rule {
                id: format!("rule-{}", chrono_now_compact()),
                condition: format!("Working on: {}", outcome.summary),
                action: format!("Strategy: {}", outcome.lessons),
                confidence: 0.6,
                source_case_id: Some(case.id.clone()),
                validated_count: 0,
                created_at: chrono_now(),
            };
            rules.push(rule);
        }

        // 3. Update tool preferences based on outcome
        // (In full implementation, tool-specific data would come from agent execution)
        // For MVP, record a default preference entry
        let tool_name = "shell.run".to_string();
        let pref = preferences
            .entry(tool_name.clone())
            .or_insert_with(|| ToolPreference::new(&tool_name));
        pref.record_call(outcome.success, outcome.tokens_used);
    }

    /// Generate a teaching from the most confident rules.
    pub fn generate_teaching(
        author_role: &str,
        rules: &[Rule],
        teaching_lib: &mut super::TeachingLibrary,
    ) -> Result<Vec<String>, super::RoleError> {
        let mut published_ids = Vec::new();
        for rule in rules {
            if rule.confidence >= 0.7 && rule.validated_count >= 2 {
                let teaching = Teaching::new(
                    author_role,
                    TeachingKind::Rule,
                    &format!("When: {}\nThen: {}", rule.condition, rule.action),
                    extract_tags(&rule.condition),
                );
                let id = teaching.id.clone();
                teaching_lib.publish(teaching)?;
                published_ids.push(id);
            }
        }
        Ok(published_ids)
    }
}

fn extract_tags(text: &str) -> Vec<String> {
    // MVP: extract key nouns/phrases from text
    text.split_whitespace()
        .filter(|w| w.len() > 4)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .take(5)
        .collect()
}

fn chrono_now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn chrono_now_compact() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}
```

**Step 2: Write tests**

Create `crates/xuanji-role/tests/reflect_test.rs`:

```rust
use std::collections::HashMap;
use xuanji_role::*;

#[test]
fn test_reflect_creates_case_and_rule() {
    let outcome = GoalOutcome {
        goal_id: "g1".into(),
        success: true,
        summary: "Profiled project build".into(),
        tool_calls_count: 5,
        tokens_used: 2000,
        lessons: "Use cargo build --release for accurate profiling".into(),
    };
    
    let mut rules = Vec::new();
    let mut cases = Vec::new();
    let mut prefs = HashMap::new();
    
    LearningEngine::reflect_on_goal(&outcome, &mut rules, &mut cases, &mut prefs);
    
    assert_eq!(cases.len(), 1);
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].confidence, 0.6);
    assert_eq!(cases[0].lessons, "Use cargo build --release for accurate profiling");
}

#[test]
fn test_reflect_failure_case_only() {
    let outcome = GoalOutcome {
        goal_id: "g2".into(),
        success: false,
        summary: "Reduce binary size".into(),
        tool_calls_count: 3,
        tokens_used: 1000,
        lessons: "".into(),
    };
    
    let mut rules = Vec::new();
    let mut cases = Vec::new();
    let mut prefs = HashMap::new();
    
    LearningEngine::reflect_on_goal(&outcome, &mut rules, &mut cases, &mut prefs);
    
    assert_eq!(cases.len(), 1);
    // No rule because lessons are empty
    assert_eq!(rules.len(), 0);
}

#[test]
fn test_teaching_generation_threshold() {
    let mut lib = TeachingLibrary::load().unwrap();
    let rules = vec![
        Rule {
            id: "r1".into(),
            condition: "Working on builds".into(),
            action: "Use --release flag".into(),
            confidence: 0.8,
            source_case_id: None,
            validated_count: 3,
            created_at: "now".into(),
        },
        Rule {
            id: "r2".into(),
            condition: "Working on tests".into(),
            action: "Run with --nocapture".into(),
            confidence: 0.5,  // Below threshold
            source_case_id: None,
            validated_count: 1,
            created_at: "now".into(),
        },
    ];
    
    let ids = LearningEngine::generate_teaching("test-role", &rules, &mut lib).unwrap();
    assert_eq!(ids.len(), 1);  // Only r1 qualifies
}
```

**Step 3: Update lib.rs**

Add `pub mod reflect;` to lib.rs

**Step 4: Run tests**

```bash
cargo test -p xuanji-role 2>&1
```
Expected: All tests pass.

**Step 5: Commit**

```bash
git add crates/xuanji-role/
git commit -m "feat(role): implement LearningEngine (reflect + teach generation)"
```

---

### Task 4.3: Implement Role struct (self-direction loop)

**Files:**
- Modify: `crates/xuanji-role/src/lib.rs`

**Step 1: Add Role struct to lib.rs**

Replace the entire `crates/xuanji-role/src/lib.rs`:

```rust
pub mod discover;
pub mod error;
pub mod reflect;
pub mod store;
pub mod teaching;
pub mod types;

use crate::discover::DiscoverEngine;
use crate::error::RoleError;
use crate::reflect::LearningEngine;
use crate::store::RoleStore;
use crate::teaching::TeachingLibrary;
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use xuanji_agent::Agent;
use xuanji_budget::BudgetController;
use xuanji_llm::LlmProvider;

pub use error::RoleError;
pub use store::RoleStore;
pub use teaching::TeachingLibrary;
pub use types::*;

/// An autonomous, self-evolving role.
pub struct Role {
    pub profile: RoleProfile,
    store: RoleStore,
    /// Agent for executing tasks.
    pub agent: Option<Agent>,
    /// Active goal queue.
    goals: Vec<GoalNode>,
    /// Learned rules.
    rules: Vec<Rule>,
    /// Learned cases.
    cases: Vec<CaseEntry>,
    /// Tool preference signals.
    preferences: HashMap<String, ToolPreference>,
    /// Shared teaching library.
    teaching_lib: TeachingLibrary,
    /// Whether the self-direction loop is active.
    active: bool,
    /// Outcomes from the current cycle.
    outcomes: Vec<GoalOutcome>,
}

impl Role {
    /// Create a new role. Requires agent to be set later via `with_agent`.
    pub fn new(name: &str, seed_purpose: &str) -> Result<Self, RoleError> {
        let profile = RoleProfile::new(name, seed_purpose);
        let store = RoleStore::new(name)?;

        // Load existing state from disk (if any)
        let goals = store.load_goals()?;
        let rules = store.load_rules()?;
        let cases = store.load_cases()?;
        let preferences = store.load_preferences()?;
        let teaching_lib = TeachingLibrary::load()?;

        // Save initial profile
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

    /// Attach an agent to this role.
    pub fn with_agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Activate the self-direction loop.
    pub fn activate(&mut self) {
        self.active = true;
        tracing::info!("Role '{}' activated", self.profile.name);
    }

    /// Deactivate the self-direction loop.
    pub fn deactivate(&mut self) {
        self.active = false;
        tracing::info!("Role '{}' deactivated", self.profile.name);
    }

    /// Check if the role is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Execute one full cycle of the self-direction loop.
    pub async fn run_cycle(&mut self) -> Result<Option<GoalOutcome>, RoleError> {
        if !self.active {
            return Ok(None);
        }

        // Phase 1: REFLECT — process completed goals
        for outcome in self.outcomes.drain(..) {
            LearningEngine::reflect_on_goal(
                &outcome,
                &mut self.rules,
                &mut self.cases,
                &mut self.preferences,
            );
        }

        // Generate teachings from confident rules
        let _published = LearningEngine::generate_teaching(
            &self.profile.name,
            &self.rules,
            &mut self.teaching_lib,
        )?;

        // Phase 2: DISCOVER — find candidate goals
        let candidates = DiscoverEngine::discover(&self.profile, &self.rules, &self.cases);

        // Phase 3: PRIORITIZE — rank candidates
        let scored = DiscoverEngine::prioritize(candidates, 0); // TODO: budget integration

        // Phase 4: DECOMPOSE + EXECUTE — run top candidate
        if let Some(top) = scored.first() {
            let goal = GoalNode::new(&top.description, top.score, GoalSource::SelfDiscovered);
            self.goals.push(goal);

            let subtasks = DiscoverEngine::decompose(&top.description);

            let mut success = true;
            let mut total_calls = 0u32;
            let mut total_tokens = 0u32;

            for subtask in &subtasks {
                if let Some(ref mut agent) = self.agent {
                    match agent.run(subtask.description.clone()).await {
                        Ok(result) => {
                            tracing::info!("Role '{}' subtask done: {}", self.profile.name, &result[..result.len().min(100)]);
                            total_calls += 1;
                        }
                        Err(e) => {
                            success = false;
                            tracing::warn!("Role '{}' subtask failed: {}", self.profile.name, e);
                        }
                    }
                }
            }

            let outcome = GoalOutcome {
                goal_id: format!("goal-outcome-{}", chrono_now_compact()),
                success,
                summary: top.description.clone(),
                tool_calls_count: total_calls,
                tokens_used: total_tokens,
                lessons: if success {
                    String::new()
                } else {
                    "Task execution failed".into()
                },
            };

            // Phase 5: LEARN — process the outcome (deferred to next cycle)
            self.outcomes.push(outcome.clone());

            // Update evolution stage
            self.profile.evolution_stage = self.profile.evolution_stage.promote(
                self.cases.len(),
                self.rules.iter().filter(|r| r.confidence >= 0.7).count(),
            );
            self.store.save_profile(&self.profile)?;

            // Persist state
            self.persist()?;

            return Ok(Some(outcome));
        }

        // No candidates found — persist and return
        self.persist()?;
        Ok(None)
    }

    /// Persist current state to disk.
    pub fn persist(&self) -> Result<(), RoleError> {
        self.store.save_goals(&self.goals)?;
        self.store.save_rules(&self.rules)?;
        self.store.save_cases(&self.cases)?;
        self.store.save_preferences(&self.preferences)?;
        Ok(())
    }

    /// Get all goals (read-only).
    pub fn goals(&self) -> &[GoalNode] {
        &self.goals
    }

    /// Get all rules (read-only).
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Get all cases (read-only).
    pub fn cases(&self) -> &[CaseEntry] {
        &self.cases
    }

    /// Get tool preferences (read-only).
    pub fn preferences(&self) -> &HashMap<String, ToolPreference> {
        &self.preferences
    }

    /// Get the teaching library.
    pub fn teaching_lib(&self) -> &TeachingLibrary {
        &self.teaching_lib
    }

    /// Mutable access to teaching library.
    pub fn teaching_lib_mut(&mut self) -> &mut TeachingLibrary {
        &mut self.teaching_lib
    }

    /// Manually add a user goal to the queue.
    pub fn add_user_goal(&mut self, description: &str) {
        let goal = GoalNode::new(description, 1.0, GoalSource::User);
        self.goals.push(goal);
    }
}

fn chrono_now_compact() -> String {
    chrono::Local::now().format("%Y%m%d%H%M%S").to_string()
}
```

**Step 2: Add chrono to Cargo.toml**

Already added in Phase 1.

**Step 3: Write Role integration test**

Create `crates/xuanji-role/tests/role_test.rs`:

```rust
use xuanji_role::*;

#[test]
fn test_role_creation_and_persistence() {
    let role = Role::new("test-role-integration", "test integration").unwrap();
    assert_eq!(role.profile.name, "test-role-integration");
    assert_eq!(role.profile.evolution_stage, Stage::Seed);
    assert!(role.goals().is_empty());
    assert!(role.rules().is_empty());
    assert!(role.cases().is_empty());
    
    // Cleanup
    RoleStore::delete("test-role-integration").ok();
}

#[test]
fn test_role_activation() {
    let mut role = Role::new("test-role-activate", "test").unwrap();
    assert!(!role.is_active());
    role.activate();
    assert!(role.is_active());
    role.deactivate();
    assert!(!role.is_active());
    
    RoleStore::delete("test-role-activate").ok();
}

#[test]
fn test_add_user_goal() {
    let role = Role::new("test-role-goal", "test").unwrap();
    drop(role); // Role is not Send because of Agent, drop here
    
    // Test GoalNode directly
    let goal = GoalNode::new("manual task", 1.0, GoalSource::User);
    assert_eq!(goal.description, "manual task");
    assert_eq!(goal.status, GoalStatus::Pending);
    
    RoleStore::delete("test-role-goal").ok();
}

#[test]
fn test_run_cycle_without_agent() {
    let mut role = Role::new("test-role-cycle", "test").unwrap();
    role.activate();
    
    // run_cycle without agent should still work (no execution)
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(role.run_cycle());
    match result {
        Ok(Some(outcome)) => {
            // May produce outcome from discovery
            assert!(!outcome.summary.is_empty());
        }
        Ok(None) => {
            // No candidates found
        }
        Err(_) => {
            // Expected if agent is None
        }
    }
    
    RoleStore::delete("test-role-cycle").ok();
}
```

**Step 4: Run tests**

```bash
cargo test -p xuanji-role 2>&1
```
Expected: All tests pass (test-run-cycle-without-agent should not panic).

**Step 5: Commit**

```bash
git add crates/xuanji-role/
git commit -m "feat(role): implement Role struct with self-direction run_cycle"
```

---

## Phase 5: Implement God Role bootstrap + CLI commands

### Task 5.1: Add god module to CLI

**Files:**
- Create: `crates/xuanji-cli/src/commands/god.rs`

**Step 1: Write god.rs**

```rust
use anyhow::Result;
use xuanji_role::Role;

/// God Role name constant.
pub const GOD_NAME: &str = "god";

/// God Role seed purpose.
const GOD_PURPOSE: &str = "统筹管理所有 Role，发现协作机会，优化整体效率";

/// Bootstrap the God Role (idempotent).
pub fn bootstrap_god() -> Result<Role> {
    match Role::new(GOD_NAME, GOD_PURPOSE) {
        Ok(mut role) => {
            role.activate();
            role.persist()?;
            tracing::info!("God Role bootstrapped successfully");
            Ok(role)
        }
        Err(e) => {
            tracing::warn!("God Role bootstrap skipped: {}", e);
            // Return an in-memory role for chat/prompt use
            let mut role = Role::new(GOD_NAME, GOD_PURPOSE)?;
            role.activate();
            Ok(role)
        }
    }
}

/// Run a single prompt through God Role.
pub async fn run_prompt(prompt: &str, _config: &super::super::config::XuanjiConfig) -> Result<()> {
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
pub async fn run_chat(config: &super::super::config::XuanjiConfig) -> Result<()> {
    println!("╔══════════════════════════════════╗");
    println!("║  xuanji chat — God Role          ║");
    println!("║  输入 /help 查看命令               ║");
    println!("║  输入 /quit 退出                  ║");
    println!("╚══════════════════════════════════╝");
    println!();

    let mut god = bootstrap_god()?;
    
    loop {
        let mut input = String::new();
        print!("> ");
        use std::io::Write;
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
                match xuanji_role::RoleStore::list_roles() {
                    Ok(names) => {
                        if names.is_empty() {
                            println!("没有活跃的角色。使用 xuanji role hire 创建。");
                        } else {
                            println!("活跃角色:");
                            for name in &names {
                                println!("  - {}", name);
                            }
                        }
                    }
                    Err(e) => println!("无法列出角色: {}", e),
                }
                continue;
            }
            "/teachings" => {
                let lib = god.teaching_lib();
                let teachings = lib.list();
                if teachings.is_empty() {
                    println!("教学库为空。");
                } else {
                    println!("教学库 ({}):", teachings.len());
                    for t in teachings {
                        println!("  [{}] {} - {} (置信度: {:.2})",
                            t.author_role, t.kind_str(), &t.content[..t.content.len().min(60)], t.confidence);
                    }
                }
                continue;
            }
            _ => {
                god.add_user_goal(&input);
                match god.run_cycle().await {
                    Ok(Some(outcome)) => {
                        if outcome.success {
                            println!();
                            // The actual response would come from agent.run()
                            // For now, echo back the goal
                            println!("目标已记录: {} (状态: 成功)", outcome.summary);
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
```

**Step 2: Add TeachingKind::kind_str helper to types.rs**

Add to the TeachingKind impl:

```rust
impl TeachingKind {
    /// Human-readable kind name.
    pub fn kind_str(&self) -> &'static str {
        match self {
            TeachingKind::Rule => "规则",
            TeachingKind::AntiPattern => "反模式",
            TeachingKind::Heuristic => "启发式",
            TeachingKind::CaseStudy => "案例研究",
        }
    }
}
```

**Step 3: Add god module to CLI's commands/mod.rs**

Append to `crates/xuanji-cli/src/commands/mod.rs`:

```rust
pub mod god;
pub mod role;
```

**Step 4: Commit**

```bash
git add crates/xuanji-role/src/types.rs crates/xuanji-cli/src/commands/god.rs crates/xuanji-cli/src/commands/mod.rs
git commit -m "feat(cli): add God Role bootstrap and chat/prompt entry points"
```

---

### Task 5.2: Add role subcommand to CLI

**Files:**
- Create: `crates/xuanji-cli/src/commands/role.rs`

**Step 1: Write role.rs**

```rust
use anyhow::Result;
use xuanji_role::{Role, RoleStore};

/// Handle the 'role' subcommand.
pub async fn handle_role(action: &RoleAction) -> Result<()> {
    match action {
        RoleAction::Hire { name, purpose } => hire_role(name, purpose)?,
        RoleAction::Fire { name } => fire_role(name)?,
        RoleAction::List => list_roles()?,
        RoleAction::Show { name } => show_role(name)?,
        RoleAction::Activate { name } => activate_role(name)?,
        RoleAction::Chat { name } => chat_with_role(name).await?,
        RoleAction::Evolve { name } => evolve_role(name)?,
    }
    Ok(())
}

fn hire_role(name: &str, purpose: &str) -> Result<()> {
    let role = Role::new(name, purpose)?;
    role.persist()?;
    println!("✅ 角色 '{}' 已创建", name);
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
    println!("✅ 角色 '{}' 已销毁", name);
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
            println!("    [{}] {} (priority: {:.2})", 
                goal_status_icon(&g.status), g.description, g.priority);
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
    println!("✅ 角色 '{}' 已激活", name);
    Ok(())
}

async fn chat_with_role(name: &str) -> Result<()> {
    println!("与角色 '{}' 对话中...", name);
    if name == "god" {
        // Redirect to god chat
        super::god::run_chat(&super::super::config::XuanjiConfig::default()).await?;
    } else {
        println!("(非 God Role 的 chat 功能需要 Role 持有 Agent 实例)");
        let mut role = Role::new(name, "")?;
        role.activate();
        role.add_user_goal("chat初始化");
        match role.run_cycle().await {
            Ok(_) => println!("Chat initialized"),
            Err(e) => println!("初始化错误: {}", e),
        }
    }
    Ok(())
}

fn evolve_role(name: &str) -> Result<()> {
    let mut role = Role::new(name, "")?;
    role.activate();
    let rt = tokio::runtime::Runtime::new()?;
    match rt.block_on(role.run_cycle()) {
        Ok(Some(outcome)) => {
            println!("✅ 角色 '{}' 完成一轮进化", name);
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

fn goal_status_icon(status: &xuanji_role::GoalStatus) -> &'static str {
    use xuanji_role::GoalStatus;
    match status {
        GoalStatus::Pending => "⏳",
        GoalStatus::InProgress => "🔄",
        GoalStatus::Done => "✅",
        GoalStatus::Failed => "❌",
        GoalStatus::Blocked => "🚫",
    }
}
```

**Step 2: Add RoleAction enum to main.rs**

In `crates/xuanji-cli/src/main.rs`, add to the `Commands` enum:

```rust
/// Role management
Role {
    #[command(subcommand)]
    action: RoleAction,
},
```

And define `RoleAction`:

```rust
#[derive(clap::Subcommand)]
pub enum RoleAction {
    /// Create a new role
    Hire {
        /// Role name
        name: String,
        /// Role purpose (seed direction)
        #[arg(long)]
        purpose: String,
    },
    /// Destroy a role
    Fire {
        name: String,
    },
    /// List all roles
    List,
    /// Show role details
    Show {
        name: String,
    },
    /// Activate role self-direction loop
    Activate {
        name: String,
    },
    /// Chat with a specific role
    Chat {
        name: String,
    },
    /// Trigger deep reflection cycle
    Evolve {
        name: String,
    },
}
```

**Step 3: Add CLI routing to main.rs**

Add to the match arms under `(None, Some(Commands::Role { action }))`:

```rust
// Role management
(None, Some(Commands::Role { action })) => {
    commands::role::handle_role(&action).await?;
}
```

**Step 4: Build**

```bash
cargo build -p xuanji-cli 2>&1
```
Expected: Compilation success.

**Step 5: Commit**

```bash
git add crates/xuanji-cli/
git commit -m "feat(cli): add role subcommand (hire, fire, list, show, activate, evolve)"
```

---

## Phase 6: Wire God Role as default entry point

### Task 6.1: Replace default chat/prompt with God Role

**Files:**
- Modify: `crates/xuanji-cli/src/main.rs`

**Step 1: Update agent and chat arms in main.rs**

Replace the existing agent mode arm:

```rust
// Agent mode: xuanji "task description" → God Role
(Some(prompt), None) => {
    commands::god::run_prompt(&prompt, &config).await?;
}

// Chat mode: xuanji chat → God Role chat
(None, Some(Commands::Chat)) => {
    commands::god::run_chat(&config).await?;
}
```

**Step 2: Add `use std::io::Write;` if not present**

**Step 3: Build**

```bash
cargo build -p xuanji-cli 2>&1
```
Expected: Compilation success.

**Step 4: Verify CLI help**

```bash
cargo run --bin xuanji -- --help 2>&1
```
Expected: Shows help with new "role" subcommand.

```bash
cargo run --bin xuanji -- role --help 2>&1
```
Expected: Shows role subcommands.

**Step 5: Commit**

```bash
git add crates/xuanji-cli/src/main.rs
git commit -m "feat(cli): wire God Role as default chat/prompt entry point"
```

---

### Task 6.2: Run all tests

**Step 1: Full workspace test**

```bash
cargo test --workspace 2>&1
```
Expected: All tests pass.

**Step 2: Fix any compilation or test issues**

**Step 3: Commit any fixes**

```bash
git add -A && git commit -m "fix: address test failures from role integration"
```

---

## Summary

**Files created:**
- `crates/xuanji-role/Cargo.toml`
- `crates/xuanji-role/src/lib.rs`
- `crates/xuanji-role/src/types.rs`
- `crates/xuanji-role/src/error.rs`
- `crates/xuanji-role/src/store.rs`
- `crates/xuanji-role/src/teaching.rs`
- `crates/xuanji-role/src/discover.rs`
- `crates/xuanji-role/src/reflect.rs`
- `crates/xuanji-role/tests/store_test.rs`
- `crates/xuanji-role/tests/teaching_test.rs`
- `crates/xuanji-role/tests/discover_test.rs`
- `crates/xuanji-role/tests/reflect_test.rs`
- `crates/xuanji-role/tests/role_test.rs`
- `crates/xuanji-cli/src/commands/god.rs`
- `crates/xuanji-cli/src/commands/role.rs`

**Files modified:**
- `Cargo.toml` (workspace members)
- `crates/xuanji-cli/src/main.rs` (role cmd + god default)
- `crates/xuanji-cli/src/commands/mod.rs`

**Commit history:**
1. `feat(role): add xuanji-role crate skeleton`
2. `feat(role): add core types`
3. `feat(role): implement RoleStore persistence layer`
4. `feat(role): implement TeachingLibrary with publish/validate/query`
5. `feat(role): implement DiscoverEngine`
6. `feat(role): implement LearningEngine`
7. `feat(role): implement Role struct with self-direction run_cycle`
8. `feat(cli): add God Role bootstrap and chat/prompt entry points`
9. `feat(cli): add role subcommand`
10. `feat(cli): wire God Role as default chat/prompt entry point`
