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
            self_description: format!("我是 {}。我的目标: {}", name, seed_purpose),
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
        // Weighted moving average
        let prev_total = self.total_calls.saturating_sub(1) as u32;
        self.avg_token_cost = if self.total_calls > 1 {
            (self.avg_token_cost * prev_total + tokens) / self.total_calls
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
    /// Required skill tag for role matching (None = God Role handles it).
    pub required_skill: Option<String>,
    /// Role assigned to execute this subtask.
    pub assignee: Option<String>,
    pub result: Option<String>,
}

/// Free-form role context notes (replaces the role-scoped use of LongTermMemory's
/// `ProjectContext`). A role's "current focus" + accumulated notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleContext {
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub focus: String,
}

impl Default for RoleContext {
    fn default() -> Self {
        Self {
            notes: String::new(),
            focus: String::new(),
        }
    }
}

/// An orchestration match decision for one sub-task: assign to an existing role,
/// or signal that a new role should be hired for `purpose`.
#[derive(Debug, Clone)]
pub struct Assignment {
    pub description: String,
    pub assignee: Option<String>,
    pub hire: Option<String>,
}

impl SubTask {
    pub fn new(description: &str) -> Self {
        Self {
            description: description.to_string(),
            depends_on: Vec::new(),
            required_skill: None,
            assignee: None,
            result: None,
        }
    }
}

/// A suggestion from God Role regarding role lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSuggestion {
    pub kind: SuggestionKind,
    pub role_name: String,
    pub purpose: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    HireRole,
    FireRole,
    RedefinePurpose,
}

/// Result of running a cycle, including orchestration suggestions.
#[derive(Debug, Clone)]
pub struct CycleResult {
    pub outcome: Option<GoalOutcome>,
    /// The final aggregated answer to show the user (None if nothing was produced).
    pub answer: Option<String>,
    pub suggestions: Vec<OrchestrationSuggestion>,
    pub dispatched_to: Vec<String>,
}
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeachingKind {
    Rule,
    AntiPattern,
    Heuristic,
    CaseStudy,
}

impl TeachingKind {
    /// Human-readable kind name.
    pub fn kind_str(&self) -> &'static str {
        match self {
            TeachingKind::Rule => "\u{89c4}\u{5219}",
            TeachingKind::AntiPattern => "\u{53cd}\u{6a21}\u{5f0f}",
            TeachingKind::Heuristic => "\u{542f}\u{53d1}\u{5f0f}",
            TeachingKind::CaseStudy => "\u{6848}\u{4f8b}\u{7814}\u{7a76}",
        }
    }
}

// ─── Helpers ───

pub(crate) fn chrono_now() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub(crate) fn chrono_now_compact() -> String {
    chrono::Local::now()
        .format("%Y%m%d%H%M%S")
        .to_string()
}
