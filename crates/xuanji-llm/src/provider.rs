use async_trait::async_trait;

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
