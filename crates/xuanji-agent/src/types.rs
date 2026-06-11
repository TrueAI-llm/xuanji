use serde::{Deserialize, Serialize};

/// Agent loop configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_max_loops")]
    pub max_loops: u32,
    #[serde(default = "default_step_timeout")]
    pub step_timeout: String,
    #[serde(default = "default_true")]
    pub confirm_risky: bool,
    #[serde(default)]
    pub risky_patterns: Vec<RiskyPattern>,
    /// When true, don't send tools via API (for models that don't support tool calling).
    /// Instead, include tool descriptions in the system prompt and parse text output.
    #[serde(default)]
    pub text_tool_mode: bool,
}

fn default_max_loops() -> u32 { 20 }
fn default_step_timeout() -> String { "60s".into() }
fn default_true() -> bool { true }

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_loops: default_max_loops(),
            step_timeout: default_step_timeout(),
            confirm_risky: true,
            risky_patterns: Vec::new(),
            text_tool_mode: false,
        }
    }
}

/// A pattern for detecting risky tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskyPattern {
    pub tool: String,
    pub pattern: String,
}

/// Result of a single tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub result: String,
    pub success: bool,
}

/// Outcome of an `Agent::run` call: the final text plus real execution statistics.
///
/// Carrying counts here (instead of only the final `String`) lets the Role layer
/// reflect on genuine tool/token usage rather than fabricated zeros.
#[derive(Debug, Clone, Default)]
pub struct ExecutionStats {
    /// The agent's final textual answer.
    pub text: String,
    /// Number of tool calls executed during the run.
    pub tool_calls: u32,
    /// Total tokens reported by the provider across all LLM calls in the run.
    pub tokens: u32,
    /// Whether the run completed without tool errors.
    pub success: bool,
}
