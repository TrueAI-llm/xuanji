use crate::error::{TriggerError, TriggerResult};
use crate::trigger_impl::Trigger;
use crate::types::{TriggerEvent, TriggerSender};
use async_trait::async_trait;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A trigger that watches file system changes.
pub struct FileWatcherTrigger {
    workflow_name: String,
    paths: Vec<String>,
    events: Vec<String>,
    watcher: Arc<Mutex<Option<RecommendedWatcher>>>,
}

impl FileWatcherTrigger {
    /// Create a new FileWatcherTrigger.
    pub fn new(workflow_name: &str, paths: Vec<String>, events: Vec<String>) -> Self {
        Self {
            workflow_name: workflow_name.to_string(),
            paths,
            events,
            watcher: Arc::new(Mutex::new(None)),
        }
    }

    /// Resolve path patterns to directory roots for watching.
    fn resolve_watch_paths(&self) -> Vec<String> {
        let mut watch_paths = Vec::new();
        for pattern in &self.paths {
            let cleaned = pattern
                .trim_end_matches("/**")
                .trim_end_matches('*')
                .trim_end_matches('/');
            let watch_path = if cleaned.is_empty() || cleaned == "." {
                ".".to_string()
            } else {
                cleaned.to_string()
            };
            watch_paths.push(watch_path);
        }
        watch_paths.dedup();
        watch_paths
    }

    /// Check if a file event kind matches our event filter.
    fn event_kind_matches(&self, kind: &notify::EventKind) -> bool {
        let event_str = match kind {
            notify::EventKind::Create(_) => "created",
            notify::EventKind::Modify(_) => "modified",
            notify::EventKind::Remove(_) => "deleted",
            notify::EventKind::Access(_) => return false,
            notify::EventKind::Any => return true,
            notify::EventKind::Other => return true,
        };
        self.events.is_empty() || self.events.iter().any(|e| e == event_str)
    }
}

#[async_trait]
impl Trigger for FileWatcherTrigger {
    fn trigger_type(&self) -> &str {
        "file-watcher"
    }

    async fn start(&self, sender: TriggerSender) -> TriggerResult<()> {
        let workflow_name = self.workflow_name.clone();
        let events_filter = self.events.clone();
        let paths_filter = self.paths.clone();

        let (sync_tx, sync_rx) = std::sync::mpsc::channel::<Event>();
        // Wrap receiver in Arc<Mutex> so spawn_blocking can share it
        let sync_rx = Arc::new(std::sync::Mutex::new(sync_rx));

        // Create the file watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = sync_tx.send(event);
                }
            },
            Config::default().with_poll_interval(std::time::Duration::from_millis(500)),
        )
        .map_err(|e| TriggerError::Watcher(e.to_string()))?;

        // Register watches
        let watch_paths = self.resolve_watch_paths();
        for watch_path in &watch_paths {
            let path = Path::new(watch_path);
            if path.exists() {
                watcher
                    .watch(path, RecursiveMode::Recursive)
                    .map_err(|e| {
                        TriggerError::Watcher(format!("watch '{}' failed: {}", watch_path, e))
                    })?;
                tracing::info!(
                    "FileWatcherTrigger '{}': watching '{}'",
                    workflow_name,
                    watch_path
                );
            } else {
                tracing::warn!(
                    "FileWatcherTrigger '{}': path '{}' does not exist, skipping",
                    workflow_name,
                    watch_path
                );
            }
        }

        // Store watcher
        {
            let mut w = self.watcher.lock().await;
            *w = Some(watcher);
        }

        // Spawn task to bridge sync channel → async channel
        tokio::spawn(async move {
            let mut last_path = String::new();
            let mut last_event = String::new();

            loop {
                // Receive from sync channel via spawn_blocking
                let rx = sync_rx.clone();
                let event = match tokio::task::spawn_blocking(move || {
                    rx.lock().unwrap().recv()
                })
                .await
                {
                    Ok(Ok(e)) => e,
                    _ => break,
                };

                // Check event type
                let event_type = match event.kind {
                    notify::EventKind::Create(_) => "created",
                    notify::EventKind::Modify(_) => "modified",
                    notify::EventKind::Remove(_) => "deleted",
                    _ => continue,
                };

                if !events_filter.is_empty()
                    && !events_filter.iter().any(|e| e == event_type)
                {
                    continue;
                }

                // Get changed file path
                let path_str = event
                    .paths
                    .first()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Basic path filter
                if !paths_filter.is_empty() {
                    let matches = paths_filter.iter().any(|pattern| {
                        if pattern.contains("**") {
                            let prefix = pattern.replace("/**", "").replace("/*", "");
                            path_str.starts_with(&prefix)
                        } else {
                            path_str.ends_with(pattern) || path_str.contains(pattern)
                        }
                    });
                    if !matches {
                        continue;
                    }
                }

                // Debounce
                if path_str == last_path && event_type == last_event {
                    continue;
                }
                last_path = path_str.clone();
                last_event = event_type.to_string();

                let trigger_event = TriggerEvent {
                    trigger_type: "file-watcher".to_string(),
                    workflow_name: workflow_name.clone(),
                    payload: serde_json::json!({
                        "path": path_str,
                        "event": event_type
                    }),
                };

                if sender.send(trigger_event).await.is_err() {
                    break;
                }

                // Debounce delay
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        });

        Ok(())
    }

    async fn stop(&self) -> TriggerResult<()> {
        let mut w = self.watcher.lock().await;
        *w = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_watch_paths() {
        let trigger = FileWatcherTrigger::new(
            "test",
            vec!["src/**".to_string(), "Cargo.toml".to_string()],
            vec!["modified".to_string()],
        );
        let paths = trigger.resolve_watch_paths();
        assert!(paths.contains(&"src".to_string()));
        assert!(paths.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn test_event_kind_matches() {
        let trigger = FileWatcherTrigger::new(
            "test",
            vec![],
            vec!["modified".to_string(), "created".to_string()],
        );
        assert!(trigger.event_kind_matches(&notify::EventKind::Modify(
            notify::event::ModifyKind::Any
        )));
        assert!(trigger.event_kind_matches(&notify::EventKind::Create(
            notify::event::CreateKind::Any
        )));
        assert!(!trigger.event_kind_matches(&notify::EventKind::Remove(
            notify::event::RemoveKind::Any
        )));
    }
}
