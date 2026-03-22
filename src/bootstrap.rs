use std::sync::Arc;
use async_trait::async_trait;
use electro_core::config::credentials::load_credentials_file;
use electro_core::config::credentials::is_placeholder_key;
use electro_core::Channel;

/// Wraps any Channel to censor known API keys from outbound messages.
/// This is the hardcoded last-line-of-defense filter — the system prompt
/// tells the agent not to leak secrets, but this catches anything that slips.
pub struct SecretCensorChannel {
    pub inner: Arc<dyn Channel>,
}

#[async_trait]
impl Channel for SecretCensorChannel {
    fn name(&self) -> &str {
        self.inner.name()
    }
    async fn start(&mut self) -> std::result::Result<(), electro_core::types::error::ElectroError> {
        Ok(())
    }
    async fn stop(&mut self) -> std::result::Result<(), electro_core::types::error::ElectroError> {
        Ok(())
    }
    async fn send_message(
        &self,
        mut msg: electro_core::types::message::OutboundMessage,
    ) -> std::result::Result<(), electro_core::types::error::ElectroError> {
        msg.text = censor_secrets(&msg.text);
        self.inner.send_message(msg).await
    }
    fn file_transfer(&self) -> Option<&dyn electro_core::FileTransfer> {
        self.inner.file_transfer()
    }
    fn is_allowed(&self, user_id: &str) -> bool {
        self.inner.is_allowed(user_id)
    }
    async fn delete_message(
        &self,
        chat_id: &str,
        message_id: &str,
    ) -> std::result::Result<(), electro_core::types::error::ElectroError> {
        self.inner.delete_message(chat_id, message_id).await
    }
}

/// Hardcoded output filter: replaces any known API key in the text with [REDACTED].
/// This is the last line of defense — the system prompt tells the agent not to leak
/// secrets, but this filter catches any that slip through.
pub fn censor_secrets(text: &str) -> String {
    let creds = match load_credentials_file() {
        Some(c) => c,
        None => return text.to_string(),
    };
    let mut censored = text.to_string();
    for provider in &creds.providers {
        for key in &provider.keys {
            if !key.is_empty() && !is_placeholder_key(key) && key.len() >= 8 {
                censored = censored.replace(key, "[REDACTED]");
            }
        }
    }
    censored
}
