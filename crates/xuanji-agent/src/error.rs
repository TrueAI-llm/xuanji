use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] xuanji_llm::LlmError),

    #[error("Plugin error: {0}")]
    Plugin(#[from] xuanji_plugin::PluginError),

    #[error("Max loops ({0}) exceeded")]
    MaxLoopsExceeded(u32),

    #[error("Step timeout")]
    StepTimeout,

    #[error("User cancelled")]
    UserCancelled,

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
