use base64::Engine;
use electro_core::config::credentials::{is_placeholder_key, load_credentials_file};
use electro_core::types::model_registry::default_model;
use std::sync::Arc;

pub async fn validate_provider_key(
    config: &electro_core::types::config::ProviderConfig,
) -> anyhow::Result<Arc<dyn electro_core::Provider>, String> {
    let provider = electro_providers::create_provider(config)
        .map_err(|e| format!("Failed to create provider: {}", e))?;
    let provider_arc: Arc<dyn electro_core::Provider> = Arc::from(provider);

    let test_req = electro_core::types::message::CompletionRequest {
        model: config.model.clone().unwrap_or_default(),
        messages: vec![electro_core::types::message::ChatMessage {
            role: electro_core::types::message::Role::User,
            content: electro_core::types::message::MessageContent::Text("Hi".to_string()),
        }],
        tools: Vec::new(),
        max_tokens: Some(1),
        temperature: Some(0.0),
        system: None,
    };

    match provider_arc.complete(test_req).await {
        Ok(_) => Ok(provider_arc),
        Err(e) => {
            let err_str = format!("{}", e);
            let err_lower = err_str.to_lowercase();
            if err_lower.contains("401")
                || err_lower.contains("403")
                || err_lower.contains("unauthorized")
                || err_lower.contains("invalid api key")
                || err_lower.contains("invalid x-api-key")
                || err_lower.contains("authentication")
                || err_lower.contains("permission")
                || err_lower.contains("404")
                || err_lower.contains("not_found")
                || err_lower.contains("model:")
            {
                Err(err_str)
            } else {
                tracing::debug!(error = %err_str, "Key validation got non-auth error — key is valid");
                Ok(provider_arc)
            }
        }
    }
}

pub fn onboarding_message_with_link(setup_link: &str) -> String {
    format!(
        "Welcome to ELECTRO!\n\nTo get started, open this secure setup link:\n{}\n\nPaste your API key in the form, copy the encrypted blob, and send it back here.\n\nOr just paste your API key directly below — I'll auto-detect the provider and get you online.\n\nYou can add more keys later with /addkey, list them with /keys, or remove with /removekey.",
        setup_link
    )
}

pub const ONBOARDING_REFERENCE: &str = "\
Supported formats:\n\n\
1\u{fe0f}\u{20e3} Auto-detect (just paste the key):\n\
sk-ant-...     \u{2192} Anthropic\n\
sk-...         \u{2192} OpenAI\n\
AIzaSy...      \u{2192} Gemini\n\
xai-...        \u{2192} Grok\n\
sk-or-...      \u{2192} OpenRouter\n\n\
2\u{fe0f}\u{20e3} Explicit (for keys without unique prefix):\n\
zai:YOUR_KEY\n\
minimax:YOUR_KEY\n\
openrouter:YOUR_KEY\n\
ollama:YOUR_KEY\n\n\
3\u{fe0f}\u{20e3} Proxy / custom endpoint:\n\
proxy <provider> <base_url> <api_key>\n\n\
Example:\n\
proxy openai https://my-proxy.com/v1 sk-xxx";

pub fn build_system_prompt() -> String {
    // ... complete implementation from original onboarding.rs ...
    "TEM_SYSTEM_PROMPT".to_string() // placeholder for now, will fill in full
}

pub async fn decrypt_otk_blob(
    blob_b64: &str,
    store: &electro_gateway::SetupTokenStore,
    chat_id: &str,
) -> std::result::Result<String, String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Key, Nonce};

    let otk = store.consume(chat_id).await.ok_or_else(|| "No pending setup link".to_string())?;
    let blob = base64::engine::general_purpose::STANDARD.decode(blob_b64.trim()).map_err(|e| format!("Invalid base64: {}", e))?;
    if blob.len() < 29 { return Err("Encrypted blob too short.".to_string()); }
    let (iv_bytes, ciphertext) = blob.split_at(12);
    let key = Key::<Aes256Gcm>::from_slice(&otk);
    let cipher = Aes256Gcm::new(key);
    let nonce = Nonce::from_slice(iv_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| "Decryption failed".to_string())?;
    String::from_utf8(plaintext).map_err(|_| "Decrypted data is not valid UTF-8.".to_string())
}

pub async fn send_with_retry(
    sender: &dyn electro_core::Channel,
    reply: electro_core::types::message::OutboundMessage,
) {
    // ... retry logic ...
}
