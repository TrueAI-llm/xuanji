use crate::types::AgentConfig;
use regex::Regex;

/// Checks tool calls against configured risk patterns.
pub struct RiskChecker {
    patterns: Vec<(String, Regex)>,
}

impl RiskChecker {
    pub fn new(config: &AgentConfig) -> Self {
        let patterns = config
            .risky_patterns
            .iter()
            .filter_map(|p| Regex::new(&p.pattern).ok().map(|re| (p.tool.clone(), re)))
            .collect();
        Self { patterns }
    }

    /// Check if a tool call is considered risky.
    pub fn is_risky(&self, tool_name: &str, arguments: &serde_json::Value) -> bool {
        let args_str = arguments.to_string();
        self.patterns
            .iter()
            .any(|(tool, pattern)| tool_name == tool && pattern.is_match(&args_str))
    }
}
