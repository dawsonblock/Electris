use crate::app::onboarding::build_system_prompt;
use crate::app::server::context::WorkerServices;
use crate::app::server::slot::ChatSlot;
use electro_core::types::message::{
    ChatMessage, CompletionRequest, InboundMessage, MessageContent, Role,
};
use electro_core::Channel;
use electro_core::Tenant;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Mutex;

#[allow(clippy::too_many_arguments)]
pub async fn run_message_dispatcher(
    mut msg_rx: tokio::sync::mpsc::Receiver<InboundMessage>,
    msg_tx: tokio::sync::mpsc::Sender<InboundMessage>,
    primary_channel: Option<Arc<dyn Channel>>,
    services: WorkerServices,
) {
    if let Some(sender) = primary_channel {
        let chat_slots: Arc<Mutex<HashMap<String, ChatSlot>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let msg_tx_redispatch = msg_tx.clone();
        let services_clone = services.clone();

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
                                if let Ok(mut pq) = services_clone.pending_messages.lock() {
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
                            let icpt_services = services_clone.clone();

                            tokio::spawn(async move {
                                let task_desc =
                                    icpt_task.lock().map(|t| t.clone()).unwrap_or_default();
                                let agent_guard = icpt_services.agent_state.read().await;
                                if let Some(agent) = agent_guard.as_ref() {
                                    let provider = agent.provider_arc();
                                    let model = agent.model().to_string();
                                    drop(agent_guard);

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
                let chat_workspace = if services_clone.config.electro.tenant_isolation {
                    let tid = services_clone
                        .tenant_manager
                        .resolve_tenant(&inbound.channel, &inbound.user_id)
                        .await
                        .unwrap_or_else(|_| electro_core::traits::TenantId::default_tenant());
                    services_clone.tenant_manager.workspace_path(&tid)
                } else {
                    services_clone.workspace_root.clone()
                };

                let slot = slots.entry(chat_id.clone()).or_insert_with(|| {
                    crate::app::server::worker::create_chat_worker(
                        &chat_id,
                        &sender,
                        &services_clone,
                        chat_workspace,
                    )
                });

                if !is_heartbeat_msg {
                    let tx = slot.tx.clone();
                    let inbound_backup = inbound.clone();
                    drop(slots);
                    if tx.send(inbound).await.is_err() {
                        let mut slots = chat_slots.lock().await;
                        slots.remove(&chat_id);
                        let _ = msg_tx_redispatch.send(inbound_backup).await;
                    }
                }
            }
        });
    }
}
