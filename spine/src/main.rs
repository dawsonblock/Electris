//! Electris Runtime Spine - Server.
//!
//! This is the main entrypoint for the runtime spine server.
//! It starts the HTTP gateway and listens for requests.

use spine_config::Config;
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = Config::load()?;

    // Initialize tracing with configured level
    let level = parse_log_level(&config.logging.level);
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;

    tracing::info!(
        "Electris Runtime Spine v{}",
        env!("CARGO_PKG_VERSION")
    );
    
    tracing::info!(
        port = config.port(),
        llm_enabled = config.llm_enabled(),
        worker_policy = %config.worker.policy,
        "Starting server"
    );

    // Run the gateway server
    spine_gateway::run_server(config.port()).await;
    
    Ok(())
}

fn parse_log_level(s: &str) -> Level {
    match s.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    }
}
