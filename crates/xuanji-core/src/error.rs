use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("YAML parse error: {0}")]
    YamlParse(String),

    #[error("DAG validation error: {0}")]
    DagValidation(String),

    #[error("Cycle detected in workflow DAG")]
    DagCycle,

    #[error("Task '{task}' failed: {reason}")]
    TaskFailed { task: String, reason: String },

    #[error("Template resolution error: {0}")]
    Template(String),

    #[error("Timeout: task '{0}' exceeded its time limit")]
    Timeout(String),

    #[error("User cancelled (confirm required)")]
    UserCancelled,

    #[error("Plugin error: {0}")]
    Plugin(#[from] xuanji_plugin::PluginError),

    #[error("LLM error: {0}")]
    Llm(#[from] xuanji_llm::LlmError),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
