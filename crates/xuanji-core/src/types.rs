use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level workflow definition parsed from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub inputs: HashMap<String, InputDef>,
    pub tasks: HashMap<String, TaskDef>,
}

/// Definition of a workflow input parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDef {
    #[serde(rename = "type", default = "default_input_type")]
    pub input_type: String,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

fn default_input_type() -> String {
    "string".to_string()
}

/// Definition of a single task within a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDef {
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub retry: Option<RetryPolicy>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default)]
    pub when: Option<String>,
}

/// Retry policy for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    #[serde(default = "default_retry_delay")]
    pub delay: String,
}

fn default_retry_delay() -> String {
    "5s".to_string()
}

/// Runtime status of a task in the DAG.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    Running,
    Done,
    Failed,
    Blocked,
    Skipped,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::Running => write!(f, "running"),
            TaskStatus::Done => write!(f, "done"),
            TaskStatus::Failed => write!(f, "failed"),
            TaskStatus::Blocked => write!(f, "blocked"),
            TaskStatus::Skipped => write!(f, "skipped"),
        }
    }
}

/// Runtime result of a completed task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub status: TaskStatus,
    pub output: serde_json::Value,
}

/// Resolved inputs provided at invocation time.
pub type WorkflowInputs = HashMap<String, serde_json::Value>;

/// Final result of a workflow execution.
#[derive(Debug, Clone)]
pub struct WorkflowResult {
    pub task_statuses: HashMap<String, TaskStatus>,
    pub task_results: HashMap<String, TaskResult>,
}

impl WorkflowResult {
    pub fn overall_success(&self) -> bool {
        self.task_statuses
            .values()
            .all(|s| matches!(s, TaskStatus::Done | TaskStatus::Skipped))
    }

    pub fn display_summary(&self) -> String {
        let mut lines = Vec::new();
        lines.push(if self.overall_success() { "✅ SUCCESS".to_string() } else { "❌ FAILED".to_string() });
        lines.push(String::new());

        let mut names: Vec<_> = self.task_statuses.keys().collect();
        names.sort();

        for name in names {
            let status = self.task_statuses.get(name).unwrap();
            let icon = match status {
                TaskStatus::Done => "✅",
                TaskStatus::Failed => "❌",
                TaskStatus::Blocked => "🚫",
                TaskStatus::Skipped => "⏭️",
                TaskStatus::Running => "🔄",
                TaskStatus::Pending => "⏳",
            };
            lines.push(format!("  {} {} — {}", icon, name, status));
        }

        lines.join("\n")
    }
}
