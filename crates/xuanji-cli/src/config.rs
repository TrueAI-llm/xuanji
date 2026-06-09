use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use xuanji_agent::types::AgentConfig;
use xuanji_llm::LlmConfig;
use xuanji_plugin::types::McpServerConfig;

fn default_llm_config() -> LlmConfig {
    LlmConfig {
        default_provider: None,
        providers: HashMap::new(),
    }
}

#[derive(Debug, Deserialize)]
pub struct XuanjiConfig {
    #[serde(default = "default_llm_config")]
    pub llm: LlmConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default, rename = "mcp_server")]
    pub mcp_servers: Vec<McpServerConfig>,
}

impl Default for XuanjiConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                default_provider: None,
                providers: HashMap::new(),
            },
            agent: AgentConfig::default(),
            mcp_servers: Vec::new(),
        }
    }
}

impl XuanjiConfig {
    /// Load config: global (~/.xuanji/config.toml) then local (./xuanji.toml).
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        // Global config
        let global_path = dirs_data().join("config.toml");
        if global_path.exists() {
            let content = std::fs::read_to_string(&global_path)
                .context(format!("Reading {:?}", global_path))?;
            let global: Self = toml::from_str(&content)?;
            config = config.merge(global);
        }

        // Project-local config (overrides global)
        let local_path = PathBuf::from("xuanji.toml");
        if local_path.exists() {
            let content = std::fs::read_to_string(&local_path)
                .context("Reading ./xuanji.toml")?;
            let local: Self = toml::from_str(&content)?;
            config = config.merge(local);
        }

        // Resolve environment variables in api_keys
        for provider in config.llm.providers.values_mut() {
            resolve_env_in_string(&mut provider.api_key);
        }

        Ok(config)
    }

    fn merge(mut self, other: Self) -> Self {
        if other.llm.default_provider.is_some() {
            self.llm.default_provider = other.llm.default_provider;
        }
        self.llm.providers.extend(other.llm.providers);
        if !other.mcp_servers.is_empty() {
            self.mcp_servers = other.mcp_servers;
        }
        self
    }
}

/// Replace ${VAR} patterns in a string with the environment variable value.
fn resolve_env_in_string(s: &mut String) {
    if s.starts_with("${") && s.ends_with('}') {
        let var_name = &s[2..s.len() - 1];
        if let Ok(val) = std::env::var(var_name) {
            *s = val;
        }
    }
}

fn dirs_data() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".xuanji")
}
