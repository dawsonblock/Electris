use serde::{Deserialize, Serialize};

use electro_core::types::message::ChatMessage;

use crate::events::OutboundEvent;

pub const MAX_REMOTE_REQUEST_BYTES: usize = 250_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRequest {
    pub request_id: String,
    pub input: String,
    pub channel: String,
    pub chat_id: String,
    pub user_id: String,
    pub history: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteResponse {
    pub request_id: String,
    pub output: String,
    pub history: Vec<ChatMessage>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStreamEvent {
    pub request_id: String,
    pub event: OutboundEvent,
}
