use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutboundEvent {
    Started { request_id: String },
    Token { request_id: String, content: String },
    Completed { request_id: String, content: String },
    Failed { request_id: String, error: String },
}
