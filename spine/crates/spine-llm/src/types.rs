//! Types for LLM interactions.

use serde::{Deserialize, Serialize};

/// Role in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Role {
    System,
    #[default]
    User,
    Assistant,
}


/// A message in the chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
        }
    }
}

/// A chat completion request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
}

impl ChatRequest {
    pub fn new(model: impl Into<String>, messages: Vec<ChatMessage>) -> Self {
        Self {
            model: model.into(),
            messages,
            max_tokens: None,
            temperature: None,
            stream: None,
        }
    }

    pub fn with_max_tokens(mut self, tokens: usize) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn with_streaming(mut self) -> Self {
        self.stream = Some(true);
        self
    }
}

/// A chat completion response.
#[derive(Debug, Clone, Deserialize)]
pub struct ChatResponse {
    pub id: String,
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
}

/// Token usage information.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// A chunk from a streaming response.
#[derive(Debug, Clone, Deserialize)]
pub struct StreamingChunk {
    pub id: String,
    pub content: String,
    pub finish_reason: Option<String>,
}

impl StreamingChunk {
    pub fn is_finished(&self) -> bool {
        self.finish_reason.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_creation() {
        let system = ChatMessage::system("You are a helpful assistant");
        assert_eq!(system.role, Role::System);

        let user = ChatMessage::user("Hello!");
        assert_eq!(user.role, Role::User);

        let assistant = ChatMessage::assistant("Hi there!");
        assert_eq!(assistant.role, Role::Assistant);
    }

    #[test]
    fn chat_request_builder() {
        let request = ChatRequest::new("gpt-4", vec![])
            .with_max_tokens(100)
            .with_temperature(0.5);

        assert_eq!(request.max_tokens, Some(100));
        assert_eq!(request.temperature, Some(0.5));
    }

    #[test]
    fn streaming_chunk_finish_detection() {
        let ongoing = StreamingChunk {
            id: "1".to_string(),
            content: "Hello".to_string(),
            finish_reason: None,
        };
        assert!(!ongoing.is_finished());

        let finished = StreamingChunk {
            id: "2".to_string(),
            content: "Done".to_string(),
            finish_reason: Some("stop".to_string()),
        };
        assert!(finished.is_finished());
    }
}
