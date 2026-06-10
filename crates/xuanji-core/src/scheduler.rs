use crate::dag::{build_dag, find_ready_tasks, propagate_failure};
use crate::error::CoreError;
use crate::template::{resolve_templates, TemplateContext};
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;
use xuanji_plugin::ToolRegistry;

/// DAG workflow execution engine.
pub struct DagScheduler {
    registry: Arc<ToolRegistry>,
}

impl DagScheduler {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    /// Execute a workflow with the given inputs.
    pub async fn execute(
        &self,
        workflow: &WorkflowDef,
        inputs: &WorkflowInputs,
    ) -> Result<WorkflowResult, CoreError> {
        // 1. Build and validate DAG
        let (graph, name_to_idx) = build_dag(workflow)?;

        // 2. Initialize all task statuses
        let mut statuses: HashMap<String, TaskStatus> = workflow
            .tasks
            .keys()
            .map(|name| (name.clone(), TaskStatus::Pending))
            .collect();
        let mut results: HashMap<String, TaskResult> = HashMap::new();

        // 3. Build initial template context
        let mut ctx = TemplateContext {
            inputs: inputs.clone(),
            tasks: HashMap::new(),
            env: std::env::vars().collect(),
        };

        // 4. Main scheduling loop
        loop {
            // Find ready tasks
            let ready = find_ready_tasks(&graph, &name_to_idx, &statuses, workflow);

            if ready.is_empty() {
                // No more tasks to run
                break;
            }

            // Evaluate when conditions and mark Skipped
            for name in &ready {
                let task_def = &workflow.tasks[name];
                if let Some(ref when_expr) = task_def.when {
                    match evaluate_when(when_expr, &ctx) {
                        Ok(false) => {
                            statuses.insert(name.clone(), TaskStatus::Skipped);
                            results.insert(
                                name.clone(),
                                TaskResult {
                                    status: TaskStatus::Skipped,
                                    output: serde_json::Value::String("skipped".into()),
                                },
                            );
                        }
                        Ok(true) => {}
                        Err(e) => {
                            tracing::warn!("when evaluation error for '{}': {}", name, e);
                            statuses.insert(name.clone(), TaskStatus::Failed);
                            results.insert(
                                name.clone(),
                                TaskResult {
                                    status: TaskStatus::Failed,
                                    output: serde_json::Value::String(format!("when error: {}", e)),
                                },
                            );
                        }
                    }
                }
            }

            // Filter to actually runnable tasks
            let runnable: Vec<String> = ready
                .into_iter()
                .filter(|name| matches!(statuses.get(name), Some(TaskStatus::Pending)))
                .collect();

            if runnable.is_empty() {
                continue;
            }

            // Execute all runnable tasks in parallel
            let mut join_set = JoinSet::new();

            for name in runnable {
                let task_def = workflow.tasks[&name].clone();
                let registry = self.registry.clone();
                let ctx_clone = ctx.clone();

                statuses.insert(name.clone(), TaskStatus::Running);

                join_set.spawn(async move {
                    let result = execute_single_task(&name, &task_def, &ctx_clone, &registry).await;
                    (name, result)
                });
            }

            // Collect results as they complete
            while let Some(join_result) = join_set.join_next().await {
                match join_result {
                    Ok((name, Ok(task_result))) => {
                        tracing::info!("Task '{}' completed: {}", name, task_result.status);
                        statuses.insert(name.clone(), task_result.status.clone());
                        ctx.tasks.insert(name.clone(), task_result.clone());
                        results.insert(name, task_result);
                    }
                    Ok((name, Err(e))) => {
                        tracing::error!("Task '{}' failed: {}", name, e);
                        statuses.insert(name.clone(), TaskStatus::Failed);
                        results.insert(
                            name.clone(),
                            TaskResult {
                                status: TaskStatus::Failed,
                                output: serde_json::Value::String(format!("{}", e)),
                            },
                        );
                        propagate_failure(&name, &graph, &name_to_idx, &mut statuses);
                    }
                    Err(e) => {
                        tracing::error!("Task join error: {}", e);
                    }
                }
            }
        }

        // 5. Build final result
        Ok(WorkflowResult {
            task_statuses: statuses,
            task_results: results,
        })
    }
}

/// Execute a single task with template resolution, timeout, and retry.
async fn execute_single_task(
    name: &str,
    task_def: &TaskDef,
    ctx: &TemplateContext,
    registry: &Arc<ToolRegistry>,
) -> Result<TaskResult, CoreError> {
    // 1. Resolve template variables in arguments
    let resolved_args = resolve_templates(&task_def.arguments, ctx)?;

    // 2. Confirm if required
    if task_def.confirm {
        println!("⚠ Task '{}' requires confirmation. Execute? [y/N] ", name);
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| CoreError::Other(anyhow::anyhow!("stdin read error: {}", e)))?;
        if !input.trim().eq_ignore_ascii_case("y") {
            return Err(CoreError::UserCancelled);
        }
    }

    // 3. Parse timeout
    let timeout = task_def.timeout
        .as_deref()
        .map(parse_duration)
        .transpose()?
        .unwrap_or(Duration::from_secs(300));

    // 4. Parse retry policy
    let retry = task_def.retry.as_ref();
    let max_attempts = retry.map(|r| r.max_attempts).unwrap_or(1);
    let retry_delay = retry
        .map(|r| parse_duration(&r.delay))
        .transpose()?
        .unwrap_or(Duration::from_secs(5));

    // 5. Execute with retry
    let mut last_error = None;
    for attempt in 0..max_attempts {
        if attempt > 0 {
            tracing::info!("Retrying task '{}' (attempt {}/{})", name, attempt + 1, max_attempts);
            tokio::time::sleep(retry_delay).await;
        }

        match tokio::time::timeout(
            timeout,
            registry.call_tool(&task_def.tool, resolved_args.clone()),
        )
        .await
        {
            Ok(Ok(mcp_result)) => {
                let text = mcp_result
                    .content
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(String::from))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_else(|| mcp_result.content.to_string());

                return Ok(TaskResult {
                    status: TaskStatus::Done,
                    output: serde_json::Value::String(text),
                });
            }
            Ok(Err(e)) => {
                last_error = Some(CoreError::Plugin(e));
            }
            Err(_) => {
                last_error = Some(CoreError::Timeout(name.to_string()));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| CoreError::TaskFailed {
        task: name.to_string(),
        reason: "unknown error".into(),
    }))
}

/// Parse a duration string like "30s", "5m", "1h".
fn parse_duration(s: &str) -> Result<Duration, CoreError> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix('s') {
        num.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| CoreError::Template(format!("invalid duration: {}", s)))
    } else if let Some(num) = s.strip_suffix('m') {
        num.parse::<u64>()
            .map(|n| Duration::from_secs(n * 60))
            .map_err(|_| CoreError::Template(format!("invalid duration: {}", s)))
    } else if let Some(num) = s.strip_suffix('h') {
        num.parse::<u64>()
            .map(|n| Duration::from_secs(n * 3600))
            .map_err(|_| CoreError::Template(format!("invalid duration: {}", s)))
    } else {
        s.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|_| CoreError::Template(format!("invalid duration: {}", s)))
    }
}

fn evaluate_when(expr: &str, ctx: &TemplateContext) -> Result<bool, CoreError> {
    crate::template::evaluate_when(expr, ctx)
}
