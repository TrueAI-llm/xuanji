use serde::{Deserialize, Serialize};

use crate::protocol::Protocol;

/// Configuration for a single LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub protocol: Protocol,
    pub model: String,
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub temperature: Option<f64>,
}

/// Top-level LLM configuration containing one or more providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub default_provider: Option<String>,
    pub providers: std::collections::HashMap<String, ProviderConfig>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_provider_config_from_toml() {
        let toml_str = r#"
            protocol = "openai"
            model = "gpt-4"
            api_key = "sk-test-key"
            base_url = "https://api.openai.com"
            max_tokens = 4096
            temperature = 0.7
        "#;
        let config: ProviderConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.protocol, Protocol::OpenAI);
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.api_key, "sk-test-key");
        assert_eq!(config.base_url.as_deref(), Some("https://api.openai.com"));
        assert_eq!(config.max_tokens, Some(4096));
        assert!((config.temperature.unwrap() - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_llm_config_from_toml() {
        let toml_str = r#"
            default_provider = "openai"

            [providers.openai]
            protocol = "openai"
            model = "gpt-4"
            api_key = "sk-test-key"

            [providers.anthropic]
            protocol = "anthropic"
            model = "claude-sonnet-4-20250514"
            api_key = "sk-ant-test"
        "#;
        let config: LlmConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.default_provider.as_deref(), Some("openai"));
        assert_eq!(config.providers.len(), 2);
        assert!(config.providers.contains_key("openai"));
        assert!(config.providers.contains_key("anthropic"));
    }
}
