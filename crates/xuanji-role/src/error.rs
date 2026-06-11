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

    #[error("TOML parse error: {0}")]
    Toml(String),

    #[error("Agent error: {0}")]
    Agent(String),
}
