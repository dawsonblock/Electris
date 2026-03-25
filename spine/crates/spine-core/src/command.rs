use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Command {
    pub intent_id: String,
    pub action: String,
    pub args: serde_json::Value,
}

impl Command {
    pub fn new(intent_id: impl Into<String>, action: impl Into<String>, args: serde_json::Value) -> Self {
        Self {
            intent_id: intent_id.into(),
            action: action.into(),
            args,
        }
    }
}
