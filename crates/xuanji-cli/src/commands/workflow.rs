use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use xuanji_core::{parse_workflow, DagScheduler, WorkflowInputs};
use xuanji_llm::openai::OpenAIProvider;
use xuanji_llm::anthropic::AnthropicProvider;
use xuanji_llm::{LlmProvider, ProviderConfig, Protocol};
use xuanji_plugin::process::McpProcess;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;

/// Run a YAML workflow.
pub async fn run_workflow(
    workflow_path: &str,
    input_pairs: &[String],
    provider_config: &ProviderConfig,
    mcp_servers: &[McpServerConfig],
) -> Result<()> {
    // 1. Read and parse the YAML file
    let yaml_str = std::fs::read_to_string(workflow_path)?;
    let workflow = parse_workflow(&yaml_str).map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("▶ Running workflow: {}\n", workflow.name);

    // 2. Parse input parameters
    let inputs = parse_inputs(input_pairs, &workflow.inputs);

    // 3. Create provider and registry
    let provider = create_provider(provider_config)?;
    let mut registry = create_registry(mcp_servers).await?;

    // 4. Register system tools
    xuanji_core::register_system_tools(&mut registry, provider);

    // 5. Wrap in Arc and run
    let registry = Arc::new(registry);
    let scheduler = DagScheduler::new(registry.clone());

    let result = scheduler
        .execute(&workflow, &inputs)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // 6. Print results
    println!("{}", result.display_summary());

    registry.shutdown_all().await?;

    if !result.overall_success() {
        anyhow::bail!("Workflow completed with failures");
    }

    Ok(())
}

fn parse_inputs(
    pairs: &[String],
    defaults: &HashMap<String, xuanji_core::InputDef>,
) -> WorkflowInputs {
    let mut inputs = WorkflowInputs::new();

    // Apply defaults first
    for (name, def) in defaults {
        if let Some(ref default) = def.default {
            inputs.insert(name.clone(), default.clone());
        }
    }

    // Override with CLI-provided values
    for pair in pairs {
        if let Some((key, value)) = pair.split_once('=') {
            inputs.insert(key.to_string(), serde_json::Value::String(value.to_string()));
        }
    }

    inputs
}

fn create_provider(config: &ProviderConfig) -> Result<Arc<dyn LlmProvider>> {
    let mut config = config.clone();
    if config.api_key.starts_with("${") && config.api_key.ends_with('}') {
        let var_name = &config.api_key[2..config.api_key.len() - 1];
        config.api_key = std::env::var(var_name).unwrap_or_default();
    }

    match config.protocol {
        Protocol::OpenAI => Ok(Arc::new(OpenAIProvider::new(config))),
        Protocol::Anthropic => Ok(Arc::new(AnthropicProvider::new(config))),
        Protocol::Gemini => anyhow::bail!("Gemini protocol not yet implemented"),
    }
}

async fn create_registry(mcp_servers: &[McpServerConfig]) -> Result<ToolRegistry> {
    let mut registry = ToolRegistry::new();

    for server_config in mcp_servers {
        let process = McpProcess::new(server_config.clone());
        let mut client = xuanji_plugin::McpClient::new(process);
        match client.initialize().await {
            Ok(()) => {
                registry.register_server(client).await?;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to start MCP server '{}': {}. Skipping.",
                    server_config.name,
                    e
                );
            }
        }
    }

    Ok(registry)
}
