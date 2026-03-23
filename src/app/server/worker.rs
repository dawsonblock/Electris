use crate::app::server::commands::handle_slash_command;
use crate::app::server::dispatcher::state::WorkerState;
use crate::app::server::slot::ChatSlot;
use electro_core::types::message::{ChatMessage, InboundMessage, OutboundMessage};
use electro_core::types::session::SessionContext;
use electro_core::{Channel, Memory, Tool, UsageStore, Vault};
use electro_runtime::{OutboundEvent, RuntimeHandle};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};

pub enum WorkerOutcome {
    Success { response: String },
    Timeout,
    Cancelled,
    Failed { error: String },
}

fn emit_outbound_event(runtime: &RuntimeHandle, event: OutboundEvent) {
    if let Err(error) = runtime.emit_outbound_event(event) {
        tracing::debug!(
            ?error,
            "Skipping outbound event broadcast without subscribers"
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn create_chat_worker(
    worker_chat_id: &str,
    sender: &Arc<dyn Channel>,
    runtime: &RuntimeHandle,
    memory: &Arc<dyn Memory>,
    tools_template: &[Arc<dyn Tool>],
    _custom_registry: &Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")] _mcp_mgr: &Arc<electro_mcp::McpManager>,
    _max_turns: usize,
    _max_ctx: usize,
    _max_rounds: usize,
    max_task_duration: u64,
    _max_spend: f64,
    _v2_opt: bool,
    _pp_opt: bool,
    _base_url: &Option<String>,
    ws_path: &std::path::Path,
    pending_clone: &electro_tools::PendingMessages,
    setup_tokens_clone: &electro_gateway::SetupTokenStore,
    pending_raw_keys_clone: &Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")] login_sessions_clone: &Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    _usage_store_clone: &Arc<dyn UsageStore>,
    _hive_clone: &Option<Arc<electro_hive::Hive>>,
    personality_locked: bool,
    #[cfg(feature = "browser")] browser_ref_worker: &Option<Arc<electro_tools::BrowserTool>>,
    vault: &Option<Arc<dyn Vault>>,
) -> ChatSlot {
    let (chat_tx, mut chat_rx) = tokio::sync::mpsc::channel::<InboundMessage>(4);
    let interrupt = Arc::new(AtomicBool::new(false));
    let is_heartbeat = Arc::new(AtomicBool::new(false));
    let current_task = Arc::new(std::sync::Mutex::new(String::new()));
    let cancel_token = Arc::new(Mutex::new(tokio_util::sync::CancellationToken::new()));
    let state = Arc::new(RwLock::new(WorkerState::Idle));
    let last_active = Arc::new(Mutex::new(Instant::now()));

    let worker_chat_id = worker_chat_id.to_string();
    let sender = sender.clone();
    let memory = memory.clone();
    let runtime = runtime.clone();
    let current_task_worker = current_task.clone();
    let interrupt_worker = interrupt.clone();
    let is_heartbeat_worker = is_heartbeat.clone();
    let cancel_token_worker = cancel_token.clone();
    let state_worker = state.clone();
    let last_active_worker = last_active.clone();
    let tools_template = tools_template.to_vec();
    let ws_path = ws_path.to_path_buf();
    let pending_messages = pending_clone.clone();
    let setup_tokens = setup_tokens_clone.clone();
    let pending_raw_keys = pending_raw_keys_clone.clone();
    #[cfg(feature = "browser")]
    let login_sessions = login_sessions_clone.clone();
    #[cfg(feature = "browser")]
    let browser_ref = browser_ref_worker.clone();
    let vault = vault.clone();

    tokio::spawn(async move {
        let history_key = format!("chat_history:{}", worker_chat_id);
        let mut persistent_history: Vec<ChatMessage> = match memory.get(&history_key).await {
            Ok(Some(entry)) => serde_json::from_str(&entry.content).unwrap_or_default(),
            _ => Vec::new(),
        };

        while let Some(msg) = chat_rx.recv().await {
            is_heartbeat_worker.store(msg.channel == "heartbeat", Ordering::Relaxed);
            *last_active_worker.lock().await = Instant::now();

            tracing::info!(
                chat_id = %msg.chat_id,
                request_id = %msg.id,
                channel = %msg.channel,
                "worker request dequeued"
            );

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
                *state_worker.write().await = WorkerState::Idle;
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
                *state_worker.write().await = WorkerState::Failed;
                is_heartbeat_worker.store(false, Ordering::Relaxed);
                continue;
            };

            interrupt_worker.store(false, Ordering::Relaxed);
            if let Ok(mut task) = current_task_worker.lock() {
                *task = msg.text.clone().unwrap_or_default();
            }

            let request_id = msg.id.clone();
            emit_outbound_event(
                &runtime,
                OutboundEvent::Started {
                    request_id: request_id.clone(),
                },
            );
            let request_cancel = tokio_util::sync::CancellationToken::new();
            let mut cancel_slot = cancel_token_worker.lock().await;
            cancel_slot.cancel();
            *cancel_slot = request_cancel.clone();
            drop(cancel_slot);
            *state_worker.write().await = WorkerState::Running {
                request_id: request_id.clone(),
            };
            tracing::info!(
                chat_id = %msg.chat_id,
                request_id = %request_id,
                "worker request started"
            );

            let mut session_ctx = SessionContext {
                session_id: worker_chat_id.clone(),
                channel: msg.channel.clone(),
                chat_id: msg.chat_id.clone(),
                user_id: msg.user_id.clone(),
                history: persistent_history.clone(),
                workspace_path: ws_path.clone(),
            };

            let (status_tx, mut status_rx) =
                tokio::sync::watch::channel(electro_agent::AgentTaskStatus::default());
            let request_id_for_logs = request_id.clone();
            let chat_id_for_logs = msg.chat_id.clone();
            let lifecycle_logger = tokio::spawn(async move {
                let mut last_phase = None;
                while status_rx.changed().await.is_ok() {
                    let status = status_rx.borrow().clone();
                    if Some(status.phase.clone()) == last_phase {
                        continue;
                    }
                    match &status.phase {
                        electro_agent::AgentTaskPhase::CallingProvider { round } => tracing::info!(
                            chat_id = %chat_id_for_logs,
                            request_id = %request_id_for_logs,
                            round = *round,
                            "provider_called"
                        ),
                        electro_agent::AgentTaskPhase::ExecutingTool {
                            round, tool_name, ..
                        } => tracing::info!(
                            chat_id = %chat_id_for_logs,
                            request_id = %request_id_for_logs,
                            round = *round,
                            tool = %tool_name,
                            "tool_called"
                        ),
                        _ => {}
                    }
                    last_phase = Some(status.phase);
                }
            });

            let result = tokio::time::timeout(
                Duration::from_secs(max_task_duration),
                agent.process_message(
                    &msg,
                    &mut session_ctx,
                    Some(interrupt_worker.clone()),
                    Some(pending_messages.clone()),
                    None,
                    Some(status_tx),
                    Some(request_cancel.clone()),
                ),
            )
            .await;
            lifecycle_logger.abort();

            let outcome = match result {
                Ok(Ok((reply, _usage))) => {
                    let response_text = reply.text.clone();
                    let _ = sender.send_message(reply).await;
                    emit_outbound_event(
                        &runtime,
                        OutboundEvent::Completed {
                            request_id: request_id.clone(),
                            content: response_text.clone(),
                        },
                    );
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

                    WorkerOutcome::Success {
                        response: response_text,
                    }
                }
                Ok(Err(error)) => {
                    if request_cancel.is_cancelled() || interrupt_worker.load(Ordering::Relaxed) {
                        WorkerOutcome::Cancelled
                    } else {
                        let error = error.to_string();
                        emit_outbound_event(
                            &runtime,
                            OutboundEvent::Failed {
                                request_id: request_id.clone(),
                                error: error.clone(),
                            },
                        );
                        WorkerOutcome::Failed { error }
                    }
                }
                Err(_) => {
                    request_cancel.cancel();
                    emit_outbound_event(
                        &runtime,
                        OutboundEvent::Failed {
                            request_id: request_id.clone(),
                            error: format!("request timed out after {} seconds", max_task_duration),
                        },
                    );
                    WorkerOutcome::Timeout
                }
            };

            match &outcome {
                WorkerOutcome::Success { response } => {
                    tracing::info!(
                        chat_id = %msg.chat_id,
                        request_id = %request_id,
                        response_len = response.len(),
                        "worker request completed"
                    );
                    *state_worker.write().await = WorkerState::Idle;
                }
                WorkerOutcome::Timeout => {
                    tracing::warn!(
                        chat_id = %msg.chat_id,
                        request_id = %request_id,
                        timeout_secs = max_task_duration,
                        "worker request timed out"
                    );
                    let _ = sender
                        .send_message(OutboundMessage {
                            chat_id: msg.chat_id.clone(),
                            text: format!(
                                "Error: request timed out after {} seconds",
                                max_task_duration
                            ),
                            reply_to: Some(msg.id.clone()),
                            parse_mode: None,
                        })
                        .await;
                    *state_worker.write().await = WorkerState::Idle;
                }
                WorkerOutcome::Cancelled => {
                    tracing::info!(
                        chat_id = %msg.chat_id,
                        request_id = %request_id,
                        "worker request cancelled"
                    );
                    *state_worker.write().await = WorkerState::Idle;
                }
                WorkerOutcome::Failed { error } => {
                    tracing::error!(
                        chat_id = %msg.chat_id,
                        request_id = %request_id,
                        error = %error,
                        "worker request failed"
                    );
                    let _ = sender
                        .send_message(OutboundMessage {
                            chat_id: msg.chat_id.clone(),
                            text: format!("Error: {error}"),
                            reply_to: Some(msg.id.clone()),
                            parse_mode: None,
                        })
                        .await;
                    *state_worker.write().await = WorkerState::Failed;
                }
            }

            if let Ok(mut task) = current_task_worker.lock() {
                task.clear();
            }
            let mut cancel_slot = cancel_token_worker.lock().await;
            cancel_slot.cancel();
            *cancel_slot = tokio_util::sync::CancellationToken::new();
            drop(cancel_slot);
            *last_active_worker.lock().await = Instant::now();
            is_heartbeat_worker.store(false, Ordering::Relaxed);
        }
    });

    ChatSlot {
        tx: chat_tx,
        interrupt,
        is_heartbeat,
        current_task,
        cancel_token,
        state,
        last_active,
    }
}

#[cfg(test)]
mod tests {
    use super::create_chat_worker;
    use crate::app::server::dispatcher::state::WorkerState;
    use electro_agent::AgentRuntime;
    use electro_core::types::config::{ElectroMode, MemoryStrategy};
    use electro_core::{Channel, Memory, UsageStore};
    use electro_runtime::{OutboundEvent, RuntimeHandle};
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
        runtime.set_active_provider("anthropic").await;

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

        let msg = make_inbound_msg("hello worker");

        slot.tx
            .send(msg)
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
        assert!(matches!(&*slot.state.read().await, WorkerState::Idle));
    }

    #[tokio::test]
    async fn worker_emits_outbound_events_for_completed_requests() {
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
        );
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
        let mut events = runtime.subscribe_outbound_events();

        let msg = make_inbound_msg("hello worker");
        let request_id = msg.id.clone();

        slot.tx
            .send(msg)
            .await
            .expect("message should be accepted by worker");

        let started = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("started event should arrive")
            .expect("started event should be readable");
        let completed = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("completed event should arrive")
            .expect("completed event should be readable");

        assert!(matches!(
            started,
            OutboundEvent::Started { request_id } if !request_id.is_empty()
        ));
        assert_eq!(
            completed,
            OutboundEvent::Completed {
                request_id,
                content: "worker reply".to_string(),
            }
        );
    }
}
