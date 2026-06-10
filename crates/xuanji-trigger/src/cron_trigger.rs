use crate::error::{TriggerError, TriggerResult};
use crate::trigger_impl::Trigger;
use crate::types::{TriggerEvent, TriggerSender};
use async_trait::async_trait;
use chrono::Local;
use cron::Schedule;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::task::JoinHandle;

/// A trigger that fires on a cron schedule.
pub struct CronTrigger {
    workflow_name: String,
    schedule: Schedule,
    cancel: Arc<AtomicBool>,
    handle: tokio::sync::Mutex<Option<JoinHandle<()>>>,
}

impl CronTrigger {
    /// Create a new CronTrigger from a cron expression and workflow name.
    /// Accepts standard 5-field cron (min hour dom month dow) — auto-adds
    /// seconds=0 and year=* to produce a 7-field expression for the cron crate.
    pub fn new(workflow_name: &str, schedule_expr: &str) -> TriggerResult<Self> {
        let normalized = normalize_cron_expr(schedule_expr);
        let schedule = Schedule::from_str(&normalized)
            .map_err(|e| TriggerError::CronParse(e.to_string()))?;

        Ok(Self {
            workflow_name: workflow_name.to_string(),
            schedule,
            cancel: Arc::new(AtomicBool::new(false)),
            handle: tokio::sync::Mutex::new(None),
        })
    }
}

/// Normalize a cron expression to 7 fields (sec min hour dom month dow year).
/// - 5 fields → prepend "0 " (seconds=0), append " *" (any year)
/// - 6 fields → prepend "0 " (seconds=0)
/// - 7 fields → use as-is
/// - Other → return as-is (will fail parse, giving a clear error)
fn normalize_cron_expr(expr: &str) -> String {
    let field_count = expr.split_whitespace().count();
    match field_count {
        5 => format!("0 {} *", expr),
        6 => format!("0 {}", expr),
        _ => expr.to_string(),
    }
}

#[async_trait]
impl Trigger for CronTrigger {
    fn trigger_type(&self) -> &str {
        "cron"
    }

    async fn start(&self, sender: TriggerSender) -> TriggerResult<()> {
        self.cancel.store(false, Ordering::SeqCst);

        let workflow_name = self.workflow_name.clone();
        let cancel = self.cancel.clone();
        let schedule = self.schedule.clone();

        let handle = tokio::spawn(async move {
            loop {
                if cancel.load(Ordering::SeqCst) {
                    break;
                }

                // Compute next fire time
                let now = Local::now();
                let next = match schedule.after(&now).next() {
                    Some(next) => next,
                    None => {
                        tracing::error!(
                            "CronTrigger '{}': no next fire time, stopping",
                            workflow_name
                        );
                        break;
                    }
                };

                let delay = (next - now)
                    .to_std()
                    .unwrap_or_else(|_| std::time::Duration::from_millis(500));

                // Sleep until next fire time
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {
                        if cancel.load(Ordering::SeqCst) {
                            break;
                        }
                        let scheduled_time = next.to_rfc3339();
                        let event = TriggerEvent {
                            trigger_type: "cron".to_string(),
                            workflow_name: workflow_name.clone(),
                            payload: serde_json::json!({
                                "scheduled_time": scheduled_time
                            }),
                        };

                        if sender.send(event).await.is_err() {
                            tracing::warn!(
                                "CronTrigger '{}': receiver dropped, stopping",
                                workflow_name
                            );
                            break;
                        }
                    }
                }
            }
        });

        let mut h = self.handle.lock().await;
        *h = Some(handle);

        Ok(())
    }

    async fn stop(&self) -> TriggerResult<()> {
        self.cancel.store(true, Ordering::SeqCst);
        let mut h = self.handle.lock().await;
        if let Some(handle) = h.take() {
            handle.abort();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_parse_valid() {
        assert!(CronTrigger::new("test", "0 9 * * 1-5").is_ok());
        assert!(CronTrigger::new("test", "* * * * *").is_ok());
        assert!(CronTrigger::new("test", "0 */5 * * *").is_ok());
    }

    #[test]
    fn test_cron_parse_invalid() {
        assert!(CronTrigger::new("test", "invalid").is_err());
    }

    #[tokio::test]
    async fn test_cron_trigger_fires() {
        // Every second cron: 7 fields (sec min hour dom month dow year)
        let trigger = CronTrigger::new("test-workflow", "* * * * * * *").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);

        trigger.start(tx).await.unwrap();

        // Wait for first event (should fire within 2 seconds)
        let result = tokio::time::timeout(std::time::Duration::from_secs(3), rx.recv()).await;

        trigger.stop().await.unwrap();

        let event = result.unwrap().unwrap();
        assert_eq!(event.trigger_type, "cron");
        assert_eq!(event.workflow_name, "test-workflow");
        assert!(event.payload.get("scheduled_time").is_some());
    }
}
