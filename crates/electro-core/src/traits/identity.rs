use crate::types::error::ElectroError;
use async_trait::async_trait;

/// Identity / auth trait — authentication and authorization
#[async_trait]
pub trait Identity: Send + Sync {
    /// Authenticate a user from a channel message
    async fn authenticate(&self, channel: &str, user_id: &str) -> Result<AuthResult, ElectroError>;

    /// Check if a user has a specific permission
    async fn has_permission(&self, user_id: &str, permission: &str) -> Result<bool, ElectroError>;

    /// Register a new user (from chat-based onboarding)
    async fn register_user(&self, user_id: &str, channel: &str) -> Result<(), ElectroError>;
}

#[derive(Debug, Clone)]
pub enum AuthResult {
    Allowed,
    Denied { reason: String },
    NeedsSetup,
}
