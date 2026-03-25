use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DomainEvent {
    IntentReceived { intent_id: String },
    CommandStarted { intent_id: String, action: String },
    CommandCompleted { intent_id: String },
    CommandFailed { intent_id: String, error: String },
}
