use crate::app::server::slot::ChatSlot;
use electro_core::types::message::InboundMessage;
use std::collections::VecDeque;

pub const MAX_PENDING_PER_CHAT: usize = 8;
pub const IDLE_REAP_SECS: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerState {
    Idle,
    Running { request_id: String },
    Cancelling { request_id: String },
    Failed,
}

pub struct DispatchEntry {
    pub slot: ChatSlot,
    pub pending: VecDeque<InboundMessage>,
}

impl DispatchEntry {
    pub fn new(slot: ChatSlot) -> Self {
        Self {
            slot,
            pending: VecDeque::new(),
        }
    }
}
