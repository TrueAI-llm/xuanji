use thiserror::Error;

#[derive(Debug, Error)]
pub enum TriggerError {
    #[error("file watcher error: {0}")]
    Watcher(String),

    #[error("cron parse error: {0}")]
    CronParse(String),

    #[error("webhook server error: {0}")]
    Webhook(String),

    #[error("daemon error: {0}")]
    Daemon(String),

    #[error("workflow discovery error: {0}")]
    Discovery(String),

    #[error("PID file error: {0}")]
    PidFile(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type TriggerResult<T> = Result<T, TriggerError>;
