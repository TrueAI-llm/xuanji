# Xuanji MVP Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a working AI Agent CLI that accepts natural language input, calls LLM providers, and executes tools via MCP protocol.

**Architecture:** Rust workspace with 5 crates: `xuanji-llm` (LLM abstraction), `xuanji-plugin` (MCP client), `xuanji-agent` (ReAct loop), `xuanji-memory` (basic memory), `xuanji-cli` (binary entry). The agent loop drives everything: it sends prompts to LLM, parses tool call responses, dispatches to MCP tools or system tools, and loops until done.

**Tech Stack:** Rust, tokio, reqwest, clap, serde + serde_json, serde_yml, toml, tracing, rmcp (Rust MCP client crate)

**Spec:** `docs/superpowers/specs/2026-06-09-xuanji-design.md`

---

## File Structure

```
xuanji/
├── Cargo.toml                          # workspace root
├── crates/
│   ├── xuanji-llm/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs                  # crate entry, re-exports
│   │       ├── protocol.rs             # Protocol enum
│   │       ├── config.rs               # ProviderConfig, Config structs
│   │       ├── provider.rs             # LlmProvider trait
│   │       ├── openai.rs               # OpenAI-compatible adapter
│   │       ├── anthropic.rs            # Anthropic adapter
│   │       ├── types.rs                # Message, ToolSchema, LlmResponse, etc.
│   │       └── error.rs                # LlmError enum
│   ├── xuanji-plugin/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs               # MCP client (using rmcp)
│   │       ├── registry.rs             # ToolRegistry
│   │       ├── process.rs              # MCP server process manager
│   │       ├── types.rs                # McpServerConfig, etc.
│   │       └── error.rs
│   ├── xuanji-agent/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── agent.rs                # Agent loop
│   │       ├── types.rs                # AgentResponse, Message, ToolResult, etc.
│   │       ├── prompt.rs               # System prompt construction
│   │       ├── risk.rs                 # Risk pattern matching
│   │       ├── context.rs              # Context compression (sliding window)
│   │       └── error.rs
│   ├── xuanji-memory/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── short_term.rs           # Short-term memory (in-memory)
│   │       ├── working.rs              # Working memory (task progress)
│   │       └── types.rs
│   └── xuanji-cli/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                 # CLI entry
│           ├── config.rs               # Config loading & merging
│           └── commands/
│               ├── mod.rs
│               ├── agent.rs            # `xuanji "..."` and `xuanji chat`
│               └── mcp.rs              # `xuanji mcp list/info/call`
├── plugins/
│   └── xuanji-mcp-shell/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs                 # Shell MCP server
├── xuanji.toml                         # example config
└── tests/
    └── integration/
        └── agent_e2e.rs                # end-to-end agent test
```

---

## Chunk 1: Workspace & Core Types

### Task 1: Initialize Cargo Workspace

**Files:**
- Create: `Cargo.toml`
- Create: `crates/xuanji-llm/Cargo.toml`
- Create: `crates/xuanji-llm/src/lib.rs`
- Create: `crates/xuanji-plugin/Cargo.toml`
- Create: `crates/xuanji-plugin/src/lib.rs`
- Create: `crates/xuanji-agent/Cargo.toml`
- Create: `crates/xuanji-agent/src/lib.rs`
- Create: `crates/xuanji-memory/Cargo.toml`
- Create: `crates/xuanji-memory/src/lib.rs`
- Create: `crates/xuanji-cli/Cargo.toml`
- Create: `crates/xuanji-cli/src/main.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/xuanji-llm",
    "crates/xuanji-plugin",
    "crates/xuanji-agent",
    "crates/xuanji-memory",
    "crates/xuanji-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
async-trait = "0.1"
```

- [ ] **Step 2: Create xuanji-llm Cargo.toml**

```toml
[package]
name = "xuanji-llm"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
async-trait.workspace = true
reqwest = { version = "0.12", features = ["json", "stream"] }
futures = "0.3"
schemars = "0.8"
```

- [ ] **Step 3: Create xuanji-llm/src/lib.rs**

```rust
pub mod config;
pub mod error;
pub mod openai;
pub mod anthropic;
pub mod protocol;
pub mod provider;
pub mod types;

pub use config::LlmConfig;
pub use error::LlmError;
pub use protocol::Protocol;
pub use provider::LlmProvider;
pub use types::*;
```

- [ ] **Step 4: Create xuanji-plugin Cargo.toml**

```toml
[package]
name = "xuanji-plugin"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
async-trait.workspace = true
tracing.workspace = true
rmcp = "0.1"
```

- [ ] **Step 5: Create xuanji-plugin/src/lib.rs**

```rust
pub mod client;
pub mod error;
pub mod process;
pub mod registry;
pub mod types;

pub use error::PluginError;
pub use registry::ToolRegistry;
pub use types::McpServerConfig;
```

- [ ] **Step 6: Create xuanji-agent Cargo.toml**

```toml
[package]
name = "xuanji-agent"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
async-trait.workspace = true
tracing.workspace = true
regex = "1"
xuanji-llm = { path = "../xuanji-llm" }
xuanji-plugin = { path = "../xuanji-plugin" }
xuanji-memory = { path = "../xuanji-memory" }
```

- [ ] **Step 7: Create xuanji-agent/src/lib.rs**

```rust
pub mod agent;
pub mod context;
pub mod error;
pub mod prompt;
pub mod risk;
pub mod types;

pub use agent::Agent;
pub use error::AgentError;
pub use types::*;
```

- [ ] **Step 8: Create xuanji-memory Cargo.toml**

```toml
[package]
name = "xuanji-memory"
version.workspace = true
edition.workspace = true

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
thiserror.workspace = true
```

- [ ] **Step 9: Create xuanji-memory/src/lib.rs**

```rust
pub mod short_term;
pub mod types;
pub mod working;

pub use short_term::ShortTermMemory;
pub use types::*;
pub use working::WorkingMemory;
```

- [ ] **Step 10: Create xuanji-cli Cargo.toml**

```toml
[package]
name = "xuanji-cli"
version.workspace = true
edition.workspace = true

[[bin]]
name = "xuanji"
path = "src/main.rs"

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
clap = { version = "4", features = ["derive"] }
toml = "0.8"
xuanji-llm = { path = "../xuanji-llm" }
xuanji-plugin = { path = "../xuanji-plugin" }
xuanji-agent = { path = "../xuanji-agent" }
xuanji-memory = { path = "../xuanji-memory" }
```

- [ ] **Step 11: Create xuanji-cli/src/main.rs**

```rust
fn main() {
    println!("xuanji v0.1.0 - not yet implemented");
}
```

- [ ] **Step 12: Verify workspace compiles**

Run: `cargo build`
Expected: SUCCESS, all crates compile with empty lib.rs files

- [ ] **Step 13: Commit**

```bash
git add -A
git commit -m "feat: initialize cargo workspace with 5 crates"
```

---

### Task 2: LLM Core Types & Protocol

**Files:**
- Create: `crates/xuanji-llm/src/protocol.rs`
- Create: `crates/xuanji-llm/src/config.rs`
- Create: `crates/xuanji-llm/src/types.rs`
- Create: `crates/xuanji-llm/src/error.rs`
- Create: `crates/xuanji-llm/src/provider.rs`
- Test: `crates/xuanji-llm/tests/protocol_test.rs`

- [ ] **Step 1: Write failing test for Protocol enum**

Create `crates/xuanji-llm/tests/protocol_test.rs`:

```rust
use xuanji_llm::Protocol;

#[test]
fn test_protocol_from_str() {
    assert_eq!("openai".parse::<Protocol>(), Ok(Protocol::OpenAI));
    assert_eq!("anthropic".parse::<Protocol>(), Ok(Protocol::Anthropic));
    assert_eq!("gemini".parse::<Protocol>(), Ok(Protocol::Gemini));
    assert!("invalid".parse::<Protocol>().is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xuanji-llm --test protocol_test`
Expected: FAIL - Protocol not found

- [ ] **Step 3: Implement protocol.rs**

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    OpenAI,
    Anthropic,
    Gemini,
}

#[derive(Debug, Error)]
#[error("unknown protocol: {0}")]
pub struct UnknownProtocol(String);

impl FromStr for Protocol {
    type Err = UnknownProtocol;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "openai" => Ok(Self::OpenAI),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            _ => Err(UnknownProtocol(s.to_string())),
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenAI => write!(f, "openai"),
            Self::Anthropic => write!(f, "anthropic"),
            Self::Gemini => write!(f, "gemini"),
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xuanji-llm --test protocol_test`
Expected: PASS

- [ ] **Step 5: Write failing test for ProviderConfig**

Add to `crates/xuanji-llm/tests/protocol_test.rs`:

```rust
use xuanji_llm::config::ProviderConfig;
use std::time::Duration;

#[test]
fn test_provider_config_toml_parse() {
    let toml_str = r#"
protocol = "openai"
base_url = "https://api.example.com/v1"
api_key = "sk-test"
model = "gpt-4o"
timeout = "120s"
max_tokens = 4096
temperature = 0.7
"#;
    let config: ProviderConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.protocol, Protocol::OpenAI);
    assert_eq!(config.base_url, "https://api.example.com/v1");
    assert_eq!(config.model, "gpt-4o");
    assert_eq!(config.api_key, Some("sk-test".to_string()));
    assert_eq!(config.max_tokens, Some(4096));
}
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cargo test -p xuanji-llm --test protocol_test`
Expected: FAIL - config module not found

- [ ] **Step 7: Implement config.rs**

```rust
use crate::Protocol;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub protocol: Protocol,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    #[serde(default)]
    pub timeout: Option<String>,   // TOML doesn't natively support Duration
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub default: String,
    #[serde(default)]
    pub providers: std::collections::HashMap<String, ProviderConfig>,
}
```

Add `toml` dependency to xuanji-llm Cargo.toml:

```toml
toml = "0.8"
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cargo test -p xuanji-llm --test protocol_test`
Expected: PASS

- [ ] **Step 9: Implement types.rs (LLM message types)**

```rust
use serde::{Deserialize, Serialize};

/// Tool schema exposed to LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value, // JSON Schema
}

/// A single tool call from LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: Option<String>,
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// LLM response
#[derive(Debug, Clone)]
pub enum LlmResponse {
    /// LLM wants to call one or more tools
    ToolCalls {
        calls: Vec<ToolCall>,
        text: Option<String>,
    },
    /// LLM responds with text only
    Text { content: String },
}

/// Message in conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    System { content: String },
    User { content: String },
    Assistant { content: String },
    AssistantToolCalls { calls: Vec<ToolCall> },
    ToolResult {
        tool_call_id: Option<String>,
        tool_name: String,
        result: String,
        success: bool,
    },
}
```

- [ ] **Step 10: Implement error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error: status={status}, message={message}")]
    Api { status: u16, message: String },

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Context window exceeded")]
    ContextExceeded,

    #[error("Streaming error: {0}")]
    Streaming(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
```

- [ ] **Step 11: Implement provider.rs (trait)**

```rust
use crate::error::LlmError;
use crate::types::{LlmResponse, Message, ToolSchema};
use async_trait::async_trait;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider name for logging
    fn name(&self) -> &str;

    /// Send a chat completion request
    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<LlmResponse, LlmError>;
}
```

- [ ] **Step 12: Verify all compiles**

Run: `cargo build -p xuanji-llm`
Expected: SUCCESS

- [ ] **Step 13: Commit**

```bash
git add -A
git commit -m "feat(llm): add core types, protocol, config, and provider trait"
```

---

## Chunk 2: LLM Adapters

### Task 3: OpenAI-Compatible Adapter

**Files:**
- Create: `crates/xuanji-llm/src/openai.rs`
- Test: `crates/xuanji-llm/tests/openai_test.rs`

- [ ] **Step 1: Write failing test for OpenAI request format**

Create `crates/xuanji-llm/tests/openai_test.rs`:

```rust
use xuanji_llm::openai::OpenAiAdapter;
use xuanji_llm::types::{Message, ToolSchema};
use xuanji_llm::config::ProviderConfig;
use xuanji_llm::Protocol;

fn test_config() -> ProviderConfig {
    ProviderConfig {
        protocol: Protocol::OpenAI,
        base_url: "https://api.example.com/v1".into(),
        api_key: Some("sk-test".into()),
        model: "gpt-4o".into(),
        timeout: Some("120s".into()),
        max_tokens: Some(4096),
        temperature: Some(0.7),
    }
}

#[tokio::test]
async fn test_openai_build_request_body() {
    let adapter = OpenAiAdapter::new("test".into(), test_config()).unwrap();
    let messages = vec![
        Message::System { content: "You are helpful.".into() },
        Message::User { content: "Hello".into() },
    ];
    let tools = vec![ToolSchema {
        name: "shell.run".into(),
        description: "Run a command".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"]
        }),
    }];

    let body = adapter.build_request_body(&messages, &tools, None, None);
    assert_eq!(body["model"], "gpt-4o");
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert!(body["tools"].is_array());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xuanji-llm --test openai_test`
Expected: FAIL - openai module not found

- [ ] **Step 3: Implement OpenAI adapter**

Create `crates/xuanji-llm/src/openai.rs`:

```rust
use crate::config::ProviderConfig;
use crate::error::LlmError;
use crate::provider::LlmProvider;
use crate::types::{LlmResponse, Message, ToolCall, ToolSchema};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct OpenAiAdapter {
    name: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
    client: reqwest::Client,
}

impl OpenAiAdapter {
    pub fn new(name: String, config: ProviderConfig) -> Result<Self, LlmError> {
        let base_url = config.base_url.trim_end_matches('/').to_string();
        Ok(Self {
            name,
            base_url,
            api_key: config.api_key,
            model: config.model,
            client: reqwest::Client::new(),
        })
    }

    pub fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Value {
        let msgs: Vec<Value> = messages.iter().map(|m| message_to_openai(m)).collect();
        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "messages": msgs,
        });

        if !tools_json.is_empty() {
            body["tools"] = json!(tools_json);
        }
        if let Some(t) = temperature {
            body["temperature"] = json!(t);
        }
        if let Some(mt) = max_tokens {
            body["max_tokens"] = json!(mt);
        }

        body
    }
}

fn message_to_openai(msg: &Message) -> Value {
    match msg {
        Message::System { content } => json!({"role": "system", "content": content}),
        Message::User { content } => json!({"role": "user", "content": content}),
        Message::Assistant { content } => json!({"role": "assistant", "content": content}),
        Message::AssistantToolCalls { calls } => {
            let tool_calls: Vec<Value> = calls
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "type": "function",
                        "function": {
                            "name": c.tool_name,
                            "arguments": c.arguments.to_string(),
                        }
                    })
                })
                .collect();
            json!({"role": "assistant", "tool_calls": tool_calls})
        }
        Message::ToolResult {
            tool_call_id,
            tool_name: _,
            result,
            success,
        } => json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": result,
        }),
    }
}

fn parse_openai_response(value: Value) -> Result<LlmResponse, LlmError> {
    let choice = &value["choices"][0]["message"];
    let content = choice["content"].as_str().unwrap_or("").to_string();

    let tool_calls_json = choice["tool_calls"].as_array();
    match tool_calls_json {
        Some(calls) if !calls.is_empty() => {
            let parsed: Vec<ToolCall> = calls
                .iter()
                .map(|c| ToolCall {
                    id: c["id"].as_str().map(String::from),
                    tool_name: c["function"]["name"].as_str().unwrap_or("").to_string(),
                    arguments: c["function"]["arguments"]
                        .as_str()
                        .and_then(|s| serde_json::from_str(s).ok())
                        .unwrap_or(json!({})),
                })
                .collect();
            Ok(LlmResponse::ToolCalls {
                calls: parsed,
                text: if content.is_empty() { None } else { Some(content) },
            })
        }
        _ => Ok(LlmResponse::Text { content }),
    }
}

#[async_trait]
impl LlmProvider for OpenAiAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(&messages, &tools, temperature, max_tokens);

        let mut req = self.client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req.send().await?;
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: text,
            });
        }

        let value: Value = resp.json().await?;
        parse_openai_response(value)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xuanji-llm --test openai_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(llm): implement OpenAI-compatible adapter"
```

---

### Task 4: Anthropic Adapter

**Files:**
- Create: `crates/xuanji-llm/src/anthropic.rs`
- Test: `crates/xuanji-llm/tests/anthropic_test.rs`

- [ ] **Step 1: Write failing test for Anthropic request format**

Create `crates/xuanji-llm/tests/anthropic_test.rs`:

```rust
use xuanji_llm::anthropic::AnthropicAdapter;
use xuanji_llm::config::ProviderConfig;
use xuanji_llm::types::{Message, ToolSchema};
use xuanji_llm::Protocol;

fn test_config() -> ProviderConfig {
    ProviderConfig {
        protocol: Protocol::Anthropic,
        base_url: "https://api.anthropic.com".into(),
        api_key: Some("sk-ant-test".into()),
        model: "claude-sonnet-4-6".into(),
        timeout: Some("120s".into()),
        max_tokens: Some(4096),
        temperature: Some(0.7),
    }
}

#[tokio::test]
async fn test_anthropic_build_request_body() {
    let adapter = AnthropicAdapter::new("test".into(), test_config()).unwrap();
    let messages = vec![
        Message::System { content: "You are helpful.".into() },
        Message::User { content: "Hello".into() },
    ];
    let tools = vec![ToolSchema {
        name: "shell.run".into(),
        description: "Run a command".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "command": { "type": "string" } },
            "required": ["command"]
        }),
    }];

    let (system, body) = adapter.build_request_body(&messages, &tools, None, None);
    // Anthropic separates system prompt
    assert!(system.is_some());
    assert_eq!(body["model"], "claude-sonnet-4-6");
    // messages should not contain system role
    let msgs = body["messages"].as_array().unwrap();
    assert!(msgs.iter().all(|m| m["role"] != "system"));
    assert!(body["tools"].is_array());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xuanji-llm --test anthropic_test`
Expected: FAIL - anthropic module not found

- [ ] **Step 3: Implement Anthropic adapter**

Create `crates/xuanji-llm/src/anthropic.rs`:

```rust
use crate::config::ProviderConfig;
use crate::error::LlmError;
use crate::provider::LlmProvider;
use crate::types::{LlmResponse, Message, ToolCall, ToolSchema};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct AnthropicAdapter {
    name: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
    client: reqwest::Client,
}

impl AnthropicAdapter {
    pub fn new(name: String, config: ProviderConfig) -> Result<Self, LlmError> {
        let base_url = config.base_url.trim_end_matches('/').to_string();
        Ok(Self {
            name,
            base_url,
            api_key: config.api_key,
            model: config.model,
            client: reqwest::Client::new(),
        })
    }

    /// Returns (system_prompt, body) — Anthropic separates system from messages
    pub fn build_request_body(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> (Option<String>, Value) {
        let mut system_prompt = None;
        let mut msgs: Vec<Value> = Vec::new();

        for msg in messages {
            match msg {
                Message::System { content } => {
                    system_prompt = Some(content.clone());
                }
                Message::User { content } => {
                    msgs.push(json!({"role": "user", "content": content}));
                }
                Message::Assistant { content } => {
                    msgs.push(json!({"role": "assistant", "content": content}));
                }
                Message::AssistantToolCalls { calls } => {
                    let tool_uses: Vec<Value> = calls
                        .iter()
                        .map(|c| {
                            json!({
                                "type": "tool_use",
                                "id": c.id,
                                "name": c.tool_name,
                                "input": c.arguments,
                            })
                        })
                        .collect();
                    msgs.push(json!({"role": "assistant", "content": tool_uses}));
                }
                Message::ToolResult {
                    tool_call_id,
                    tool_name: _,
                    result,
                    success,
                } => {
                    msgs.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": result,
                            "is_error": !success,
                        }]
                    }));
                }
            }
        }

        let tools_json: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect();

        let mut body = json!({
            "model": self.model,
            "max_tokens": max_tokens.unwrap_or(4096),
            "messages": msgs,
        });

        if !tools_json.is_empty() {
            body["tools"] = json!(tools_json);
        }
        if let Some(t) = temperature {
            body["temperature"] = json!(t);
        }

        (system_prompt, body)
    }
}

fn parse_anthropic_response(value: Value) -> Result<LlmResponse, LlmError> {
    let content_blocks = value["content"].as_array();
    match content_blocks {
        Some(blocks) => {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = block["text"].as_str() {
                            text_parts.push(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().map(String::from),
                            tool_name: block["name"].as_str().unwrap_or("").to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                    _ => {}
                }
            }

            if !tool_calls.is_empty() {
                let text = if text_parts.is_empty() {
                    None
                } else {
                    Some(text_parts.join(""))
                };
                Ok(LlmResponse::ToolCalls { calls: tool_calls, text })
            } else {
                Ok(LlmResponse::Text {
                    content: text_parts.join(""),
                })
            }
        }
        None => Ok(LlmResponse::Text {
            content: String::new(),
        }),
    }
}

#[async_trait]
impl LlmProvider for AnthropicAdapter {
    fn name(&self) -> &str {
        &self.name
    }

    async fn complete(
        &self,
        messages: Vec<Message>,
        tools: Vec<ToolSchema>,
        temperature: Option<f32>,
        max_tokens: Option<u32>,
    ) -> Result<LlmResponse, LlmError> {
        let url = format!("{}/v1/messages", self.base_url);
        let (system, body) = self.build_request_body(&messages, &tools, temperature, max_tokens);

        let mut final_body = body;
        if let Some(sys) = system {
            final_body["system"] = json!(sys);
        }

        let mut req = self.client.post(&url).json(&final_body);
        if let Some(ref key) = self.api_key {
            req = req
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        }

        let resp = req.send().await?;
        let status = resp.status();

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api {
                status: status.as_u16(),
                message: text,
            });
        }

        let value: Value = resp.json().await?;
        parse_anthropic_response(value)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xuanji-llm --test anthropic_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(llm): implement Anthropic adapter"
```

---

## Chunk 3: MCP Client & Tool Registry

### Task 5: MCP Client

**Files:**
- Create: `crates/xuanji-plugin/src/types.rs`
- Create: `crates/xuanji-plugin/src/error.rs`
- Create: `crates/xuanji-plugin/src/process.rs`
- Create: `crates/xuanji-plugin/src/client.rs`
- Test: `crates/xuanji-plugin/tests/process_test.rs`

- [ ] **Step 1: Implement types.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}
```

- [ ] **Step 2: Implement error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("MCP server process error: {0}")]
    Process(#[from] std::io::Error),

    #[error("MCP protocol error: {0}")]
    Protocol(String),

    #[error("MCP server '{0}' not found")]
    ServerNotFound(String),

    #[error("Tool '{0}' not found")]
    ToolNotFound(String),

    #[error("Tool execution failed: {0}")]
    ExecutionFailed(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
```

- [ ] **Step 3: Implement process.rs (MCP server process manager)**

```rust
use crate::error::PluginError;
use crate::types::McpServerConfig;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use std::sync::Arc;

pub struct McpProcess {
    config: McpServerConfig,
    child: Option<Child>,
}

impl McpProcess {
    pub fn new(config: McpServerConfig) -> Self {
        Self { config, child: None }
    }

    /// Start the MCP server process if not already running
    pub async fn ensure_started(&mut self) -> Result<(), PluginError> {
        if self.child.is_some() {
            // Check if still alive
            if let Some(ref mut child) = self.child {
                match child.try_wait()? {
                    Some(_) => {
                        // Process exited, restart
                        self.child = None;
                    }
                    None => return Ok(()), // Still running
                }
            }
        }

        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        let child = cmd.spawn()?;
        tracing::info!("Started MCP server '{}': {}", self.config.name, self.config.command);
        self.child = Some(child);
        Ok(())
    }

    /// Get stdin and stdout for communication
    pub fn get_pipes(
        &mut self,
    ) -> Result<(&mut tokio::process::ChildStdin, &mut tokio::process::ChildStdout), PluginError> {
        let child = self.child.as_mut().ok_or_else(|| {
            PluginError::Process(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "MCP server not started",
            ))
        })?;

        let stdin = child.stdin.as_mut().ok_or_else(|| {
            PluginError::Process(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stdin not available",
            ))
        })?;

        let stdout = child.stdout.as_mut().ok_or_else(|| {
            PluginError::Process(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stdout not available",
            ))
        })?;

        Ok((stdin, stdout))
    }

    /// Kill the MCP server process
    pub async fn kill(&mut self) -> Result<(), PluginError> {
        if let Some(ref mut child) = self.child {
            child.kill().await?;
            tracing::info!("Killed MCP server '{}'", self.config.name);
        }
        self.child = None;
        Ok(())
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Best effort kill on drop
            let _ = child.start_kill();
        }
    }
}
```

- [ ] **Step 4: Implement client.rs (MCP JSON-RPC client)**

```rust
use crate::error::PluginError;
use crate::process::McpProcess;
use crate::types::McpServerConfig;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

pub struct McpClient {
    process: McpProcess,
    next_id: u64,
    initialized: bool,
}

impl McpClient {
    pub fn new(config: McpServerConfig) -> Self {
        let process = McpProcess::new(config);
        Self {
            process,
            next_id: 1,
            initialized: false,
        }
    }

    /// Initialize the MCP server: start process + handshake
    pub async fn initialize(&mut self) -> Result<(), PluginError> {
        self.process.ensure_started().await?;

        // Send initialize request
        let resp = self.send_request("initialize", json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "xuanji", "version": "0.1.0" }
        })).await?;

        // Send initialized notification (no response expected)
        self.send_notification("notifications/initialized", json!({})).await?;

        self.initialized = true;
        tracing::info!("MCP server '{}' initialized", self.process.name());
        Ok(())
    }

    /// List available tools from the MCP server
    pub async fn list_tools(&mut self) -> Result<Vec<McpToolInfo>, PluginError> {
        if !self.initialized {
            self.initialize().await?;
        }

        let resp = self.send_request("tools/list", json!({})).await?;
        let tools: Vec<McpToolInfo> = serde_json::from_value(
            resp.get("tools").cloned().unwrap_or(json!([])),
        )?;

        tracing::debug!("MCP server '{}' has {} tools", self.process.name(), tools.len());
        Ok(tools)
    }

    /// Call a tool on the MCP server
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolResult, PluginError> {
        if !self.initialized {
            self.initialize().await?;
        }

        let resp = self.send_request("tools/call", json!({
            "name": tool_name,
            "arguments": arguments,
        })).await?;

        let is_error = resp.get("isError").and_then(|v| v.as_bool()).unwrap_or(false);
        let content = resp.get("content").cloned().unwrap_or(json!([]));

        Ok(McpToolResult { content, is_error })
    }

    /// Shutdown the MCP server
    pub async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.process.kill().await?;
        self.initialized = false;
        Ok(())
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value, PluginError> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let (stdin, stdout) = self.process.get_pipes()?;
        let request_str = format!("{}\n", serde_json::to_string(&request)?);

        // Use tokio async I/O
        let stdin = self.process.child.as_mut().unwrap().stdin.take().unwrap();
        let stdout = self.process.child.as_mut().unwrap().stdout.take().unwrap();

        let mut writer = tokio::io::BufWriter::new(stdin);
        writer.write_all(request_str.as_bytes()).await?;
        writer.flush().await?;

        let mut reader = tokio::io::BufReader::new(stdout);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        // Put pipes back
        self.process.child.as_mut().unwrap().stdin = Some(writer.into_inner());
        self.process.child.as_mut().unwrap().stdout = Some(reader.into_inner());

        let response: Value = serde_json::from_str(line.trim())?;

        if let Some(error) = response.get("error") {
            return Err(PluginError::Protocol(format!(
                "MCP error: {}",
                error
            )));
        }

        Ok(response.get("result").cloned().unwrap_or(json!({})))
    }

    async fn send_notification(&mut self, method: &str, params: Value) -> Result<(), PluginError> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let stdin = self.process.child.as_mut().unwrap().stdin.take().unwrap();
        let mut writer = tokio::io::BufWriter::new(stdin);
        let notif_str = format!("{}\n", serde_json::to_string(&notification)?);
        writer.write_all(notif_str.as_bytes()).await?;
        writer.flush().await?;
        self.process.child.as_mut().unwrap().stdin = Some(writer.into_inner());

        Ok(())
    }

    pub fn name(&self) -> &str {
        self.process.name()
    }
}

#[derive(Debug, Clone)]
pub struct McpToolResult {
    pub content: Value,
    pub is_error: bool,
}
```

- [ ] **Step 5: Implement registry.rs (Tool Registry)**

```rust
use crate::client::{McpClient, McpToolInfo, McpToolResult};
use crate::error::PluginError;
use crate::types::McpServerConfig;
use std::collections::HashMap;

/// Unified tool info (from MCP or system)
#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
    pub source: ToolSource,
}

#[derive(Debug, Clone)]
pub enum ToolSource {
    Mcp { server_name: String },
    System { tool_fn: String },
}

pub struct ToolRegistry {
    servers: HashMap<String, McpClient>,
    tools: HashMap<String, ToolEntry>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            tools: HashMap::new(),
        }
    }

    /// Register an MCP server (lazy — not started until first use)
    pub fn register_server(&mut self, config: McpServerConfig) {
        let name = config.name.clone();
        let client = McpClient::new(config);
        self.servers.insert(name, client);
    }

    /// Register a system tool
    pub fn register_system_tool(&mut self, entry: ToolEntry) {
        self.tools.insert(entry.name.clone(), entry);
    }

    /// Ensure an MCP server is started and its tools are loaded
    pub async fn ensure_loaded(&mut self, server_name: &str) -> Result<(), PluginError> {
        let client = self.servers.get_mut(server_name).ok_or_else(|| {
            PluginError::ServerNotFound(server_name.to_string())
        })?;

        let tools = client.list_tools().await?;
        for tool in tools {
            self.tools.insert(tool.name.clone(), ToolEntry {
                name: tool.name.clone(),
                description: tool.description.unwrap_or_default(),
                parameters: tool.input_schema,
                source: ToolSource::Mcp {
                    server_name: server_name.to_string(),
                },
            });
        }

        Ok(())
    }

    /// Load all registered MCP servers
    pub async fn load_all(&mut self) -> Result<(), PluginError> {
        let server_names: Vec<String> = self.servers.keys().cloned().collect();
        for name in server_names {
            self.ensure_loaded(&name).await?;
        }
        Ok(())
    }

    /// Get all tool schemas (for sending to LLM)
    pub fn all_tool_schemas(&self) -> Vec<xuanji_llm::ToolSchema> {
        self.tools
            .values()
            .map(|t| xuanji_llm::ToolSchema {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    /// Call a tool by name
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<McpToolResult, PluginError> {
        let entry = self.tools.get(tool_name).ok_or_else(|| {
            PluginError::ToolNotFound(tool_name.to_string())
        })?;

        match &entry.source {
            ToolSource::Mcp { server_name } => {
                self.ensure_loaded(server_name).await.ok(); // Ensure started
                let client = self.servers.get_mut(server_name).ok_or_else(|| {
                    PluginError::ServerNotFound(server_name.clone())
                })?;
                client.call_tool(tool_name, arguments).await
            }
            ToolSource::System { .. } => {
                // System tools are handled by the agent directly
                Err(PluginError::Protocol(format!(
                    "System tool '{}' should be handled by the agent",
                    tool_name
                )))
            }
        }
    }

    /// Shutdown all MCP servers
    pub async fn shutdown_all(&mut self) {
        for (name, client) in self.servers.iter_mut() {
            if let Err(e) = client.shutdown().await {
                tracing::warn!("Failed to shutdown MCP server '{}': {}", name, e);
            }
        }
    }

    /// List all registered tools
    pub fn list_tools(&self) -> Vec<&ToolEntry> {
        self.tools.values().collect()
    }

    /// Get tool by name
    pub fn get_tool(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.get(name)
    }
}
```

Add `xuanji-llm` dependency to `xuanji-plugin/Cargo.toml`:

```toml
xuanji-llm = { path = "../xuanji-llm" }
```

- [ ] **Step 6: Verify all compiles**

Run: `cargo build -p xuanji-plugin`
Expected: SUCCESS

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(plugin): implement MCP client, process manager, and tool registry"
```

---

### Task 6: Shell MCP Server

**Files:**
- Create: `plugins/xuanji-mcp-shell/Cargo.toml`
- Create: `plugins/xuanji-mcp-shell/src/main.rs`

- [ ] **Step 1: Create shell MCP server Cargo.toml**

```toml
[package]
name = "xuanji-mcp-shell"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "xuanji-mcp-shell"
path = "src/main.rs"

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

Add to workspace `Cargo.toml` members:

```toml
members = [
    "crates/*",
    "plugins/*",
]
```

- [ ] **Step 2: Implement shell MCP server**

```rust
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use tokio::process::Command;

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let result = match method {
            "initialize" => handle_initialize(),
            "notifications/initialized" => {
                // No response needed for notifications
                continue;
            }
            "tools/list" => handle_tools_list(),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                handle_tools_call(params)
            }
            _ => json!({"error": {"code": -32601, "message": format!("Unknown method: {}", method)}}),
        };

        if let Some(id) = id {
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result,
            });
            writeln!(stdout, "{}", serde_json::to_string(&response).unwrap()).ok();
        }
    }
}

fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "xuanji-mcp-shell",
            "version": "0.1.0"
        }
    })
}

fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "shell.run",
                "description": "在 shell 中执行命令",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "要执行的命令"
                        }
                    },
                    "required": ["command"]
                }
            }
        ]
    })
}

fn handle_tools_call(params: Value) -> Value {
    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match tool_name {
        "shell.run" => {
            let command = args.get("command").and_then(|c| c.as_str()).unwrap_or("");

            let rt = tokio::runtime::Runtime::new().unwrap();
            let output = rt.block_on(async {
                Command::new("sh")
                    .arg("-c")
                    .arg(command)
                    .output()
                    .await
            });

            match output {
                Ok(output) => {
                    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
                    let is_error = !output.status.success();

                    let mut content = vec![json!({
                        "type": "text",
                        "text": if stdout_str.is_empty() { stderr_str.clone() } else { stdout_str },
                    })];

                    if !stderr_str.is_empty() && !output.status.success() {
                        content.push(json!({
                            "type": "text",
                            "text": format!("stderr: {}", stderr_str),
                        }));
                    }

                    json!({
                        "content": content,
                        "isError": is_error,
                    })
                }
                Err(e) => json!({
                    "content": [{"type": "text", "text": format!("Failed to execute: {}", e)}],
                    "isError": true,
                }),
            }
        }
        _ => json!({
            "content": [{"type": "text", "text": format!("Unknown tool: {}", tool_name)}],
            "isError": true,
        }),
    }
}
```

- [ ] **Step 3: Verify all compiles**

Run: `cargo build -p xuanji-mcp-shell`
Expected: SUCCESS

- [ ] **Step 4: Test shell MCP server manually**

Run: `echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}' | cargo run -p xuanji-mcp-shell`
Expected: JSON response with serverInfo

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(plugin): implement shell MCP server"
```

---

## Chunk 4: Agent Loop

### Task 7: Agent Types & Risk Check

**Files:**
- Create: `crates/xuanji-agent/src/types.rs`
- Create: `crates/xuanji-agent/src/error.rs`
- Create: `crates/xuanji-agent/src/risk.rs`
- Test: `crates/xuanji-agent/tests/risk_test.rs`

- [ ] **Step 1: Implement types.rs**

```rust
use serde::{Deserialize, Serialize};
use xuanji_llm::ToolCall;

/// Agent loop configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub max_loops: u32,
    #[serde(default = "default_step_timeout")]
    pub step_timeout: String,
    #[serde(default = "default_true")]
    pub confirm_risky: bool,
    #[serde(default)]
    pub risky_patterns: Vec<RiskyPattern>,
}

fn default_step_timeout() -> String { "60s".into() }
fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskyPattern {
    pub tool: String,
    pub pattern: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_loops: 20,
            step_timeout: default_step_timeout(),
            confirm_risky: true,
            risky_patterns: Vec::new(),
        }
    }
}

/// Result of a single tool execution
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub result: String,
    pub success: bool,
}

/// Sub-task in working memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub description: String,
    pub status: SubTaskStatus,
    pub result_summary: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SubTaskStatus {
    Pending,
    InProgress,
    Done,
    Failed,
    Skipped,
}
```

- [ ] **Step 2: Implement error.rs**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(#[from] xuanji_llm::LlmError),

    #[error("Plugin error: {0}")]
    Plugin(#[from] xuanji_plugin::PluginError),

    #[error("Max loops ({0}) exceeded")]
    MaxLoopsExceeded(u32),

    #[error("Step timeout")]
    StepTimeout,

    #[error("User cancelled")]
    UserCancelled,

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
```

- [ ] **Step 3: Write failing test for risk check**

Create `crates/xuanji-agent/tests/risk_test.rs`:

```rust
use xuanji_agent::risk::RiskChecker;
use xuanji_agent::types::{AgentConfig, RiskyPattern};

fn test_checker() -> RiskChecker {
    let config = AgentConfig {
        risky_patterns: vec![
            RiskyPattern { tool: "shell.run".into(), pattern: "rm\\s+-rf".into() },
            RiskyPattern { tool: "shell.run".pattern: "DROP\\s+".into() },
        ],
        ..Default::default()
    };
    RiskChecker::new(&config)
}

#[test]
fn test_risky_command_detected() {
    let checker = test_checker();
    assert!(checker.is_risky("shell.run", &serde_json::json!({"command": "rm -rf /tmp/test"})));
}

#[test]
fn test_safe_command_passes() {
    let checker = test_checker();
    assert!(!checker.is_risky("shell.run", &serde_json::json!({"command": "ls -la"})));
}

#[test]
fn test_different_tool_not_checked() {
    let checker = test_checker();
    assert!(!checker.is_risky("http.get", &serde_json::json!({"url": "http://example.com"})));
}
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test -p xuanji-agent --test risk_test`
Expected: FAIL - risk module not found

- [ ] **Step 5: Implement risk.rs**

```rust
use crate::types::AgentConfig;
use regex::Regex;

pub struct RiskChecker {
    patterns: Vec<(String, Regex)>,
}

impl RiskChecker {
    pub fn new(config: &AgentConfig) -> Self {
        let patterns = config
            .risky_patterns
            .iter()
            .filter_map(|p| {
                Regex::new(&p.pattern)
                    .ok()
                    .map(|re| (p.tool.clone(), re))
            })
            .collect();

        Self { patterns }
    }

    /// Check if a tool call is considered risky
    pub fn is_risky(&self, tool_name: &str, arguments: &serde_json::Value) -> bool {
        let args_str = arguments.to_string();
        self.patterns
            .iter()
            .any(|(tool, pattern)| tool_name == tool && pattern.is_match(&args_str))
    }
}
```

Add `regex` to xuanji-agent Cargo.toml if not already present (it is).

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test -p xuanji-agent --test risk_test`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(agent): add types, error, and risk pattern checker"
```

---

### Task 8: Memory Modules

**Files:**
- Create: `crates/xuanji-memory/src/types.rs`
- Create: `crates/xuanji-memory/src/short_term.rs`
- Create: `crates/xuanji-memory/src/working.rs`

- [ ] **Step 1: Implement types.rs**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub max_history: usize,
    pub max_context_turns: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_history: 100,
            max_context_turns: 20,
        }
    }
}
```

- [ ] **Step 2: Implement short_term.rs**

```rust
use crate::types::MemoryConfig;
use xuanji_llm::Message;

pub struct ShortTermMemory {
    messages: Vec<Message>,
    max_turns: usize,
}

impl ShortTermMemory {
    pub fn new(config: &MemoryConfig) -> Self {
        Self {
            messages: Vec::new(),
            max_turns: config.max_context_turns,
        }
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
        self.compress_if_needed();
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    pub fn into_messages(self) -> Vec<Message> {
        self.messages
    }

    /// Keep system prompt + first user message + last N turns
    fn compress_if_needed(&mut self) {
        if self.messages.len() <= self.max_turns + 2 {
            return;
        }

        // Always keep first message (system prompt) and first user message
        let preserved_front = 2;
        let keep_from = self.messages.len().saturating_sub(self.max_turns);

        let mut new_messages = Vec::with_capacity(preserved_front + self.max_turns);
        new_messages.extend(self.messages.drain(0..preserved_front));
        new_messages.extend(self.messages.drain(keep_from.saturating_sub(preserved_front)..));

        self.messages = new_messages;
    }
}
```

- [ ] **Step 3: Implement working.rs**

```rust
use crate::types::MemoryConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    pub description: String,
    pub status: SubTaskStatus,
    pub result_summary: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum SubTaskStatus {
    Pending,
    InProgress,
    Done,
    Failed,
}

pub struct WorkingMemory {
    pub goal: String,
    pub subtasks: Vec<SubTask>,
    pub key_results: Vec<String>,
    pub errors: Vec<String>,
}

impl WorkingMemory {
    pub fn new(goal: String) -> Self {
        Self {
            goal,
            subtasks: Vec::new(),
            key_results: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Generate a status summary for injecting into system prompt
    pub fn to_prompt_context(&self) -> String {
        let mut ctx = format!("## 当前任务进度\n目标: {}\n", self.goal);

        for task in &self.subtasks {
            let marker = match task.status {
                SubTaskStatus::Done => "[x]",
                SubTaskStatus::InProgress => "[→]",
                SubTaskStatus::Failed => "[!]",
                SubTaskStatus::Pending => "[ ]",
            };
            let result = task
                .result_summary
                .as_ref()
                .map(|r| format!(" → {}", r))
                .unwrap_or_default();
            ctx.push_str(&format!("- {} {}{}\n", marker, task.description, result));
        }

        if !self.errors.is_empty() {
            ctx.push_str("\n## 错误记录\n");
            for err in &self.errors {
                ctx.push_str(&format!("- {}\n", err));
            }
        }

        ctx
    }
}
```

- [ ] **Step 4: Verify all compiles**

Run: `cargo build -p xuanji-memory`
Expected: SUCCESS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(memory): add short-term and working memory"
```

---

### Task 9: Agent Loop Core

**Files:**
- Create: `crates/xuanji-agent/src/prompt.rs`
- Create: `crates/xuanji-agent/src/context.rs`
- Create: `crates/xuanji-agent/src/agent.rs`

- [ ] **Step 1: Implement prompt.rs (system prompt construction)**

```rust
use xuanji_llm::ToolSchema;
use xuanji_memory::working::WorkingMemory;

pub fn build_system_prompt(
    tools: &[ToolSchema],
    working_memory: Option<&WorkingMemory>,
    memory_context: Option<&str>,
) -> String {
    let mut prompt = String::from(
r#"你是 xuanji，一个自动化任务执行助手。

## 你的工作方式
1. 理解用户目标，将其拆解为可执行的子任务
2. 按顺序或并行调用工具来完成每个子任务
3. 观察工具返回的结果，决定下一步
4. 所有子任务完成后，总结结果

## 规则
- 每次只调用必要的工具，不要做多余操作
- 如果工具调用失败，分析原因并尝试替代方案
- 如果信息不足以完成任务，向用户提问
- 完成后给出简洁的执行总结
"#,
    );

    // Inject available tools
    if !tools.is_empty() {
        prompt.push_str("\n## 可用工具\n");
        for tool in tools {
            prompt.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
        }
    }

    // Inject working memory context
    if let Some(wm) = working_memory {
        prompt.push_str(&format!("\n{}\n", wm.to_prompt_context()));
    }

    // Inject long-term memory context
    if let Some(ctx) = memory_context {
        prompt.push_str(&format!("\n## 项目知识\n{}\n", ctx));
    }

    prompt
}
```

- [ ] **Step 2: Implement context.rs (context compression)**

```rust
use xuanji_llm::Message;

/// Count approximate token count (rough estimate: 1 token ≈ 4 chars)
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|m| match m {
            Message::System { content } => content.len() / 4,
            Message::User { content } => content.len() / 4,
            Message::Assistant { content } => content.len() / 4,
            Message::AssistantToolCalls { calls } => {
                calls.iter().map(|c| c.tool_name.len() / 4 + 50).sum()
            }
            Message::ToolResult { result, .. } => result.len() / 4,
        })
        .sum()
}
```

- [ ] **Step 3: Implement agent.rs (main agent loop)**

```rust
use crate::error::AgentError;
use crate::prompt::build_system_prompt;
use crate::risk::RiskChecker;
use crate::types::{AgentConfig, ToolResult};
use xuanji_llm::{LlmProvider, LlmResponse, Message, ToolCall, ToolSchema};
use xuanji_memory::short_term::ShortTermMemory;
use xuanji_memory::working::WorkingMemory;
use xuanji_memory::MemoryConfig;
use xuanji_plugin::ToolRegistry;
use std::collections::HashMap;

pub struct Agent {
    provider: Box<dyn LlmProvider>,
    registry: ToolRegistry,
    config: AgentConfig,
    risk_checker: RiskChecker,
    memory_config: MemoryConfig,
}

impl Agent {
    pub fn new(
        provider: Box<dyn LlmProvider>,
        registry: ToolRegistry,
        config: AgentConfig,
    ) -> Self {
        let risk_checker = RiskChecker::new(&config);
        Self {
            provider,
            registry,
            config,
            risk_checker,
            memory_config: MemoryConfig::default(),
        }
    }

    /// Run a single-shot agent task
    pub async fn run(&mut self, user_input: String) -> Result<String, AgentError> {
        let tools = self.registry.all_tool_schemas();
        let mut short_term = ShortTermMemory::new(&self.memory_config);
        let mut working = WorkingMemory::new(user_input.clone());

        // Initial system prompt
        let system_prompt = build_system_prompt(&tools, None, None);
        short_term.push(Message::System { content: system_prompt });
        short_term.push(Message::User { content: user_input });

        let mut loop_count = 0;

        loop {
            if loop_count >= self.config.max_loops {
                return Err(AgentError::MaxLoopsExceeded(self.config.max_loops));
            }
            loop_count += 1;

            // Build system prompt with current working memory
            let system_prompt = build_system_prompt(
                &tools,
                Some(&working),
                None,
            );

            // Replace system prompt in messages
            let messages = self.prepare_messages(&short_term, &system_prompt);

            // Call LLM
            tracing::info!("Agent loop iteration {loop_count}");
            let response = self.provider
                .complete(messages, tools.clone(), None, None)
                .await?;

            match response {
                LlmResponse::ToolCalls { calls, text } => {
                    if let Some(t) = &text {
                        tracing::info!("Agent reasoning: {}", t);
                    }

                    // Push assistant tool calls to history
                    short_term.push(Message::AssistantToolCalls { calls: calls.clone() });

                    // Execute all tool calls (collect results)
                    let mut results = Vec::new();
                    for call in &calls {
                        let result = self.execute_tool(call).await?;
                        results.push(result);
                    }

                    // Push all tool results to history
                    for result in &results {
                        short_term.push(Message::ToolResult {
                            tool_call_id: None,
                            tool_name: result.tool_name.clone(),
                            result: result.result.clone(),
                            success: result.success,
                        });
                    }
                }

                LlmResponse::Text { content } => {
                    // Task complete or needs user input
                    return Ok(content);
                }
            }
        }
    }

    /// Execute a single tool call
    async fn execute_tool(&mut self, call: &ToolCall) -> Result<ToolResult, AgentError> {
        tracing::info!("Calling tool: {}({})", call.tool_name, call.arguments);

        // Risk check
        if self.config.confirm_risky
            && self.risk_checker.is_risky(&call.tool_name, &call.arguments)
        {
            tracing::warn!("Risky tool call detected: {}", call.tool_name);
            // For now, log warning and proceed. Interactive confirmation in CLI layer.
        }

        // Check if it's a system tool
        if let Some(entry) = self.registry.get_tool(&call.tool_name) {
            if matches!(entry.source, xuanji_plugin::registry::ToolSource::System { .. }) {
                // System tools handled differently — for MVP, return not implemented
                return Ok(ToolResult {
                    tool_name: call.tool_name.clone(),
                    result: "System tool not yet implemented in MVP".into(),
                    success: false,
                });
            }
        }

        // Execute via MCP
        match self.registry.call_tool(&call.tool_name, call.arguments.clone()).await {
            Ok(mcp_result) => {
                let text = mcp_result
                    .content
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| c.get("text").and_then(|t| t.as_str()).map(String::from))
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default();

                Ok(ToolResult {
                    tool_name: call.tool_name.clone(),
                    result: text,
                    success: !mcp_result.is_error,
                })
            }
            Err(e) => Ok(ToolResult {
                tool_name: call.tool_name.clone(),
                result: format!("Error: {}", e),
                success: false,
            }),
        }
    }

    /// Prepare messages with updated system prompt
    fn prepare_messages(&self, memory: &ShortTermMemory, system_prompt: &str) -> Vec<Message> {
        let mut messages = Vec::new();
        messages.push(Message::System {
            content: system_prompt.to_string(),
        });

        // Skip the first system message from memory, use the fresh one
        for msg in memory.messages().iter().skip(1) {
            messages.push(msg.clone());
        }

        messages
    }
}
```

- [ ] **Step 4: Verify all compiles**

Run: `cargo build -p xuanji-agent`
Expected: SUCCESS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(agent): implement ReAct agent loop with MCP tool execution"
```

---

## Chunk 5: CLI Integration

### Task 10: CLI Entry & Config Loading

**Files:**
- Modify: `crates/xuanji-cli/src/main.rs`
- Create: `crates/xuanji-cli/src/config.rs`
- Create: `crates/xuanji-cli/src/commands/mod.rs`
- Create: `crates/xuanji-cli/src/commands/agent.rs`
- Create: `crates/xuanji-cli/src/commands/mcp.rs`
- Create: `xuanji.toml` (example config)

- [ ] **Step 1: Create config.rs (config loading & merging)**

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use xuanji_agent::types::AgentConfig;
use xuanji_llm::config::{LlmConfig, ProviderConfig};
use xuanji_plugin::types::McpServerConfig;

#[derive(Debug, Deserialize)]
pub struct XuanjiConfig {
    #[serde(default)]
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
                default: String::new(),
                providers: HashMap::new(),
            },
            agent: AgentConfig::default(),
            mcp_servers: Vec::new(),
        }
    }
}

use std::collections::HashMap;

impl XuanjiConfig {
    /// Load config from file discovery: ./xuanji.toml → ~/.xuanji/config.toml
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        // Global config
        let global_path = dirs().join("config.toml");
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
            if let Some(ref key) = provider.api_key {
                if key.starts_with("${") && key.ends_with('}') {
                    let var_name = &key[2..key.len()-1];
                    provider.api_key = std::env::var(var_name).ok();
                }
            }
        }

        Ok(config)
    }

    fn merge(mut self, other: Self) -> Self {
        if !other.llm.default.is_empty() {
            self.llm.default = other.llm.default;
        }
        self.llm.providers.extend(other.llm.providers);
        if !other.mcp_servers.is_empty() {
            self.mcp_servers = other.mcp_servers;
        }
        self
    }
}

fn dirs() -> PathBuf {
    dirs_home().join(".xuanji")
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}
```

Add `dirs` crate to xuanji-cli Cargo.toml:

```toml
dirs = "6"
```

- [ ] **Step 2: Create commands/mod.rs**

```rust
pub mod agent;
pub mod mcp;
```

- [ ] **Step 3: Create commands/agent.rs**

```rust
use anyhow::Result;
use xuanji_agent::Agent;
use xuanji_agent::types::AgentConfig;
use xuanji_llm::config::ProviderConfig;
use xuanji_llm::openai::OpenAiAdapter;
use xuanji_llm::anthropic::AnthropicAdapter;
use xuanji_llm::LlmProvider;
use xuanji_llm::Protocol;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;

pub async fn run_agent(
    prompt: &str,
    provider_name: &str,
    provider_config: &ProviderConfig,
    agent_config: &AgentConfig,
    mcp_servers: &[McpServerConfig],
) -> Result<String> {
    // Create LLM provider
    let provider: Box<dyn LlmProvider> = match provider_config.protocol {
        Protocol::OpenAI => Box::new(OpenAiAdapter::new(
            provider_name.to_string(),
            provider_config.clone(),
        )?),
        Protocol::Anthropic => Box::new(AnthropicAdapter::new(
            provider_name.to_string(),
            provider_config.clone(),
        )?),
        Protocol::Gemini => {
            anyhow::bail!("Gemini protocol not yet implemented");
        }
    };

    // Create tool registry and register MCP servers
    let mut registry = ToolRegistry::new();
    for server_config in mcp_servers {
        registry.register_server(server_config.clone());
    }

    // Load all MCP servers (start processes + discover tools)
    registry.load_all().await?;

    // Create and run agent
    let mut agent = Agent::new(provider, registry, agent_config.clone());
    let result = agent.run(prompt.to_string()).await?;

    Ok(result)
}
```

- [ ] **Step 4: Create commands/mcp.rs**

```rust
use anyhow::Result;
use xuanji_plugin::types::McpServerConfig;
use xuanji_plugin::ToolRegistry;

pub async fn list_tools(mcp_servers: &[McpServerConfig]) -> Result<()> {
    let mut registry = ToolRegistry::new();
    for config in mcp_servers {
        registry.register_server(config.clone());
    }
    registry.load_all().await?;

    let tools = registry.list_tools();
    if tools.is_empty() {
        println!("No MCP tools registered.");
        return Ok(());
    }

    println!("Registered MCP tools:\n");
    for tool in tools {
        println!("  {} - {}", tool.name, tool.description);
    }

    registry.shutdown_all().await;
    Ok(())
}
```

- [ ] **Step 5: Implement main.rs with clap**

```rust
mod commands;
mod config;

use anyhow::Result;
use clap::{Parser, Command};

#[derive(Parser)]
#[command(name = "xuanji")]
#[command(version, about = "AI-driven universal automation platform")]
struct Cli {
    /// Natural language task description (starts agent mode)
    prompt: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Interactive multi-turn agent chat
    Chat,

    /// MCP server management
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Initialize configuration file
    ConfigInit,
}

#[derive(clap::Subcommand)]
enum McpAction {
    /// List registered MCP servers and their tools
    List,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("XUANJI_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let cli = Cli::parse();
    let config = config::XuanjiConfig::load().unwrap_or_default();

    match (cli.prompt, cli.command) {
        // Agent mode: xuanji "task description"
        (Some(prompt), None) => {
            let (provider_name, provider_config) = config
                .llm
                .providers
                .get(&config.llm.default)
                .map(|c| (config.llm.default.clone(), c.clone()))
                .ok_or_else(|| anyhow::anyhow!(
                    "Default provider '{}' not found in config. Run 'xuanji config-init' to create a config.",
                    config.llm.default
                ))?;

            let result = commands::agent::run_agent(
                &prompt,
                &provider_name,
                &provider_config,
                &config.agent,
                &config.mcp_servers,
            ).await?;

            println!("{}", result);
        }

        // Chat mode: xuanji chat
        (None, Some(Commands::Chat)) => {
            println!("xuanji chat mode - not yet implemented in MVP");
            // TODO: interactive REPL
        }

        // MCP list
        (None, Some(Commands::Mcp { action: McpAction::List })) => {
            commands::mcp::list_tools(&config.mcp_servers).await?;
        }

        // Config init
        (None, Some(Commands::ConfigInit)) => {
            let example = r#"[llm]
default = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
base_url = "https://api.deepseek.com/v1"
api_key = "${DEEPSEEK_API_KEY}"
model = "deepseek-chat"

[agent]
max_loops = 20
confirm_risky = true

[[mcp_server]]
name = "shell"
command = "xuanji-mcp-shell"
"#;
            std::fs::write("xuanji.toml", example)?;
            println!("Created xuanji.toml");
        }

        // No args - show help
        _ => {
            Cli::parse_from(["xuanji", "--help"]);
        }
    }

    Ok(())
}
```

- [ ] **Step 6: Create example xuanji.toml**

Create `xuanji.toml` at project root (example, gitignored for real use):

```toml
[llm]
default = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
base_url = "https://api.deepseek.com/v1"
api_key = "${DEEPSEEK_API_KEY}"
model = "deepseek-chat"

[agent]
max_loops = 20
confirm_risky = true

[[mcp_server]]
name = "shell"
command = "xuanji-mcp-shell"
```

- [ ] **Step 7: Verify all compiles**

Run: `cargo build`
Expected: SUCCESS

- [ ] **Step 8: Test CLI help**

Run: `cargo run -- --help`
Expected: Shows help text with xuanji, chat, mcp, config-init commands

- [ ] **Step 9: Test config init**

Run: `cargo run -- config-init && cat xuanji.toml`
Expected: Creates xuanji.toml with example config

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(cli): implement CLI with agent mode, MCP list, and config management"
```

---

### Task 11: End-to-End Integration Test

**Files:**
- Create: `tests/integration/agent_e2e.rs`

- [ ] **Step 1: Write E2E test that runs agent with shell MCP server**

Create `tests/integration/agent_e2e.rs`:

```rust
use std::collections::HashMap;
use xuanji_agent::Agent;
use xuanji_agent::types::AgentConfig;
use xuanji_llm::openai::OpenAiAdapter;
use xuanji_llm::config::ProviderConfig;
use xuanji_llm::Protocol;
use xuanji_plugin::ToolRegistry;
use xuanji_plugin::types::McpServerConfig;

// NOTE: This test requires a real LLM API key to run.
// Run with: DEEPSEEK_API_KEY=xxx cargo test --test agent_e2e -- --ignored
#[tokio::test]
#[ignore]
async fn test_agent_with_shell_tool() {
    let api_key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY not set");

    let provider_config = ProviderConfig {
        protocol: Protocol::OpenAI,
        base_url: "https://api.deepseek.com/v1".into(),
        api_key: Some(api_key),
        model: "deepseek-chat".into(),
        timeout: Some("120s".into()),
        max_tokens: Some(4096),
        temperature: Some(0.3),
    };

    let provider = OpenAiAdapter::new("deepseek".into(), provider_config).unwrap();

    let mut registry = ToolRegistry::new();
    registry.register_server(McpServerConfig {
        name: "shell".into(),
        command: "xuanji-mcp-shell".into(),
        args: vec![],
        env: HashMap::new(),
    });

    let config = AgentConfig {
        max_loops: 5,
        ..Default::default()
    };

    let mut agent = Agent::new(Box::new(provider), registry, config);
    let result = agent
        .run("列出当前目录下的文件".to_string())
        .await
        .expect("Agent failed");

    println!("Agent result:\n{}", result);
    assert!(!result.is_empty());
}
```

- [ ] **Step 2: Verify test compiles**

Run: `cargo test --test agent_e2e --no-run`
Expected: SUCCESS (compiles but doesn't run, marked #[ignore])

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "test: add end-to-end agent integration test"
```

---

### Task 12: Final Polish & Documentation

**Files:**
- Create: `README.md`
- Modify: `Cargo.toml` (add workspace metadata)

- [ ] **Step 1: Create README.md**

```markdown
# xuanji (璇玑)

AI 驱动的通用自动化平台 CLI 工具。

## 安装

```bash
cargo build --release
# 二进制文件在 target/release/xuanji
```

## 快速开始

```bash
# 初始化配置
xuanji config-init

# 设置 API Key
export DEEPSEEK_API_KEY=your-key-here

# 运行 Agent
xuanji "列出当前目录下的文件"

# 查看 MCP 工具
xuanji mcp list
```

## 配置

编辑 `xuanji.toml`：

```toml
[llm]
default = "deepseek"

[llm.providers.deepseek]
protocol = "openai"
base_url = "https://api.deepseek.com/v1"
api_key = "${DEEPSEEK_API_KEY}"
model = "deepseek-chat"

[[mcp_server]]
name = "shell"
command = "xuanji-mcp-shell"
```

## 架构

```
xuanji-cli → xuanji-agent → xuanji-llm (LLM 调用)
                       → xuanji-plugin (MCP 工具)
                       → xuanji-memory (上下文管理)
```

## License

MIT
```

- [ ] **Step 2: Verify everything compiles and tests pass**

Run: `cargo build && cargo test`
Expected: All crates compile, unit tests pass (E2E test skipped due to #[ignore])

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "docs: add README and finalize MVP"
```
