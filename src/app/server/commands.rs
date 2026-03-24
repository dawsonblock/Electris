use crate::app::cli::{handle_model_command, list_configured_providers, remove_provider};
use crate::app::onboarding::decrypt_otk_blob;
use electro_core::types::message::{ChatMessage, InboundMessage};
use electro_core::{Channel, Memory, Tool, Vault};
use electro_runtime::{OutboundEvent, RuntimeHandle};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Helper to emit an outbound event for slash command responses.
/// This follows the unified output model: events only, no direct channel calls.
fn emit_response(runtime: &RuntimeHandle, request_id: &str, text: impl Into<String>) {
    let _ = runtime.emit_outbound_event(OutboundEvent::Completed {
        request_id: request_id.to_string(),
        content: text.into(),
    });
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_slash_command(
    msg: &InboundMessage,
    _sender: &Arc<dyn Channel>,
    runtime: &RuntimeHandle,
    memory: &Arc<dyn Memory>,
    _history: &[ChatMessage],
    _tools_template: &[Arc<dyn Tool>],
    setup_tokens: &electro_gateway::SetupTokenStore,
    _pending_raw_keys: &Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")] _login_sessions: &Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    #[cfg(feature = "browser")] _browser_ref: &Option<Arc<electro_tools::BrowserTool>>,
    _vault: &Option<Arc<dyn Vault>>,
    _personality_locked: bool,
) -> bool {
    let text = msg.text.as_deref().unwrap_or_default().trim();
    if !text.starts_with('/') && !text.starts_with("enc:v1:") {
        return false;
    }

    let chat_id = msg.chat_id.clone();
    let msg_id = msg.id.clone();

    // Handle encrypted blobs from onboarding flow
    if text.starts_with("enc:v1:") {
        let blob_b64 = &text["enc:v1:".len()..];
        match decrypt_otk_blob(blob_b64, setup_tokens, &chat_id).await {
            Ok(_key_text) => {
                // Emit event instead of direct send
                emit_response(runtime, &msg_id, "Key received and validated.");
            }
            Err(e) => {
                emit_response(runtime, &msg_id, format!("Error: {}", e));
            }
        }
        return true;
    }

    let parts: Vec<&str> = text.split_whitespace().collect();
    let cmd = parts[0].to_lowercase();
    let args = parts[1..].join(" ");

    match cmd.as_str() {
        "/help" => {
            let help = "Available commands:\n/help - Show this help\n/model - Switch model\n/keys - List configured keys\n/addkey - Add a new API key\n/removekey - Remove an API key\n/stop - Stop active task\n/reset - Reset current chat history";
            emit_response(runtime, &msg_id, help);
            true
        }
        "/model" => {
            let args_vec = parts[1..]
                .iter()
                .map(|part| (*part).to_string())
                .collect::<Vec<_>>();
            let resp = match handle_model_command(runtime.clone(), &args_vec).await {
                Ok(resp) => resp,
                Err(error) => format!("Error: {error}"),
            };
            emit_response(runtime, &msg_id, resp);
            true
        }
        "/keys" => {
            let resp = list_configured_providers();
            emit_response(runtime, &msg_id, resp);
            true
        }
        "/removekey" => {
            let resp = remove_provider(&args);
            emit_response(runtime, &msg_id, resp);
            true
        }
        "/reset" => {
            let history_key = format!("chat_history:{}", chat_id);
            let _ = memory.delete(&history_key).await;
            emit_response(runtime, &msg_id, "Chat history reset.");
            true
        }
        "/stop" => {
            // Handled by dispatcher interruption logic, but we can confirm
            emit_response(runtime, &msg_id, "Task stopped.");
            true
        }
        _ => false,
    }
}
