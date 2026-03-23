use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;
use electro_core::types::message::InboundMessage;

/// Tracks the active task state for a single chat.
pub struct ChatSlot {
    pub tx: Sender<InboundMessage>,
    pub interrupt: Arc<AtomicBool>,
    pub is_heartbeat: Arc<AtomicBool>,
    pub is_busy: Arc<AtomicBool>,
    pub current_task: Arc<std::sync::Mutex<String>>,
    pub cancel_token: CancellationToken,
}
