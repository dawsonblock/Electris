use crate::app::onboarding::build_system_prompt;
use crate::app::server::slot::ChatSlot;
use electro_core::types::config::ElectroConfig;
use electro_core::types::message::{
    ChatMessage, CompletionRequest, InboundMessage, MessageContent, Role,
};
use electro_core::{Channel, Memory, Tool, UsageStore, Vault};
use electro_runtime::RuntimeHandle;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Mutex;

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

        let chat_slots: Arc<Mutex<HashMap<String, ChatSlot>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let queue_tx_redispatch = runtime.queue_tx.clone();
        tokio::spawn(async move {
            while let Some(inbound) = msg_rx.recv().await {
                let chat_id = inbound.chat_id.clone();
                let is_heartbeat_msg = inbound.channel == "heartbeat";

                let mut slots = chat_slots.lock().await;

                // Handle user messages while a task is active
                if !is_heartbeat_msg {
                    if let Some(slot) = slots.get(&chat_id) {
                        if slot.is_heartbeat.load(Ordering::Relaxed) {
                            slot.interrupt.store(true, Ordering::Relaxed);
                            slot.cancel_token.cancel();
                        }

                        let is_slash_stop = inbound
                            .text
                            .as_deref()
                            .map(|t| t.trim().eq_ignore_ascii_case("/stop"))
                            .unwrap_or(false);

                        if is_slash_stop {
                            slot.interrupt.store(true, Ordering::Relaxed);
                            slot.cancel_token.cancel();
                            continue;
                        }

                        if slot.is_busy.load(Ordering::Relaxed) {
                            if let Some(text) = inbound.text.as_deref() {
                                if let Ok(mut pq) = pending_clone.lock() {
                                    pq.entry(chat_id.clone())
                                        .or_default()
                                        .push(text.to_string());
                                }
                            }

                            // LLM interceptor logic...
                            let icpt_sender = sender.clone();
                            let icpt_chat_id = chat_id.clone();
                            let icpt_msg_id = inbound.id.clone();
                            let icpt_msg_text = inbound.text.clone().unwrap_or_default();
                            let icpt_interrupt = slot.interrupt.clone();
                            let icpt_cancel = slot.cancel_token.clone();
                            let icpt_task = slot.current_task.clone();
                            let icpt_runtime = runtime_clone.clone();
                            tokio::spawn(async move {
                                let task_desc =
                                    icpt_task.lock().map(|t| t.clone()).unwrap_or_default();
                                if let Some(agent) = icpt_runtime.agent().await {
                                    let provider = agent.provider_arc();
                                    let model = agent.model().to_string();

                                    let soul = build_system_prompt();
                                    let request = CompletionRequest {
                                        model,
                                        system: Some(format!("{}\n\n=== INTERCEPTOR MODE ===\n(Rules...) === END INTERCEPTOR ===\n\nTask: {}", soul, task_desc)),
                                        messages: vec![ChatMessage { role: Role::User, content: MessageContent::Text(icpt_msg_text) }],
                                        tools: vec![],
                                        max_tokens: None,
                                        temperature: Some(0.7),
                                    };

                                    if let Ok(resp) = provider.complete(request).await {
                                        let mut text = resp.content.iter().filter_map(|p| match p {
                                            electro_core::types::message::ContentPart::Text { text } => Some(text.as_str()),
                                            _ => None,
                                        }).collect::<Vec<_>>().join("");

                                        let should_cancel = text.contains("[CANCEL]");
                                        text = text.replace("[CANCEL]", "").trim().to_string();

                                        if !text.is_empty() {
                                            let _ = icpt_sender
                                                .send_message(
                                                    electro_core::types::message::OutboundMessage {
                                                        chat_id: icpt_chat_id,
                                                        text,
                                                        reply_to: Some(icpt_msg_id),
                                                        parse_mode: None,
                                                    },
                                                )
                                                .await;
                                        }

                                        if should_cancel {
                                            icpt_interrupt.store(true, Ordering::Relaxed);
                                            icpt_cancel.cancel();
                                        }
                                    }
                                }
                            });
                            continue;
                        }
                    }
                }

                // Ensure a worker exists for this chat_id
                let chat_workspace = if tenant_isolation_enabled {
                    ws_path.clone() // resolution logic remains in full impl
                } else {
                    ws_path.clone()
                };

                let slot = slots.entry(chat_id.clone()).or_insert_with(|| {
                    crate::app::server::worker::create_chat_worker(
                        &chat_id,
                        &sender,
                        &runtime_clone,
                        &memory_clone,
                        &tools_clone,
                        &custom_registry_clone,
                        #[cfg(feature = "mcp")]
                        &mcp_manager_clone,
                        config_clone.agent.max_turns,
                        config_clone.agent.max_context_tokens,
                        config_clone.agent.max_tool_rounds,
                        config_clone.agent.max_task_duration_secs,
                        config_clone.agent.max_spend_usd,
                        config_clone.agent.v2_optimizations,
                        config_clone.agent.parallel_phases,
                        &config_clone.provider.base_url,
                        &chat_workspace,
                        &pending_clone,
                        &setup_tokens_clone,
                        &pending_raw_keys_clone,
                        #[cfg(feature = "browser")]
                        &login_sessions_clone,
                        &usage_store_clone,
                        &hive_clone,
                        personality_locked,
                        #[cfg(feature = "browser")]
                        &browser_tool_ref,
                        &vault,
                    )
                });

                // Heartbeats must traverse the worker loop too so they use the
                // same execution, interruption, and persistence path.
                let tx = slot.tx.clone();
                let inbound_backup = inbound.clone();
                drop(slots);
                if tx.send(inbound).await.is_err() {
                    let mut slots = chat_slots.lock().await;
                    slots.remove(&chat_id);
                    let _ = queue_tx_redispatch.send(inbound_backup).await;
                }
            }
        });
    }
}
