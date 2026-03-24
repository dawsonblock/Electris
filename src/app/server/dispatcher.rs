use crate::app::onboarding::build_system_prompt;
use crate::app::server::scheduler::Scheduler;
use crate::app::server::dispatcher::classify::{classify_inbound, InboundKind};
use crate::app::server::dispatcher::router::{
    queue_pending_message, redispatch_pending, request_stop,
};
use crate::app::server::dispatcher::state::{DispatchEntry, WorkerState, IDLE_REAP_SECS};
use electro_core::types::config::ElectroConfig;
use electro_core::types::message::{
    ChatMessage, CompletionRequest, InboundMessage, MessageContent, Role,
};
use electro_core::{Channel, Memory, Tool, UsageStore, Vault};
use electro_runtime::RuntimeHandle;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub mod classify;
pub mod router;
pub mod state;

fn scheduler_wake_message() -> InboundMessage {
    InboundMessage {
        id: "scheduler-wake".to_string(),
        channel: "__scheduler__".to_string(),
        chat_id: "__scheduler__".to_string(),
        user_id: "__scheduler__".to_string(),
        username: None,
        text: None,
        attachments: Vec::new(),
        reply_to: None,
        timestamp: chrono::Utc::now(),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run_message_dispatcher(
    mut msg_rx: tokio::sync::mpsc::Receiver<InboundMessage>,
    primary_channel: Option<Arc<dyn Channel>>,
    runtime: RuntimeHandle,
    memory: Arc<dyn Memory>,
    tools: Vec<Arc<dyn Tool>>,
    custom_tool_registry: Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")] mcp_manager: Arc<electro_mcp::McpManager>,
    config: ElectroConfig,
    pending_messages: electro_tools::PendingMessages,
    setup_tokens: electro_gateway::SetupTokenStore,
    pending_raw_keys: Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")] login_sessions: Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    usage_store: Arc<dyn UsageStore>,
    hive_instance: Option<Arc<electro_hive::Hive>>,
    workspace_path: std::path::PathBuf,
    personality_locked: bool,
    tenant_manager: Arc<electro_core::tenant_impl::TenantManager>,
    #[cfg(feature = "browser")] browser_tool_ref: Option<Arc<electro_tools::BrowserTool>>,
    vault: Option<Arc<dyn Vault>>,
) {
    if let Some(sender) = primary_channel {
        let runtime_clone = runtime.clone();
        let memory_clone = memory.clone();
        let tools_clone = tools.clone();
        let custom_registry_clone = custom_tool_registry.clone();
        #[cfg(feature = "mcp")]
        let mcp_manager_clone = mcp_manager.clone();
        let config_clone = config.clone();
        let ws_path = workspace_path.clone();
        let pending_clone = pending_messages.clone();
        let setup_tokens_clone = setup_tokens.clone();
        let pending_raw_keys_clone = pending_raw_keys.clone();
        #[cfg(feature = "browser")]
        let login_sessions_clone = login_sessions.clone();
        let usage_store_clone = usage_store.clone();
        let hive_clone = hive_instance.clone();
        let tenant_mgr_clone = tenant_manager.clone();
        let tenant_isolation_enabled = config.electro.tenant_isolation;
        #[cfg(feature = "browser")]
        let browser_ref_clone = browser_tool_ref.clone();
        let queue_tx_redispatch = runtime.queue_tx.clone();
        let chat_slots: Arc<Mutex<HashMap<String, DispatchEntry>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let scheduler = Arc::new(Mutex::new(Scheduler::new(
            runtime.runtime_config.max_queue,
            runtime.runtime_config.max_active_per_chat,
        )));

        let housekeeping_slots = chat_slots.clone();
        let housekeeping_queue = runtime.queue_tx.clone();
        let housekeeping_scheduler = scheduler.clone();
        let housekeeping_runtime = runtime.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(250)).await;
                let mut buffered = Vec::new();
                let mut reap = Vec::new();

                {
                    let mut slots = housekeeping_slots.lock().await;
                    for (chat_id, entry) in slots.iter_mut() {
                        let state = entry.slot.state.read().await.clone();
                        if matches!(state, WorkerState::Idle | WorkerState::Failed) {
                            housekeeping_scheduler.lock().await.mark_complete(chat_id);
                            if let Some(next) = entry.pending.pop_front() {
                                buffered.push(next);
                            } else if entry.slot.last_active.lock().await.elapsed()
                                >= Duration::from_secs(IDLE_REAP_SECS)
                            {
                                reap.push(chat_id.clone());
                            }
                        }
                    }

                    for chat_id in reap {
                        tracing::info!(chat_id = %chat_id, "reaping idle chat worker");
                        slots.remove(&chat_id);
                    }
                }

                let queue_depth = housekeeping_scheduler.lock().await.len() as f64;
                housekeeping_runtime
                    .record_metric("electro.runtime.queue.depth", queue_depth, &[])
                    .await;

                if queue_depth > 0.0 {
                    buffered.push(scheduler_wake_message());
                }

                for inbound in buffered {
                    if housekeeping_queue.send(inbound).await.is_err() {
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            while let Some(inbound) = msg_rx.recv().await {
                let is_scheduler_wake = inbound.channel == "__scheduler__";
                let queue_depth = if is_scheduler_wake {
                    scheduler.lock().await.len() as f64
                } else {
                    let mut scheduler_guard = scheduler.lock().await;
                    if scheduler_guard.push(inbound.clone()).is_err() {
                        drop(scheduler_guard);
                        runtime_clone
                            .increment_counter("electro.runtime.overload_rejections", &[])
                            .await;
                        let _ = runtime_clone.emit_outbound_event(electro_runtime::OutboundEvent::Failed {
                            request_id: inbound.id.clone(),
                            error: "overloaded".to_string(),
                        });
                        let _ = sender
                            .send_message(electro_core::types::message::OutboundMessage {
                                chat_id: inbound.chat_id.clone(),
                                text: "System overloaded. Please retry shortly.".to_string(),
                                reply_to: Some(inbound.id.clone()),
                                parse_mode: None,
                            })
                            .await;
                        continue;
                    }

                    scheduler_guard.len() as f64
                };
                runtime_clone
                    .record_metric("electro.runtime.queue.depth", queue_depth, &[])
                    .await;

                loop {
                    let maybe_inbound = {
                        let mut scheduler_guard = scheduler.lock().await;
                        scheduler_guard.next()
                    };

                    let Some(inbound) = maybe_inbound else {
                        break;
                    };

                    let chat_id = inbound.chat_id.clone();
                    let inbound_kind = classify_inbound(&inbound);

                    let mut slots = chat_slots.lock().await;
                    let entry = ensure_worker(
                        &mut slots,
                        &chat_id,
                        &sender,
                        &runtime_clone,
                        &memory_clone,
                        &tools_clone,
                        &custom_registry_clone,
                        #[cfg(feature = "mcp")]
                        &mcp_manager_clone,
                        &config_clone,
                        &pending_clone,
                        &setup_tokens_clone,
                        &pending_raw_keys_clone,
                        #[cfg(feature = "browser")]
                        &login_sessions_clone,
                        &usage_store_clone,
                        &hive_clone,
                        &ws_path,
                        tenant_isolation_enabled,
                        personality_locked,
                        tenant_mgr_clone.clone(),
                        #[cfg(feature = "browser")]
                        &browser_ref_clone,
                        &vault,
                    );

                    let state = entry.slot.state.read().await.clone();
                    let is_running = matches!(
                        state,
                        WorkerState::Running { .. } | WorkerState::Cancelling { .. }
                    );
                    if !matches!(inbound_kind, InboundKind::SystemEvent)
                        && entry.slot.is_heartbeat.load(Ordering::Relaxed)
                    {
                        entry.slot.interrupt.store(true, Ordering::Relaxed);
                        entry.slot.cancel_token.lock().await.cancel();
                    }

                    match inbound_kind {
                        InboundKind::StopCommand => {
                            let stop_entry = StopRequest {
                                interrupt: entry.slot.interrupt.clone(),
                                cancel_token: entry.slot.cancel_token.clone(),
                                state: entry.slot.state.clone(),
                            };
                            drop(slots);
                            request_stop(stop_entry, &sender, &inbound).await;
                            continue;
                        }
                        InboundKind::SystemEvent => {}
                        InboundKind::UserMessage | InboundKind::AdminCommand(_) if is_running => {
                            if let InboundKind::UserMessage = inbound_kind {
                                maybe_intercept_busy_message(&sender, &runtime_clone, entry, &inbound);
                            }
                            queue_pending_message(entry, inbound, &pending_clone);
                            continue;
                        }
                        InboundKind::UserMessage | InboundKind::AdminCommand(_) => {}
                    }

                    *entry.slot.last_active.lock().await = Instant::now();
                    tracing::info!(
                        chat_id = %inbound.chat_id,
                        request_id = %inbound.id,
                        kind = ?inbound_kind,
                        "assigned inbound message to worker"
                    );
                    let tx = entry.slot.tx.clone();
                    let inbound_backup = inbound.clone();
                    drop(slots);
                    if tx.send(inbound).await.is_err() {
                        let mut slots = chat_slots.lock().await;
                        slots.remove(&chat_id);
                        let _ = queue_tx_redispatch.send(inbound_backup).await;
                        continue;
                    }
                    let queue_depth_after_dispatch = {
                        let mut scheduler_guard = scheduler.lock().await;
                        scheduler_guard.mark_dispatched(&chat_id);
                        scheduler_guard.len() as f64
                    };
                    runtime_clone
                        .record_metric(
                            "electro.runtime.queue.depth",
                            queue_depth_after_dispatch,
                            &[],
                        )
                        .await;

                    let mut slots = chat_slots.lock().await;
                    if let Some(entry) = slots.get_mut(&chat_id) {
                        redispatch_pending(entry, &queue_tx_redispatch).await;
                    }
                }
            }
        });
    }
}

pub(crate) struct StopRequest {
    pub(crate) interrupt: Arc<std::sync::atomic::AtomicBool>,
    pub(crate) cancel_token: Arc<tokio::sync::Mutex<tokio_util::sync::CancellationToken>>,
    pub(crate) state: Arc<tokio::sync::RwLock<WorkerState>>,
}

#[allow(clippy::too_many_arguments)]
fn ensure_worker<'a>(
    slots: &'a mut HashMap<String, DispatchEntry>,
    chat_id: &str,
    sender: &Arc<dyn Channel>,
    runtime: &RuntimeHandle,
    memory: &Arc<dyn Memory>,
    tools: &[Arc<dyn Tool>],
    custom_registry: &Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")] mcp_manager: &Arc<electro_mcp::McpManager>,
    config: &ElectroConfig,
    pending_messages: &electro_tools::PendingMessages,
    setup_tokens: &electro_gateway::SetupTokenStore,
    pending_raw_keys: &Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")] login_sessions: &Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    usage_store: &Arc<dyn UsageStore>,
    hive_instance: &Option<Arc<electro_hive::Hive>>,
    workspace_path: &std::path::Path,
    _tenant_isolation_enabled: bool,
    personality_locked: bool,
    _tenant_manager: Arc<electro_core::tenant_impl::TenantManager>,
    #[cfg(feature = "browser")] browser_tool_ref: &Option<Arc<electro_tools::BrowserTool>>,
    vault: &Option<Arc<dyn Vault>>,
) -> &'a mut DispatchEntry {
    slots.entry(chat_id.to_string()).or_insert_with(|| {
        let slot = crate::app::server::worker::create_chat_worker(
            chat_id,
            sender,
            runtime,
            memory,
            tools,
            custom_registry,
            #[cfg(feature = "mcp")]
            mcp_manager,
            config.agent.max_turns,
            config.agent.max_context_tokens,
            config.agent.max_tool_rounds,
            config.agent.max_task_duration_secs,
            config.agent.max_spend_usd,
            config.agent.v2_optimizations,
            config.agent.parallel_phases,
            &config.provider.base_url,
            workspace_path,
            pending_messages,
            setup_tokens,
            pending_raw_keys,
            #[cfg(feature = "browser")]
            login_sessions,
            usage_store,
            hive_instance,
            personality_locked,
            #[cfg(feature = "browser")]
            browser_tool_ref,
            vault,
        );
        DispatchEntry::new(slot)
    })
}

fn maybe_intercept_busy_message(
    sender: &Arc<dyn Channel>,
    runtime: &RuntimeHandle,
    entry: &DispatchEntry,
    inbound: &InboundMessage,
) {
    let icpt_sender = sender.clone();
    let icpt_chat_id = inbound.chat_id.clone();
    let icpt_msg_id = inbound.id.clone();
    let icpt_msg_text = inbound.text.clone().unwrap_or_default();
    let icpt_interrupt = entry.slot.interrupt.clone();
    let icpt_cancel = entry.slot.cancel_token.clone();
    let icpt_task = entry.slot.current_task.clone();
    let icpt_runtime = runtime.clone();

    tokio::spawn(async move {
        let task_desc = icpt_task
            .lock()
            .map(|task| task.clone())
            .unwrap_or_default();
        if let Some(agent) = icpt_runtime.agent().await {
            let provider = agent.provider_arc();
            let model = agent.model().to_string();

            let request = CompletionRequest {
                model,
                system: Some(format!(
                    "{}\n\n=== INTERCEPTOR MODE ===\n(Rules...) === END INTERCEPTOR ===\n\nTask: {}",
                    build_system_prompt(),
                    task_desc
                )),
                messages: vec![ChatMessage {
                    role: Role::User,
                    content: MessageContent::Text(icpt_msg_text),
                }],
                tools: vec![],
                max_tokens: None,
                temperature: Some(0.7),
            };

            if let Ok(resp) = provider.complete(request).await {
                let mut text = resp
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        electro_core::types::message::ContentPart::Text { text } => {
                            Some(text.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                let should_cancel = text.contains("[CANCEL]");
                text = text.replace("[CANCEL]", "").trim().to_string();

                if !text.is_empty() {
                    let _ = icpt_sender
                        .send_message(electro_core::types::message::OutboundMessage {
                            chat_id: icpt_chat_id,
                            text,
                            reply_to: Some(icpt_msg_id),
                            parse_mode: None,
                        })
                        .await;
                }

                if should_cancel {
                    icpt_interrupt.store(true, Ordering::Relaxed);
                    icpt_cancel.lock().await.cancel();
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::run_message_dispatcher;
    use async_trait::async_trait;
    use electro_agent::AgentRuntime;
    use electro_core::types::config::{ElectroConfig, ElectroMode, MemoryStrategy};
    use electro_core::types::error::ElectroError;
    use electro_core::types::message::{CompletionRequest, CompletionResponse, ContentPart};
    use electro_core::{Channel, Provider, UsageStore};
    use electro_runtime::RuntimeHandle;
    use electro_test_utils::{make_inbound_msg, MockChannel, MockMemory};
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use tokio::sync::{mpsc, Mutex, RwLock};
    use tokio::time::{timeout, Duration};

    struct DelayedProvider {
        calls: Arc<Mutex<usize>>,
    }

    #[async_trait]
    impl Provider for DelayedProvider {
        fn name(&self) -> &str {
            "anthropic"
        }

        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, ElectroError> {
            let mut calls = self.calls.lock().await;
            *calls += 1;
            let call = *calls;
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(CompletionResponse {
                id: format!("resp-{call}"),
                content: vec![ContentPart::Text {
                    text: format!("reply {call}"),
                }],
                stop_reason: Some("end_turn".to_string()),
                usage: electro_core::types::message::Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cost_usd: 0.0,
                },
            })
        }

        async fn stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<
            futures::stream::BoxStream<
                '_,
                Result<electro_core::types::message::StreamChunk, ElectroError>,
            >,
            ElectroError,
        > {
            Err(ElectroError::Provider("stream not supported".to_string()))
        }

        async fn health_check(&self) -> Result<bool, ElectroError> {
            Ok(true)
        }

        async fn list_models(&self) -> Result<Vec<String>, ElectroError> {
            Ok(vec!["claude-sonnet-4-6".to_string()])
        }
    }

    #[tokio::test]
    async fn dispatcher_buffers_busy_chat_messages() {
        let sender = Arc::new(MockChannel::new("cli"));
        let sender_trait: Arc<dyn Channel> = sender.clone();
        let memory = Arc::new(MockMemory::new());
        let provider = Arc::new(DelayedProvider {
            calls: Arc::new(Mutex::new(0)),
        });
        let agent = AgentRuntime::new(
            provider,
            memory.clone(),
            Vec::new(),
            "claude-sonnet-4-6".to_string(),
            None,
        )
        .with_v2_optimizations(false);

        let usage_store: Arc<dyn UsageStore> = Arc::new(
            electro_memory::SqliteUsageStore::new("sqlite::memory:")
                .await
                .expect("usage store should initialize"),
        );
        let (queue_tx, queue_rx) = mpsc::channel(8);
        let runtime = RuntimeHandle::new(
            queue_tx.clone(),
            Arc::new(RwLock::new(ElectroMode::Play)),
            Arc::new(RwLock::new(MemoryStrategy::Lambda)),
        );
        runtime.set_agent(agent).await;
        runtime.set_active_provider("anthropic").await;

        // Clone runtime for event subscription in test
        let runtime_for_test = runtime.clone();
        run_message_dispatcher(
            queue_rx,
            Some(sender_trait),
            runtime,
            memory.clone(),
            Vec::new(),
            Arc::new(electro_tools::CustomToolRegistry::new()),
            #[cfg(feature = "mcp")]
            Arc::new(electro_mcp::McpManager::new()),
            ElectroConfig::default(),
            Arc::new(std::sync::Mutex::new(HashMap::new())),
            electro_gateway::SetupTokenStore::new(),
            Arc::new(Mutex::new(HashSet::new())),
            #[cfg(feature = "browser")]
            Arc::new(Mutex::new(HashMap::new())),
            usage_store,
            None,
            std::env::temp_dir(),
            false,
            Arc::new(electro_core::tenant_impl::create_tenant_manager(
                &ElectroConfig::default(),
            )),
            #[cfg(feature = "browser")]
            None,
            None,
        )
        .await;

        queue_tx
            .send(make_inbound_msg("first"))
            .await
            .expect("first send should succeed");
        queue_tx
            .send(make_inbound_msg("second"))
            .await
            .expect("second send should succeed");

        // Subscribe to events and wait for both completions
        let mut events = runtime_for_test.subscribe_outbound_events();
        let mut completed_count = 0;
        timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(event) = events.recv().await {
                    if let electro_runtime::OutboundEvent::Completed { .. } = event {
                        completed_count += 1;
                        if completed_count >= 2 {
                            break;
                        }
                    }
                }
            }
        })
        .await
        .expect("dispatcher should flush buffered messages");
    }
}
