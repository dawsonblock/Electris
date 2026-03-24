use crate::types::error::ElectroError;
use crate::types::message::{CompletionRequest, CompletionResponse, StreamChunk};
use async_trait::async_trait;
use futures::stream::BoxStream;

/// AI model provider trait. Implement this for each AI backend (Anthropic, OpenAI, etc.)
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g., "anthropic", "openai-compatible")
    fn name(&self) -> &str;

    /// Send a completion request and get a full response
    async fn complete(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse, ElectroError>;

    /// Send a completion request and get a streaming response
    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<BoxStream<'_, Result<StreamChunk, ElectroError>>, ElectroError>;

    /// Check if the provider is healthy and reachable
    async fn health_check(&self) -> Result<bool, ElectroError>;

    /// List available models for this provider
    async fn list_models(&self) -> Result<Vec<String>, ElectroError>;
}
