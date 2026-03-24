use crate::policy::DenialReason;
use serde::{Deserialize, Serialize};

/// An immutable record of a capability decision made by the PolicyEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityDecisionRecord {
    pub tool_name: String,
    pub session_id: String,
    pub timestamp: u64,
    pub action: String,
    pub decision: DecisionOutCome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionOutCome {
    Allowed,
    Denied { reason: String },
}

impl CapabilityDecisionRecord {
    pub fn allowed(
        tool_name: impl Into<String>,
        session_id: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            session_id: session_id.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            action: action.into(),
            decision: DecisionOutCome::Allowed,
        }
    }

    pub fn denied(
        tool_name: impl Into<String>,
        session_id: impl Into<String>,
        action: impl Into<String>,
        reason: &DenialReason,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            session_id: session_id.into(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            action: action.into(),
            decision: DecisionOutCome::Denied {
                reason: reason.to_string(),
            },
        }
    }
}
