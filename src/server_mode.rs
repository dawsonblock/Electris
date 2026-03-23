use anyhow::Result;
use electro_core::types::config::ElectroConfig;

/// Main server start function (delegated to refactored app layer)
pub async fn start_server(
    config: &mut ElectroConfig,
    personality: String,
    cli_mode: String,
) -> Result<()> {
    crate::app::server::start_server(config, personality, cli_mode).await
}
