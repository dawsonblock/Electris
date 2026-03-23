use crate::app::server::dispatcher::state::WorkerState;
use electro_core::types::message::InboundMessage;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Sender;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

/// Tracks the active task state for a single chat.
pub struct ChatSlot {
    pub tx: Sender<InboundMessage>,
    pub interrupt: Arc<AtomicBool>,
    pub is_heartbeat: Arc<AtomicBool>,
    pub current_task: Arc<std::sync::Mutex<String>>,
    pub cancel_token: Arc<Mutex<CancellationToken>>,
    pub state: Arc<RwLock<WorkerState>>,
    pub last_active: Arc<Mutex<Instant>>,
}
