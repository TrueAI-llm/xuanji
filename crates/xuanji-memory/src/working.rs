use std::fmt;

/// Status of a sub-task within working memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubTaskStatus {
    Pending,
    InProgress,
    Done,
    Failed,
}

impl fmt::Display for SubTaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubTaskStatus::Pending => write!(f, "Pending"),
            SubTaskStatus::InProgress => write!(f, "InProgress"),
            SubTaskStatus::Done => write!(f, "Done"),
            SubTaskStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// A sub-task tracked in working memory.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub description: String,
    pub status: SubTaskStatus,
    pub result_summary: Option<String>,
}

impl SubTask {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            status: SubTaskStatus::Pending,
            result_summary: None,
        }
    }
}

/// Working memory that tracks the current goal, sub-tasks, key results, and errors.
pub struct WorkingMemory {
    pub goal: Option<String>,
    pub subtasks: Vec<SubTask>,
    pub key_results: Vec<String>,
    pub errors: Vec<String>,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self {
            goal: None,
            subtasks: Vec::new(),
            key_results: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn with_goal(mut self, goal: impl Into<String>) -> Self {
        self.goal = Some(goal.into());
        self
    }

    /// Generate a markdown summary of the current working memory state for inclusion
    /// in an LLM prompt context.
    pub fn to_prompt_context(&self) -> String {
        let mut lines = Vec::new();

        if let Some(goal) = &self.goal {
            lines.push(format!("## Goal\n{goal}"));
        }

        if !self.subtasks.is_empty() {
            lines.push("## Sub-Tasks".to_string());
            for (i, task) in self.subtasks.iter().enumerate() {
                let summary = task
                    .result_summary
                    .as_deref()
                    .map(|s| format!(" — {s}"))
                    .unwrap_or_default();
                lines.push(format!(
                    "{}. [{}] {}{}",
                    i + 1,
                    task.status,
                    task.description,
                    summary,
                ));
            }
        }

        if !self.key_results.is_empty() {
            lines.push("## Key Results".to_string());
            for result in &self.key_results {
                lines.push(format!("- {result}"));
            }
        }

        if !self.errors.is_empty() {
            lines.push("## Errors".to_string());
            for error in &self.errors {
                lines.push(format!("- {error}"));
            }
        }

        lines.join("\n\n")
    }
}

impl Default for WorkingMemory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_prompt_context_empty() {
        let mem = WorkingMemory::new();
        let ctx = mem.to_prompt_context();
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_to_prompt_context_full() {
        let mut mem = WorkingMemory::new();
        mem.goal = Some("Fix the build error".to_string());

        mem.subtasks.push(SubTask {
            description: "Identify the error".to_string(),
            status: SubTaskStatus::Done,
            result_summary: Some("Missing import in main.rs".to_string()),
        });
        mem.subtasks.push(SubTask {
            description: "Apply fix".to_string(),
            status: SubTaskStatus::InProgress,
            result_summary: None,
        });
        mem.key_results.push("Error was a missing `use` statement".to_string());
        mem.errors.push("Initial build failed".to_string());

        let ctx = mem.to_prompt_context();
        assert!(ctx.contains("## Goal"));
        assert!(ctx.contains("Fix the build error"));
        assert!(ctx.contains("## Sub-Tasks"));
        assert!(ctx.contains("[Done]"));
        assert!(ctx.contains("[InProgress]"));
        assert!(ctx.contains("## Key Results"));
        assert!(ctx.contains("## Errors"));
    }
}
