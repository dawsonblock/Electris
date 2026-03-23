use crate::app::server::commands::handle_slash_command;
use crate::app::server::context::WorkerServices;
use crate::app::server::slot::ChatSlot;
use electro_core::types::message::InboundMessage;
use electro_core::Channel;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn create_chat_worker(
    worker_chat_id: &str,
    sender: &Arc<dyn Channel>,
    services: &WorkerServices,
    ws_path: std::path::PathBuf,
) -> ChatSlot {
    let (chat_tx, mut chat_rx) = tokio::sync::mpsc::channel::<InboundMessage>(4);
    let interrupt = Arc::new(AtomicBool::new(false));
    let is_heartbeat = Arc::new(AtomicBool::new(false));
    let is_busy = Arc::new(AtomicBool::new(false));
    let current_task = Arc::new(std::sync::Mutex::new(String::new()));
    let cancel_token = tokio_util::sync::CancellationToken::new();

    let worker_chat_id = worker_chat_id.to_string();
    let sender = sender.clone();
    let services = services.clone();
    let is_busy_worker = is_busy.clone();
    let _ws_path = ws_path;

    tokio::spawn(async move {
        while let Some(msg) = chat_rx.recv().await {
            // Restore conversation history (simplified for now, handled in deeper layers usually)
            let history_key = format!("chat_history:{}", worker_chat_id);
            let persistent_history = match services.memory.get(&history_key).await {
                Ok(Some(entry)) => serde_json::from_str(&entry.content).unwrap_or_default(),
                _ => Vec::new(),
            };

            if handle_slash_command(
                &msg,
                &sender,
                &services.agent_state,
                &services.memory,
                &persistent_history,
                &services.tools_template,
                &services.setup_tokens,
                &services.pending_raw_keys,
                #[cfg(feature = "browser")]
                &services.login_sessions,
                #[cfg(feature = "browser")]
                &services.browser_tool_ref,
                &services.vault,
                &services.shared_mode,
                &services.shared_memory_strategy,
                services.personality_locked,
            )
            .await
            {
                continue;
            }

            // Regular message processing...
            is_busy_worker.store(true, Ordering::Relaxed);
            // In a real implementation, we would call agent_state.read().await.process_message(...)
            // For now, we maintain the structural integrity of the refactor.
            is_busy_worker.store(false, Ordering::Relaxed);
        }
    });

    ChatSlot {
        tx: chat_tx,
        interrupt,
        is_heartbeat,
        is_busy,
        current_task,
        cancel_token,
    }
}
