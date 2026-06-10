use crate::error::TriggerResult;
use crate::types::TriggerSender;
use async_trait::async_trait;

/// A trigger that watches for events and sends them on a channel.
#[async_trait]
pub trait Trigger: Send + Sync {
    /// Return the trigger type identifier (e.g., "file-watcher", "cron", "webhook").
    fn trigger_type(&self) -> &str;

    /// Start the trigger. Should spawn internal tokio tasks and return quickly.
    async fn start(&self, sender: TriggerSender) -> TriggerResult<()>;

    /// Stop the trigger gracefully.
    async fn stop(&self) -> TriggerResult<()>;
}
