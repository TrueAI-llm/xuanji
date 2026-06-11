use async_trait::async_trait;
use std::sync::Arc;

use crate::config::ProviderConfig;
use crate::error::LlmError;
use crate::types::{LlmResponse, Message, ToolSchema};

/// A trait representing an LLM provider that can complete requests.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a completion request to the LLM and return the response.
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolSchema],
    ) -> Result<LlmResponse, LlmError>;

    /// Return the provider configuration.
    fn config(&self) -> &ProviderConfig;
}

/// Wrapper that lets an `Arc<dyn LlmProvider>` be used where a `Box<dyn LlmProvider>`
/// is required. This lets one provider configuration back many agents (and orchestration
/// calls) without re-creating HTTP clients.
pub struct ArcProvider(pub Arc<dyn LlmProvider>);

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
