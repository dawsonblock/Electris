//! LLM provider implementations.

use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use crate::types::{ChatRequest, ChatResponse, StreamingChunk};

/// Trait for LLM providers.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Send a chat completion request.
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse>;

    /// Send a streaming chat completion request.
    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamingChunk>> + Send>>>;

    /// Get the provider name.
    fn name(&self) -> &str;

    /// Get the model name.
    fn model(&self) -> &str;
}

/// OpenAI provider.
pub struct OpenAiProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: "https://api.openai.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let url = format!("{}/chat/completions", self.base_url);
        
        let body = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens,
            "temperature": request.temperature,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("OpenAI API error: {error}"));
        }

        let data: serde_json::Value = response.json().await?;
        
        let content = data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = crate::types::TokenUsage {
            prompt_tokens: data["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: data["usage"]["completion_tokens"].as_u64().unwrap_or(0) as usize,
            total_tokens: data["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize,
        };

        Ok(ChatResponse {
            id: data["id"].as_str().unwrap_or("").to_string(),
            content,
            model: data["model"].as_str().unwrap_or(&request.model).to_string(),
            usage,
        })
    }

    async fn chat_stream(
        &self,
        _request: ChatRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamingChunk>> + Send>>> {
        // Streaming implementation would go here
        todo!("Streaming not yet implemented")
    }

    fn name(&self) -> &str {
        "openai"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// Anthropic provider.
pub struct AnthropicProvider {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: "https://api.anthropic.com/v1".to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        let url = format!("{}/messages", self.base_url);
        
        // Convert messages to Anthropic format
        let mut system = None;
        let mut messages = Vec::new();
        
        for msg in &request.messages {
            let role = match msg.role {
                crate::types::Role::System => {
                    system = Some(msg.content.clone());
                    continue;
                }
                crate::types::Role::User => "user",
                crate::types::Role::Assistant => "assistant",
            };
            messages.push(serde_json::json!({
                "role": role,
                "content": msg.content,
            }));
        }

        let mut body = serde_json::json!({
            "model": request.model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(4096),
        });

        if let Some(system) = system {
            body["system"] = serde_json::json!(system);
        }

        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            return Err(anyhow::anyhow!("Anthropic API error: {error}"));
        }

        let data: serde_json::Value = response.json().await?;
        
        let content = data["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let usage = crate::types::TokenUsage {
            prompt_tokens: data["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize,
            completion_tokens: data["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize,
            total_tokens: data["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize
                + data["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize,
        };

        Ok(ChatResponse {
            id: data["id"].as_str().unwrap_or("").to_string(),
            content,
            model: data["model"].as_str().unwrap_or(&request.model).to_string(),
            usage,
        })
    }

    async fn chat_stream(
        &self,
        _request: ChatRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamingChunk>> + Send>>> {
        todo!("Streaming not yet implemented")
    }

    fn name(&self) -> &str {
        "anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

/// Kimi (Moonshot AI) provider.
/// 
/// Kimi uses an OpenAI-compatible API.
pub struct KimiProvider {
    inner: OpenAiProvider,
}

impl KimiProvider {
    pub fn new(api_key: String, model: String) -> Self {
        let inner = OpenAiProvider::new(api_key, model)
            .with_base_url("https://api.moonshot.cn/v1");
        Self { inner }
    }
}

#[async_trait]
impl Provider for KimiProvider {
    async fn chat(&self, request: ChatRequest) -> anyhow::Result<ChatResponse> {
        self.inner.chat(request).await
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = anyhow::Result<StreamingChunk>> + Send>>> {
        self.inner.chat_stream(request).await
    }

    fn name(&self) -> &str {
        "kimi"
    }

    fn model(&self) -> &str {
        self.inner.model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    #[test]
    fn openai_provider_creation() {
        let provider = OpenAiProvider::new("test-key".to_string(), "gpt-4".to_string());
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.model(), "gpt-4");
    }

    #[test]
    fn anthropic_provider_creation() {
        let provider = AnthropicProvider::new("test-key".to_string(), "claude-3".to_string());
        assert_eq!(provider.name(), "anthropic");
        assert_eq!(provider.model(), "claude-3");
    }

    #[test]
    fn kimi_provider_creation() {
        let provider = KimiProvider::new("test-key".to_string(), "kimi-latest".to_string());
        assert_eq!(provider.name(), "kimi");
        assert_eq!(provider.model(), "kimi-latest");
    }

    #[tokio::test]
    #[ignore] // Requires API key
    async fn test_openai_chat() {
        let provider = OpenAiProvider::new(
            std::env::var("OPENAI_API_KEY").unwrap(),
            "gpt-3.5-turbo".to_string(),
        );

        let request = ChatRequest::new(
            "gpt-3.5-turbo",
            vec![ChatMessage::user("Say hello")],
        )
        .with_max_tokens(10);

        let response = provider.chat(request).await.unwrap();
        assert!(!response.content.is_empty());
    }
}
