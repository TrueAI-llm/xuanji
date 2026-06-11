use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use xuanji_agent::types::AgentConfig;
use xuanji_budget::BudgetConfig;
use xuanji_llm::LlmConfig;
use xuanji_memory::MemoryConfig;
use xuanji_plugin::types::McpServerConfig;
use xuanji_trigger::TriggerConfig;

fn default_llm_config() -> LlmConfig {
    LlmConfig {
        default_provider: None,
        providers: HashMap::new(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct XuanjiConfig {
    #[serde(default = "default_llm_config")]
    pub llm: LlmConfig,
    #[serde(default)]
    pub agent: AgentConfig,
    #[serde(default, rename = "mcp_server")]
    pub mcp_servers: Vec<McpServerConfig>,
    #[serde(default)]
    pub trigger: TriggerConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub budget: BudgetConfig,
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
            trigger: TriggerConfig::default(),
            memory: MemoryConfig::default(),
            budget: BudgetConfig::default(),
        }
    }
}

impl XuanjiConfig {
    /// Load config: global (~/.xuanji/config.toml) then local (./xuanji.toml).
    /// Resolves ${VAR} patterns in api_keys for runtime use.
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

    /// Load only the global config file (no env var resolution, no merge).
    /// Used by add/remove/install to preserve ${VAR} patterns.
    pub fn load_global_only() -> Result<Self> {
        let global_path = dirs_data().join("config.toml");
        if global_path.exists() {
            let content = std::fs::read_to_string(&global_path)
                .context(format!("Reading {:?}", global_path))?;
            let config: Self = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Load only the local config file (no env var resolution, no merge).
    /// Used by add/remove/install to preserve ${VAR} patterns.
    pub fn load_local_only() -> Result<Self> {
        let local_path = PathBuf::from("xuanji.toml");
        if local_path.exists() {
            let content = std::fs::read_to_string(&local_path)
                .context("Reading ./xuanji.toml")?;
            let config: Self = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save config to the project-local xuanji.toml file.
    pub fn save_local(&self) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .context("Serializing config to TOML")?;
        std::fs::write("xuanji.toml", content)
            .context("Writing xuanji.toml")?;
        Ok(())
    }

    /// Save config to the global ~/.xuanji/config.toml file.
    pub fn save_global(&self) -> Result<()> {
        let dir = dirs_data();
        std::fs::create_dir_all(&dir)
            .context(format!("Creating directory {:?}", dir))?;
        let content = toml::to_string_pretty(self)
            .context("Serializing config to TOML")?;
        std::fs::write(dir.join("config.toml"), content)
            .context("Writing global config")?;
        Ok(())
    }

    /// Add or replace an MCP server config entry by name.
    pub fn add_mcp_server(&mut self, server: McpServerConfig) {
        if let Some(existing) = self.mcp_servers.iter_mut().find(|s| s.name == server.name) {
            *existing = server;
        } else {
            self.mcp_servers.push(server);
        }
    }

    /// Clone the config for saving (preserves ${VAR} patterns in api_keys).
    /// Unlike load(), this does NOT resolve environment variables.
    pub fn clone_for_save(&self) -> Self {
        Self {
            llm: LlmConfig {
                default_provider: self.llm.default_provider.clone(),
                providers: self.llm.providers.clone(),
            },
            agent: self.agent.clone(),
            mcp_servers: self.mcp_servers.clone(),
            trigger: self.trigger.clone(),
            memory: self.memory.clone(),
            budget: self.budget.clone(),
        }
    }

    /// Remove an MCP server config entry by name. Returns true if found and removed.
    pub fn remove_mcp_server(&mut self, name: &str) -> bool {
        let before = self.mcp_servers.len();
        self.mcp_servers.retain(|s| s.name != name);
        self.mcp_servers.len() != before
    }

    fn merge(mut self, other: Self) -> Self {
        if other.llm.default_provider.is_some() {
            self.llm.default_provider = other.llm.default_provider;
        }
        self.llm.providers.extend(other.llm.providers);
        self.agent = other.agent;
        if !other.mcp_servers.is_empty() {
            self.mcp_servers = other.mcp_servers;
        }
        self.trigger = other.trigger;
        self.memory = other.memory;
        self.budget = other.budget;
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
