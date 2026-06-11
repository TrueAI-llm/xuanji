use std::sync::Arc;
use async_trait::async_trait;
use xuanji_agent::types::AgentConfig;
use xuanji_agent::Agent;
use xuanji_budget::BudgetController;
use xuanji_bus::state::SharedState;
use xuanji_bus::KnowledgeBus;
use xuanji_llm::config::ProviderConfig;
use xuanji_llm::error::LlmError;
use xuanji_llm::types::{LlmResponse, Message, ToolSchema};
use xuanji_llm::LlmProvider;
use xuanji_plugin::ToolRegistry;

use crate::parser::parse_workflow;

/// Wrapper to use `Arc<dyn LlmProvider>` as `Box<dyn LlmProvider>`.
struct ArcProvider(Arc<dyn LlmProvider>);

#[async_trait]
impl LlmProvider for ArcProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<LlmResponse, LlmError> {
        self.0.complete(messages, tools).await
    }

    fn config(&self) -> &ProviderConfig {
        self.0.config()
    }
}

/// Schema for llm.ask tool
pub const LLM_ASK_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "prompt": { "type": "string", "description": "The prompt to send to the LLM" },
        "provider": { "type": "string", "description": "Provider name override (optional)" },
        "model": { "type": "string", "description": "Model name override (optional)" },
        "temperature": { "type": "number", "description": "Temperature override (optional)" },
        "max_tokens": { "type": "integer", "description": "Max tokens override (optional)" }
    },
    "required": ["prompt"]
}"#;

/// Schema for shell.run tool (built-in)
pub const SHELL_RUN_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "command": { "type": "string", "description": "The shell command to execute" }
    },
    "required": ["command"]
}"#;

/// Schema for agent.delegate tool
pub const AGENT_DELEGATE_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "task": { "type": "string", "description": "Description of the sub-task to delegate to a sub-agent" },
        "agent_name": { "type": "string", "description": "Name for the sub-agent (default: auto-generated)" }
    },
    "required": ["task"]
}"#;

/// Register system tools (llm.ask, shell.run) with the tool registry.
pub fn register_system_tools(
    registry: &mut ToolRegistry,
    provider: Arc<dyn LlmProvider>,
) {
    let provider_clone = provider.clone();

    registry.register_system_tool(
        "llm.ask",
        "Ask an LLM a question and get a response. Useful for analysis, summarization, and decision-making within workflows.",
        serde_json::from_str(LLM_ASK_SCHEMA).unwrap_or_default(),
        move |args: serde_json::Value| {
            let provider = provider_clone.clone();
            Box::pin(async move {
                execute_llm_ask(args, &*provider).await
            })
        },
    );

    // Built-in shell.run — always available without external MCP server
    register_shell_run(registry);
}

/// Register only the built-in shell.run tool (no provider needed).
/// Use this in agent/chat mode where llm.ask isn't needed.
pub fn register_shell_run(registry: &mut ToolRegistry) {
    registry.register_system_tool(
        "shell.run",
        "Execute a shell command and return its output. Use this for running CLI commands, scripts, and any system operations.",
        serde_json::from_str(SHELL_RUN_SCHEMA).unwrap_or_default(),
        |args: serde_json::Value| {
            Box::pin(async move {
                execute_shell_run(args).await
            })
        },
    );
}

/// Register agent.delegate system tool.
///
/// This allows an agent to spawn a sub-agent to handle a sub-task.
/// The sub-agent shares the same provider, bus, budget, and shared state.
pub fn register_agent_delegate(
    registry: &mut ToolRegistry,
    provider: Arc<dyn LlmProvider>,
    agent_config: AgentConfig,
    bus: KnowledgeBus,
    budget: Arc<BudgetController>,
    shared_state: Arc<SharedState>,
    parent_depth: u32,
    parent_name: String,
) {
    registry.register_system_tool(
        "agent.delegate",
        "Delegate a sub-task to a sub-agent. The sub-agent will execute the task autonomously and return the result.",
        serde_json::from_str(AGENT_DELEGATE_SCHEMA).unwrap_or_default(),
        move |args: serde_json::Value| {
            let provider = provider.clone();
            let agent_config = agent_config.clone();
            let bus = bus.clone();
            let budget = budget.clone();
            let shared_state = shared_state.clone();
            let parent_depth = parent_depth;
            let parent_name = parent_name.clone();

            Box::pin(async move {
                execute_agent_delegate(
                    args,
                    provider,
                    agent_config,
                    bus,
                    budget,
                    shared_state,
                    parent_depth,
                    &parent_name,
                ).await
            })
        },
    );
}

async fn execute_llm_ask(
    arguments: serde_json::Value,
    provider: &dyn LlmProvider,
) -> Result<xuanji_plugin::client::McpToolResult, xuanji_plugin::PluginError> {
    let prompt = arguments
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or_else(|| xuanji_plugin::PluginError::Protocol("llm.ask: missing 'prompt' field".into()))?;

    let messages = vec![Message::User {
        content: prompt.to_string(),
    }];

    let response = provider
        .complete(&messages, &[])
        .await
        .map_err(|e| xuanji_plugin::PluginError::Protocol(format!("LLM error: {}", e)))?;

    let text = response.text_content().unwrap_or("").to_string();

    Ok(xuanji_plugin::client::McpToolResult {
        content: serde_json::json!([{ "type": "text", "text": text }]),
        is_error: false,
    })
}

/// Built-in shell.run — executes a command via `sh -c`.
async fn execute_shell_run(
    arguments: serde_json::Value,
) -> Result<xuanji_plugin::client::McpToolResult, xuanji_plugin::PluginError> {
    let command = arguments
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| xuanji_plugin::PluginError::Protocol(
            "shell.run: missing 'command' field".into(),
        ))?;

    tracing::info!("shell.run: {}", command);

    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await
        .map_err(|e| xuanji_plugin::PluginError::Protocol(
            format!("shell.run: failed to execute: {}", e),
        ))?;

    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
    let is_error = !output.status.success();

    let mut content = vec![serde_json::json!({
        "type": "text",
        "text": if stdout_str.is_empty() { stderr_str.clone() } else { stdout_str },
    })];

    if !stderr_str.is_empty() && is_error {
        content.push(serde_json::json!({
            "type": "text",
            "text": format!("stderr: {}", stderr_str),
        }));
    }

    Ok(xuanji_plugin::client::McpToolResult {
        content: serde_json::json!(content),
        is_error,
    })
}

async fn execute_agent_delegate(
    arguments: serde_json::Value,
    provider: Arc<dyn LlmProvider>,
    agent_config: AgentConfig,
    bus: KnowledgeBus,
    budget: Arc<BudgetController>,
    shared_state: Arc<SharedState>,
    parent_depth: u32,
    parent_name: &str,
) -> Result<xuanji_plugin::client::McpToolResult, xuanji_plugin::PluginError> {
    let task = arguments
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| xuanji_plugin::PluginError::Protocol("agent.delegate: missing 'task' field".into()))?;

    let child_name = arguments
        .get("agent_name")
        .and_then(|v| v.as_str())
        .unwrap_or("worker");

    let child_depth = parent_depth + 1;

    // Check depth limit
    if child_depth > budget.config().max_depth {
        return Ok(xuanji_plugin::client::McpToolResult {
            content: serde_json::json!([{ "type": "text", "text": format!("无法委派：递归深度 {} 超过最大限制 {}", child_depth, budget.config().max_depth) }]),
            is_error: true,
        });
    }

    let full_name = format!("{}-{}", parent_name, child_name);
    tracing::info!("Spawning sub-agent '{}' at depth {} for task: {}", full_name, child_depth, task);

    // Create a new tool registry for the sub-agent (empty — sub-agents don't get MCP tools by default)
    let sub_registry = ToolRegistry::new();

    // Create the sub-agent
    // Wrap Arc in a newtype that implements LlmProvider via Arc delegation
    let provider_box: Box<dyn LlmProvider> = Box::new(ArcProvider(provider));
    let mut sub_agent = Agent::new(
        provider_box,
        sub_registry,
        agent_config,
    )
        .with_name(&full_name)
        .with_bus(bus)
        .with_budget(budget)
        .with_shared_state(shared_state)
        .with_depth(child_depth);

    match sub_agent.run(task.to_string()).await {
        Ok(result) => {
            tracing::info!("Sub-agent '{}' completed successfully", full_name);
            Ok(xuanji_plugin::client::McpToolResult {
                content: serde_json::json!([{ "type": "text", "text": result }]),
                is_error: false,
            })
        }
        Err(e) => {
            tracing::warn!("Sub-agent '{}' failed: {}", full_name, e);
            Ok(xuanji_plugin::client::McpToolResult {
                content: serde_json::json!([{ "type": "text", "text": format!("子 Agent '{}' 执行失败: {}", full_name, e) }]),
                is_error: true,
            })
        }
    }
}

// ─── workflow.create system tool ───

/// Schema for workflow.create tool
pub const WORKFLOW_CREATE_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "yaml": { "type": "string", "description": "Complete workflow YAML content. Must follow the xuanji workflow schema." },
        "name": { "type": "string", "description": "Workflow filename without extension. Defaults to the 'name' field in YAML." }
    },
    "required": ["yaml"]
}"#;

/// Description for workflow.create — includes YAML format docs so the LLM knows the schema.
const WORKFLOW_CREATE_DESC: &str = indoc::indoc! {"
    Create a new workflow YAML file that can be executed by the xuanji daemon.

    Use this tool when the user wants to set up:
    - Scheduled/cron tasks (e.g. '每天早上9点执行报告')
    - File watchers (e.g. '代码变更时自动测试')
    - Webhook triggers (e.g. '接收GitHub webhook自动部署')

    The 'yaml' parameter must be valid xuanji workflow YAML with this structure:

    ```yaml
    name: my-workflow           # Required: workflow name (used as filename)
    description: \"...\"          # Optional: description
    inputs:                     # Optional: input parameters
      param1:
        type: string
        default: \"value\"
    triggers:                   # Optional: automation triggers
      - type: cron              # Scheduled execution
        schedule: \"0 9 * * *\"   # 5-field cron expression (min hour day month weekday)
      - type: file-watcher      # File change detection
        paths: [\"src/\"]
        events: [\"modified\"]
      - type: webhook           # HTTP trigger
        path: \"/deploy\"
        method: \"POST\"
    tasks:                      # Required: at least one task
      task-name:
        tool: llm.ask           # Tool to call (llm.ask, shell.run, agent.delegate, etc.)
        arguments:
          prompt: \"Task description\"
        depends_on: []          # Optional: list of task names this depends on
        timeout: \"60s\"          # Optional
        retry:                  # Optional
          max_attempts: 3
          delay: \"5s\"
    ```

    Template variables available in arguments: ${{ inputs.X }}, ${{ tasks.X.output }}, ${{ env.X }}, ${{ trigger.X }}
    After creation, the daemon must be restarted to pick up new workflows.
"};

/// Register workflow.create system tool.
///
/// Allows agents to create workflow YAML files in the configured workflows directory.
pub fn register_workflow_create(
    registry: &mut ToolRegistry,
    workflows_dir: String,
) {
    registry.register_system_tool(
        "workflow.create",
        WORKFLOW_CREATE_DESC,
        serde_json::from_str(WORKFLOW_CREATE_SCHEMA).unwrap_or_default(),
        move |args: serde_json::Value| {
            let workflows_dir = workflows_dir.clone();
            Box::pin(async move {
                execute_workflow_create(args, &workflows_dir).await
            })
        },
    );
}

async fn execute_workflow_create(
    arguments: serde_json::Value,
    workflows_dir: &str,
) -> Result<xuanji_plugin::client::McpToolResult, xuanji_plugin::PluginError> {
    let yaml_str = arguments
        .get("yaml")
        .and_then(|v| v.as_str())
        .ok_or_else(|| xuanji_plugin::PluginError::Protocol(
            "workflow.create: missing 'yaml' field".into(),
        ))?;

    // Validate the YAML by parsing it
    let workflow = parse_workflow(yaml_str)
        .map_err(|e| xuanji_plugin::PluginError::Protocol(
            format!("workflow.create: invalid workflow YAML: {}", e),
        ))?;

    // Determine filename
    let name = arguments
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| workflow.name.clone());

    // Ensure workflows directory exists
    std::fs::create_dir_all(workflows_dir)
        .map_err(|e| xuanji_plugin::PluginError::Protocol(
            format!("workflow.create: cannot create dir '{}': {}", workflows_dir, e),
        ))?;

    // Write the YAML file
    let file_path = std::path::Path::new(workflows_dir).join(format!("{}.yaml", name));
    std::fs::write(&file_path, yaml_str)
        .map_err(|e| xuanji_plugin::PluginError::Protocol(
            format!("workflow.create: cannot write '{}': {}", file_path.display(), e),
        ))?;

    tracing::info!("Created workflow '{}' at {}", name, file_path.display());

    Ok(xuanji_plugin::client::McpToolResult {
        content: serde_json::json!([{
            "type": "text",
            "text": format!(
                "✅ 工作流 '{}' 已创建: {}\n如需自动执行，请运行 `xuanji daemon start`（或重启 daemon）。",
                name,
                file_path.display()
            )
        }]),
        is_error: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_workflow_create_valid_yaml() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let mut registry = ToolRegistry::new();
        register_workflow_create(&mut registry, dir_path.clone());

        let yaml = r#"name: test-workflow
tasks:
  greet:
    tool: llm.ask
    arguments:
      prompt: "Hello"
"#;
        let args = serde_json::json!({ "yaml": yaml });
        let result = registry.call_tool("workflow.create", args).await.unwrap();
        assert!(!result.is_error);

        // Verify file was created
        let content = std::fs::read_to_string(dir.path().join("test-workflow.yaml")).unwrap();
        assert!(content.contains("test-workflow"));
    }

    #[tokio::test]
    async fn test_workflow_create_invalid_yaml() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let mut registry = ToolRegistry::new();
        register_workflow_create(&mut registry, dir_path);

        // YAML that parses but has no tasks (should fail validation)
        let yaml = "name: bad-workflow\ndescription: no tasks";
        let args = serde_json::json!({ "yaml": yaml });
        let result = registry.call_tool("workflow.create", args).await;
        assert!(result.is_err(), "Expected error for invalid workflow YAML");
    }

    #[tokio::test]
    async fn test_workflow_create_custom_name() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let mut registry = ToolRegistry::new();
        register_workflow_create(&mut registry, dir_path);

        let yaml = r#"name: original-name
tasks:
  task1:
    tool: llm.ask
    arguments:
      prompt: "test"
"#;
        let args = serde_json::json!({ "yaml": yaml, "name": "custom-name" });
        let result = registry.call_tool("workflow.create", args).await.unwrap();
        assert!(!result.is_error);

        // Should use custom name
        assert!(dir.path().join("custom-name.yaml").exists());
    }

    #[tokio::test]
    async fn test_workflow_create_with_cron_trigger() {
        let dir = TempDir::new().unwrap();
        let dir_path = dir.path().to_str().unwrap().to_string();

        let mut registry = ToolRegistry::new();
        register_workflow_create(&mut registry, dir_path);

        let yaml = r#"name: daily-report
triggers:
  - type: cron
    schedule: "0 9 * * *"
tasks:
  report:
    tool: llm.ask
    arguments:
      prompt: "Generate daily report"
"#;
        let args = serde_json::json!({ "yaml": yaml });
        let result = registry.call_tool("workflow.create", args).await.unwrap();
        assert!(!result.is_error);

        let content = std::fs::read_to_string(dir.path().join("daily-report.yaml")).unwrap();
        assert!(content.contains("cron"));
        assert!(content.contains("0 9 * * *"));
    }
}
