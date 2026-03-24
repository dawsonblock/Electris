use crate::app::server::dispatcher::classify::InboundKind;
use crate::app::server::dispatcher::state::{DispatchEntry, WorkerState, MAX_PENDING_PER_CHAT};
use crate::app::server::dispatcher::StopRequest;
use electro_core::types::message::InboundMessage;
use electro_runtime::{OutboundEvent, RuntimeHandle};
use std::sync::atomic::Ordering;

pub fn queue_pending_message(
    entry: &mut DispatchEntry,
    inbound: InboundMessage,
    pending_messages: &electro_tools::PendingMessages,
) {
    if entry.pending.len() >= MAX_PENDING_PER_CHAT {
        if let Some(dropped) = entry.pending.pop_front() {
            tracing::warn!(
                chat_id = %dropped.chat_id,
                dropped_request_id = %dropped.id,
                max_pending = MAX_PENDING_PER_CHAT,
                "dropping oldest buffered chat message"
            );
        }
    }

    if let Some(text) = inbound.text.as_deref() {
        if let Ok(mut pending) = pending_messages.lock() {
            let queue = pending.entry(inbound.chat_id.clone()).or_default();
            queue.push(text.to_string());
            if queue.len() > MAX_PENDING_PER_CHAT {
                queue.remove(0);
            }
        }
    }

    tracing::info!(
        chat_id = %inbound.chat_id,
        request_id = %inbound.id,
        kind = ?classify_inbound_label(&inbound),
        buffered = entry.pending.len() + 1,
        "queued inbound message behind active worker"
    );
    entry.pending.push_back(inbound);
}

pub async fn request_stop(
    stop_request: StopRequest,
    runtime: &RuntimeHandle,
    inbound: &InboundMessage,
) {
    let request_id = match &*stop_request.state.read().await {
        WorkerState::Running { request_id } | WorkerState::Cancelling { request_id } => {
            Some(request_id.clone())
        }
        WorkerState::Idle | WorkerState::Failed => None,
    };

    if let Some(request_id) = request_id {
        stop_request.interrupt.store(true, Ordering::Relaxed);
        stop_request.cancel_token.lock().await.cancel();
        *stop_request.state.write().await = WorkerState::Cancelling {
            request_id: request_id.clone(),
        };
        tracing::info!(
            chat_id = %inbound.chat_id,
            request_id = %request_id,
            "cancelling active worker request"
        );
        // Unified output: emit event instead of direct send_message
        let _ = runtime.emit_outbound_event(OutboundEvent::Completed {
            request_id: inbound.id.clone(),
            content: "Task stopped.".to_string(),
        });
    } else {
        let _ = runtime.emit_outbound_event(OutboundEvent::Completed {
            request_id: inbound.id.clone(),
            content: "No active task to stop.".to_string(),
        });
    }
}

pub async fn redispatch_pending(
    entry: &mut DispatchEntry,
    queue_tx: &tokio::sync::mpsc::Sender<InboundMessage>,
) {
    while matches!(
        &*entry.slot.state.read().await,
        WorkerState::Idle | WorkerState::Failed
    ) {
        let Some(next) = entry.pending.pop_front() else {
            break;
        };
        tracing::info!(
            chat_id = %next.chat_id,
            request_id = %next.id,
            remaining_buffered = entry.pending.len(),
            "redispatching buffered message to worker"
        );
        if queue_tx.send(next).await.is_err() {
            break;
        }
    }
}

fn classify_inbound_label(inbound: &InboundMessage) -> &'static str {
    match super::classify::classify_inbound(inbound) {
        InboundKind::UserMessage => "user",
        InboundKind::StopCommand => "stop",
        InboundKind::AdminCommand(_) => "admin",
        InboundKind::SystemEvent => "system",
    }
}
