use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use xuanji_core::{parse_workflow, DagScheduler, WorkflowInputs};
use xuanji_llm::ProviderConfig;
use xuanji_plugin::types::McpServerConfig;

use super::runtime::{create_provider_arc, create_registry};

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
    let provider = create_provider_arc(provider_config)?;
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
