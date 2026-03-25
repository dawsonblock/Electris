use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Intent {
    pub id: String,
    pub payload: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Intent {
    pub fn new(payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            payload,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn with_id(id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            payload,
            created_at: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_has_unique_id() {
        let i1 = Intent::new(serde_json::json!({}));
        let i2 = Intent::new(serde_json::json!({}));
        assert_ne!(i1.id, i2.id);
    }
}
