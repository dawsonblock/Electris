//! LLM provider integration for the spine runtime.
//!
//! Supports multiple providers: OpenAI, Anthropic, local models.

mod client;
mod providers;
mod types;

pub use client::LlmClient;
pub use providers::{OpenAiProvider, AnthropicProvider, KimiProvider, Provider};
pub use types::{ChatMessage, ChatRequest, ChatResponse, Role, StreamingChunk};

use std::sync::Arc;

/// Factory for creating LLM providers.
pub fn create_provider(
    provider_name: &str,
    api_key: String,
    model: String,
) -> anyhow::Result<Arc<dyn Provider>> {
    match provider_name {
        "openai" => Ok(Arc::new(OpenAiProvider::new(api_key, model))),
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(api_key, model))),
        "kimi" => Ok(Arc::new(KimiProvider::new(api_key, model))),
        _ => Err(anyhow::anyhow!("Unknown provider: {provider_name}")),
    }
}
