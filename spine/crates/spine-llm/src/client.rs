//! High-level LLM client.

use std::sync::Arc;

use crate::providers::Provider;
use crate::types::{ChatMessage, ChatRequest};

/// A high-level client for LLM interactions.
pub struct LlmClient {
    provider: Arc<dyn Provider>,
    default_model: String,
    system_prompt: Option<String>,
    conversation_history: Vec<ChatMessage>,
}

impl LlmClient {
    /// Create a new LLM client.
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        let model = provider.model().to_string();
        Self {
            provider,
            default_model: model,
            system_prompt: None,
            conversation_history: Vec::new(),
        }
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Send a single message (stateless).
    pub async fn complete(&self, prompt: impl Into<String>) -> anyhow::Result<String> {
        let mut messages = Vec::new();
        
        if let Some(ref system) = self.system_prompt {
            messages.push(ChatMessage::system(system.clone()));
        }
        
        messages.push(ChatMessage::user(prompt));

        let request = ChatRequest::new(&self.default_model, messages);
        let response = self.provider.chat(request).await?;
        
        Ok(response.content)
    }

    /// Start a conversation with multiple turns.
    pub async fn chat(&mut self, user_message: impl Into<String>) -> anyhow::Result<String> {
        let user_msg = ChatMessage::user(user_message);
        self.conversation_history.push(user_msg);

        let mut messages = Vec::new();
        
        if let Some(ref system) = self.system_prompt {
            messages.push(ChatMessage::system(system.clone()));
        }
        
        messages.extend(self.conversation_history.clone());

        let request = ChatRequest::new(&self.default_model, messages);
        let response = self.provider.chat(request).await?;

        // Add assistant response to history
        self.conversation_history.push(ChatMessage::assistant(&response.content));

        Ok(response.content)
    }

    /// Clear conversation history.
    pub fn clear_history(&mut self) {
        self.conversation_history.clear();
    }

    /// Get conversation history.
    pub fn history(&self) -> &[ChatMessage] {
        &self.conversation_history
    }

    /// Generate a plan from an intent description.
    pub async fn plan(&self, intent_description: &str) -> anyhow::Result<Vec<String>> {
        let prompt = format!(
            r#"Given this user request: "{intent_description}"

Break this down into a list of specific, actionable steps.
Each step should be a single action like:
- read_file:path=/path/to/file
- write_file:path=/path/to/file,content=<content>
- shell:command=<command>
- git:subcommand=<cmd>

Return ONLY the list of steps, one per line, in the format "action:arg1=value1,arg2=value2"
Do not include any other text."#
        );

        let response = self.complete(prompt).await?;
        
        // Parse response into steps
        let steps: Vec<String> = response
            .lines()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && !s.starts_with("#"))
            .map(|s| s.to_string())
            .collect();

        Ok(steps)
    }

    /// Analyze code and provide suggestions.
    pub async fn analyze_code(&self, code: &str, language: &str) -> anyhow::Result<String> {
        let prompt = format!(
            r#"Analyze this {language} code and provide suggestions for improvement:

```
{code}
```

Focus on:
1. Potential bugs or issues
2. Code style improvements
3. Performance optimizations
4. Best practices
"#
        );

        self.complete(prompt).await
    }

    /// Generate code based on a description.
    pub async fn generate_code(&self, description: &str, language: &str) -> anyhow::Result<String> {
        let prompt = format!(
            r#"Generate {language} code for the following:

{description}

Provide only the code, no explanation. Use proper formatting."#
        );

        self.complete(prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::OpenAiProvider;

    fn create_test_client() -> LlmClient {
        let provider = Arc::new(OpenAiProvider::new(
            "test-key".to_string(),
            "gpt-4".to_string(),
        ));
        LlmClient::new(provider)
    }

    #[test]
    fn client_creation() {
        let client = create_test_client();
        assert!(client.system_prompt.is_none());
        assert!(client.history().is_empty());
    }

    #[test]
    fn client_with_system_prompt() {
        let client = create_test_client()
            .with_system_prompt("You are a coding assistant");
        
        assert_eq!(client.system_prompt, Some("You are a coding assistant".to_string()));
    }

    #[test]
    fn conversation_history_tracking() {
        let mut client = create_test_client();
        
        // Note: We can't test actual chat without API key,
        // but we can verify the history structure
        assert!(client.history().is_empty());
        
        client.conversation_history.push(ChatMessage::user("Hello"));
        client.conversation_history.push(ChatMessage::assistant("Hi!"));
        
        assert_eq!(client.history().len(), 2);
    }

    #[test]
    fn clear_history_works() {
        let mut client = create_test_client();
        client.conversation_history.push(ChatMessage::user("Test"));
        assert_eq!(client.history().len(), 1);
        
        client.clear_history();
        assert!(client.history().is_empty());
    }
}
