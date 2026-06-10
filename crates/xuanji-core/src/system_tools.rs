use crate::error::CoreError;
use std::sync::Arc;
use xuanji_llm::types::Message;
use xuanji_llm::LlmProvider;
use xuanji_plugin::ToolRegistry;

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

/// Schema for workflow.run tool
pub const WORKFLOW_RUN_SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "path": { "type": "string", "description": "Path to the workflow YAML file" },
        "inputs": { "type": "object", "description": "Input parameters for the workflow" }
    },
    "required": ["path"]
}"#;

/// Register system tools (llm.ask) with the tool registry.
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
