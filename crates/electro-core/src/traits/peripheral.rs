use crate::types::error::ElectroError;
use async_trait::async_trait;

/// Peripheral trait — hardware integration (stub for v0.1)
#[async_trait]
pub trait Peripheral: Send + Sync {
    fn name(&self) -> &str;
    async fn read(&self) -> Result<serde_json::Value, ElectroError>;
    async fn write(&self, data: serde_json::Value) -> Result<(), ElectroError>;
}
