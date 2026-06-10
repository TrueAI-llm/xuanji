use crate::cron_trigger::CronTrigger;
use crate::error::{TriggerError, TriggerResult};
use crate::trigger_impl::Trigger;
use crate::types::{TriggerConfig, TriggerEvent};
use crate::watcher::FileWatcherTrigger;
use crate::webhook::build_webhook_router;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use xuanji_core::{parse_workflow, DagScheduler, WorkflowDef, WorkflowInputs};
use xuanji_llm::{LlmProvider, ProviderConfig, Protocol};
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::{McpClient, ToolRegistry};

/// Orchestrates triggers and executes workflows when they fire.
pub struct DaemonRunner {
    trigger_config: TriggerConfig,
    provider_config: ProviderConfig,
    mcp_servers: Vec<McpServerConfig>,
    workflow_cache: HashMap<String, WorkflowDef>,
}

impl DaemonRunner {
    pub fn new(
        trigger_config: TriggerConfig,
        provider_config: ProviderConfig,
        mcp_servers: Vec<McpServerConfig>,
    ) -> Self {
        Self {
            trigger_config,
            provider_config,
            mcp_servers,
            workflow_cache: HashMap::new(),
        }
    }

    /// Scan the workflows directory for YAML files and parse them.
    /// Returns the list of workflow names that have triggers.
    pub fn discover(&mut self) -> TriggerResult<Vec<String>> {
        let dir = PathBuf::from(&self.trigger_config.workflows_dir);
        if !dir.exists() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| TriggerError::Discovery(format!("cannot create {:?}: {}", dir, e)))?;
            tracing::info!("Created workflows directory: {:?}", dir);
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&dir)
            .map_err(|e| TriggerError::Discovery(format!("cannot read {:?}: {}", dir, e)))?;

        let mut triggered = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ext != "yaml" && ext != "yml" {
                continue;
            }

            let yaml_str = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("Cannot read {:?}: {}", path, e);
                    continue;
                }
            };

            let workflow = match parse_workflow(&yaml_str) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Cannot parse {:?}: {}", path, e);
                    continue;
                }
            };

            let name = workflow.name.clone();
            let has_triggers = !workflow.triggers.is_empty();
            self.workflow_cache.insert(name.clone(), workflow);

            if has_triggers {
                triggered.push(name.clone());
                tracing::info!("Discovered workflow '{}' with triggers", name);
            } else {
                tracing::debug!("Workflow '{}' has no triggers, skipping", name);
            }
        }

        Ok(triggered)
    }

    /// Start all triggers and the event loop. Blocks until shutdown.
    pub async fn run(mut self) -> TriggerResult<()> {
        let triggered = self.discover()?;
        if triggered.is_empty() {
            tracing::warn!("No workflows with triggers found in '{}'", self.trigger_config.workflows_dir);
            return Ok(());
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<TriggerEvent>(256);

        // Create triggers for each workflow
        let mut triggers: Vec<Box<dyn Trigger>> = Vec::new();
        let mut webhook_routes: Vec<(String, String, String)> = Vec::new();

        for name in &triggered {
            let workflow = self.workflow_cache.get(name).unwrap();

            for trigger_def in &workflow.triggers {
                match trigger_def.trigger_type.as_str() {
                    "file-watcher" => {
                        let paths = trigger_def
                            .config
                            .get("paths")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();

                        let events = trigger_def
                            .config
                            .get("events")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_else(|| vec!["modified".to_string()]);

                        let trigger = FileWatcherTrigger::new(name, paths, events);
                        triggers.push(Box::new(trigger));
                    }
                    "cron" => {
                        let schedule = trigger_def
                            .config
                            .get("schedule")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        match CronTrigger::new(name, schedule) {
                            Ok(trigger) => triggers.push(Box::new(trigger)),
                            Err(e) => {
                                tracing::error!("Invalid cron for '{}': {}", name, e);
                            }
                        }
                    }
                    "webhook" => {
                        let path = trigger_def
                            .config
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("/");
                        let method = trigger_def
                            .config
                            .get("method")
                            .and_then(|v| v.as_str())
                            .unwrap_or("POST");

                        webhook_routes.push((name.clone(), path.to_string(), method.to_string()));
                    }
                    other => {
                        tracing::warn!("Unknown trigger type '{}' for '{}'", other, name);
                    }
                }
            }
        }

        // Start all non-webhook triggers
        for trigger in &triggers {
            if let Err(e) = trigger.start(tx.clone()).await {
                tracing::error!("Failed to start trigger '{}': {}", trigger.trigger_type(), e);
            }
        }

        // Start webhook HTTP server if any webhook routes exist
        let webhook_handle = if !webhook_routes.is_empty() {
            let router = build_webhook_router(&webhook_routes, tx.clone());
            let port = self.trigger_config.webhook_port;
            let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
                .await
                .map_err(|e| TriggerError::Webhook(format!("bind port {} failed: {}", port, e)))?;

            tracing::info!("Webhook server listening on port {}", port);
            for (name, path, method) in &webhook_routes {
                tracing::info!("  Webhook: {} {} → '{}'", method, path, name);
            }

            Some(tokio::spawn(async move {
                axum::serve(listener, router).await
            }))
        } else {
            None
        };

        let total = triggers.len() + webhook_routes.len();
        tracing::info!("Daemon running with {} triggers ({} watcher/cron, {} webhook)", total, triggers.len(), webhook_routes.len());

        // Event loop
        loop {
            tokio::select! {
                Some(event) = rx.recv() => {
                    tracing::info!(
                        "Trigger fired: {} → '{}'",
                        event.trigger_type,
                        event.workflow_name
                    );
                    if let Err(e) = self.handle_event(&event).await {
                        tracing::error!(
                            "Workflow '{}' execution failed: {}",
                            event.workflow_name,
                            e
                        );
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received Ctrl+C, shutting down...");
                    break;
                }
            }
        }

        // Graceful shutdown
        for trigger in &triggers {
            if let Err(e) = trigger.stop().await {
                tracing::warn!("Error stopping trigger: {}", e);
            }
        }
        if let Some(handle) = webhook_handle {
            handle.abort();
        }

        tracing::info!("Daemon stopped.");
        Ok(())
    }

    /// Handle a trigger event by executing the associated workflow.
    async fn handle_event(&self, event: &TriggerEvent) -> TriggerResult<()> {
        let workflow = self
            .workflow_cache
            .get(&event.workflow_name)
            .ok_or_else(|| {
                TriggerError::Daemon(format!(
                    "workflow '{}' not found in cache",
                    event.workflow_name
                ))
            })?;

        // Build default inputs
        let inputs: WorkflowInputs = workflow
            .inputs
            .iter()
            .filter_map(|(name, def)| {
                def.default
                    .as_ref()
                    .map(|v| (name.clone(), v.clone()))
            })
            .collect();

        // Create provider
        let provider = create_provider(&self.provider_config)?;
        let provider = Arc::from(provider);

        // Create registry and start MCP servers
        let mut registry = ToolRegistry::new();
        for server_config in &self.mcp_servers {
            let process = McpProcess::new(server_config.clone());
            let mut client = McpClient::new(process);
            match client.initialize().await {
                Ok(()) => {
                    if let Err(e) = registry.register_server(client).await {
                        tracing::warn!("Failed to register MCP server '{}': {}", server_config.name, e);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to start MCP server '{}': {}. Skipping.", server_config.name, e);
                }
            }
        }

        // Register system tools
        xuanji_core::register_system_tools(&mut registry, provider);

        let registry = Arc::new(registry);
        let scheduler = DagScheduler::new_daemon(registry.clone());

        let result = scheduler
            .execute_with_trigger(workflow, &inputs, Some(event.payload.clone()))
            .await
            .map_err(|e| TriggerError::Daemon(e.to_string()))?;

        tracing::info!("{}", result.display_summary());

        registry
            .shutdown_all()
            .await
            .map_err(|e| TriggerError::Other(e.into()))?;

        Ok(())
    }
}

/// Create an LLM provider from config.
fn create_provider(config: &ProviderConfig) -> TriggerResult<Arc<dyn LlmProvider>> {
    let mut config = config.clone();
    if config.api_key.starts_with("${") && config.api_key.ends_with('}') {
        let var_name = &config.api_key[2..config.api_key.len() - 1];
        config.api_key = std::env::var(var_name).unwrap_or_default();
    }

    match config.protocol {
        Protocol::OpenAI => Ok(Arc::new(xuanji_llm::openai::OpenAIProvider::new(config))),
        Protocol::Anthropic => Ok(Arc::new(xuanji_llm::anthropic::AnthropicProvider::new(config))),
        Protocol::Gemini => Err(TriggerError::Other(anyhow::anyhow!(
            "Gemini protocol not yet implemented"
        ))),
    }
}
