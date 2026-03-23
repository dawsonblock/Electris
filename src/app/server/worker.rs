use crate::app::server::commands::handle_slash_command;
use crate::app::server::slot::ChatSlot;
use electro_core::types::message::{ChatMessage, InboundMessage, OutboundMessage};
use electro_core::types::session::SessionContext;
use electro_core::{Channel, Memory, Tool, UsageStore, Vault};
use electro_runtime::RuntimeHandle;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

#[allow(clippy::too_many_arguments)]
pub fn create_chat_worker(
    worker_chat_id: &str,
    sender: &Arc<dyn Channel>,
    runtime: &RuntimeHandle,
    memory: &Arc<dyn Memory>,
    tools_template: &[Arc<dyn Tool>],
    custom_registry: &Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")] mcp_mgr: &Arc<electro_mcp::McpManager>,
    max_turns: usize,
    max_ctx: usize,
    max_rounds: usize,
    max_task_duration: u64,
    max_spend: f64,
    v2_opt: bool,
    pp_opt: bool,
    base_url: &Option<String>,
    ws_path: &std::path::Path,
    pending_clone: &electro_tools::PendingMessages,
    setup_tokens_clone: &electro_gateway::SetupTokenStore,
    pending_raw_keys_clone: &Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")] login_sessions_clone: &Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    usage_store_clone: &Arc<dyn UsageStore>,
    hive_clone: &Option<Arc<electro_hive::Hive>>,
    personality_locked: bool,
    #[cfg(feature = "browser")] browser_ref_worker: &Option<Arc<electro_tools::BrowserTool>>,
    vault: &Option<Arc<dyn Vault>>,
) -> ChatSlot {
    let (chat_tx, mut chat_rx) = tokio::sync::mpsc::channel::<InboundMessage>(4);
    let interrupt = Arc::new(AtomicBool::new(false));
    let is_heartbeat = Arc::new(AtomicBool::new(false));
    let is_busy = Arc::new(AtomicBool::new(false));
    let current_task = Arc::new(std::sync::Mutex::new(String::new()));
    let cancel_token = tokio_util::sync::CancellationToken::new();

    let worker_chat_id = worker_chat_id.to_string();
    let sender = sender.clone();
    let memory = memory.clone();
    let runtime = runtime.clone();
    let is_busy_worker = is_busy.clone();
    let current_task_worker = current_task.clone();
    let interrupt_worker = interrupt.clone();
    let is_heartbeat_worker = is_heartbeat.clone();
    let cancel_token_worker = cancel_token.clone();
    let tools_template = tools_template.to_vec();
    let ws_path = ws_path.to_path_buf();
    let pending_messages = pending_clone.clone();
    let setup_tokens = setup_tokens_clone.clone();
    let pending_raw_keys = pending_raw_keys_clone.clone();
    #[cfg(feature = "browser")]
    let login_sessions = login_sessions_clone.clone();
    let _usage_store = usage_store_clone.clone();
    let _hive = hive_clone.clone();
    #[cfg(feature = "browser")]
    let browser_ref = browser_ref_worker.clone();
    let vault = vault.clone();

    tokio::spawn(async move {
        // Restore conversation history
        let history_key = format!("chat_history:{}", worker_chat_id);
        let mut persistent_history: Vec<ChatMessage> = match memory.get(&history_key).await {
            Ok(Some(entry)) => serde_json::from_str(&entry.content).unwrap_or_default(),
            _ => Vec::new(),
        };

        while let Some(msg) = chat_rx.recv().await {
            is_heartbeat_worker.store(msg.channel == "heartbeat", Ordering::Relaxed);
            if handle_slash_command(
                &msg,
                &sender,
                &runtime,
                &memory,
                &persistent_history,
                &tools_template,
                &setup_tokens,
                &pending_raw_keys,
                #[cfg(feature = "browser")]
                &login_sessions,
                #[cfg(feature = "browser")]
                &browser_ref,
                &vault,
                personality_locked,
            )
            .await
            {
                is_heartbeat_worker.store(false, Ordering::Relaxed);
                continue;
            }

            let Some(agent) = runtime.agent().await else {
                let _ = sender
                    .send_message(OutboundMessage {
                        chat_id: msg.chat_id.clone(),
                        text: "Agent offline. Configure credentials to start processing messages."
                            .to_string(),
                        reply_to: Some(msg.id.clone()),
                        parse_mode: None,
                    })
                    .await;
                is_heartbeat_worker.store(false, Ordering::Relaxed);
                continue;
            };

            interrupt_worker.store(false, Ordering::Relaxed);
            is_busy_worker.store(true, Ordering::Relaxed);

            if let Ok(mut task) = current_task_worker.lock() {
                *task = msg.text.clone().unwrap_or_default();
            }

            let mut session_ctx = SessionContext {
                session_id: worker_chat_id.clone(),
                channel: msg.channel.clone(),
                chat_id: msg.chat_id.clone(),
                user_id: msg.user_id.clone(),
                history: persistent_history.clone(),
                workspace_path: ws_path.clone(),
            };

            let result = agent
                .process_message(
                    &msg,
                    &mut session_ctx,
                    Some(interrupt_worker.clone()),
                    Some(pending_messages.clone()),
                    None,
                    None,
                    Some(cancel_token_worker.clone()),
                )
                .await;

            match result {
                Ok((reply, _usage)) => {
                    let _ = sender.send_message(reply).await;
                    persistent_history = session_ctx.history.clone();

                    if let Ok(history_json) = serde_json::to_string(&persistent_history) {
                        let _ = memory
                            .store(electro_core::MemoryEntry {
                                id: history_key.clone(),
                                content: history_json,
                                metadata: serde_json::json!({
                                    "chat_id": worker_chat_id,
                                    "channel": msg.channel,
                                }),
                                timestamp: chrono::Utc::now(),
                                session_id: Some(worker_chat_id.clone()),
                                entry_type: electro_core::MemoryEntryType::Conversation,
                            })
                            .await;
                    }
                }
                Err(error) => {
                    let _ = sender
                        .send_message(OutboundMessage {
                            chat_id: msg.chat_id.clone(),
                            text: format!("Error: {}", error),
                            reply_to: Some(msg.id.clone()),
                            parse_mode: None,
                        })
                        .await;
                }
            }

            if let Ok(mut task) = current_task_worker.lock() {
                task.clear();
            }
            is_busy_worker.store(false, Ordering::Relaxed);
            is_heartbeat_worker.store(false, Ordering::Relaxed);
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

#[cfg(test)]
mod tests {
    use super::create_chat_worker;
    use electro_agent::AgentRuntime;
    use electro_core::types::config::{ElectroMode, MemoryStrategy};
    use electro_core::{Channel, Memory, UsageStore};
    use electro_runtime::RuntimeHandle;
    use electro_test_utils::{make_inbound_msg, MockChannel, MockMemory, MockProvider};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex, RwLock};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn worker_processes_messages_through_runtime_handle() {
        let sender = Arc::new(MockChannel::new("test"));
        let sender_trait: Arc<dyn Channel> = sender.clone();
        let memory = Arc::new(MockMemory::new());
        let provider = Arc::new(MockProvider::with_text("worker reply"));
        let agent = AgentRuntime::new(
            provider,
            memory.clone(),
            Vec::new(),
            "mock-model".to_string(),
            None,
        )
        .with_v2_optimizations(false);
        let (queue_tx, _queue_rx) = mpsc::channel(1);
        let runtime = RuntimeHandle::new(
            queue_tx,
            Arc::new(RwLock::new(ElectroMode::Play)),
            Arc::new(RwLock::new(MemoryStrategy::Lambda)),
        );
        runtime.set_agent(agent).await;

        let usage_store: Arc<dyn UsageStore> = Arc::new(
            electro_memory::SqliteUsageStore::new("sqlite::memory:")
                .await
                .expect("usage store should initialize"),
        );
        let slot = create_chat_worker(
            "test-chat",
            &sender_trait,
            &runtime,
            &(memory.clone() as Arc<dyn Memory>),
            &[],
            &Arc::new(electro_tools::CustomToolRegistry::new()),
            #[cfg(feature = "mcp")]
            &Arc::new(electro_mcp::McpManager::new()),
            8,
            4096,
            8,
            30,
            10.0,
            false,
            false,
            &None,
            &std::env::temp_dir(),
            &Arc::new(std::sync::Mutex::new(HashMap::new())),
            &electro_gateway::SetupTokenStore::new(),
            &Arc::new(Mutex::new(HashSet::new())),
            #[cfg(feature = "browser")]
            &Arc::new(Mutex::new(HashMap::new())),
            &usage_store,
            &None,
            false,
            #[cfg(feature = "browser")]
            &None,
            &None,
        );

        slot.tx
            .send(make_inbound_msg("hello worker"))
            .await
            .expect("message should be accepted by worker");

        timeout(Duration::from_secs(2), async {
            loop {
                if sender.sent_count().await > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("worker should emit a reply");

        let sent = sender.sent_messages.lock().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].text, "worker reply");
        drop(sent);

        let history = memory
            .get("chat_history:test-chat")
            .await
            .expect("history lookup should succeed")
            .expect("worker should persist chat history");
        assert!(history.content.contains("hello worker"));
        assert!(history.content.contains("worker reply"));
    }
}
