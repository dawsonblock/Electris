mod admin;
mod app;
mod bootstrap;
mod cli;
mod daemon;
mod reset;
mod server_mode;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // ── Logging & Panic Hook ──
    #[cfg(feature = "tui")]
    let is_tui = matches!(cli.command, Commands::Tui);
    #[cfg(not(feature = "tui"))]
    let is_tui = false;
    app::init_logging(is_tui);
    app::init_panic_hook();

    // ── TUI fast path ──
    #[cfg(feature = "tui")]
    if is_tui {
        let config_path = cli.config.as_deref().map(std::path::Path::new);
        let config = electro_core::config::load_config(config_path)?;
        return electro_tui::launch_tui(config).await;
    }

    // Initialize health endpoint uptime clock
    electro_gateway::health::init_start_time();

    // ── Handle Reset before config loading ──
    if let Commands::Reset { confirm } = &cli.command {
        return reset::factory_reset(*confirm).await;
    }

    // Load configuration
    let config_path = cli.config.as_ref().map(std::path::Path::new);
    let mut config = electro_core::config::load_config(config_path)?;

    // ── Security Policy Enforcement ──
    app::enforce_security_policy(&config);

    if !is_tui {
        tracing::info!(mode = %cli.mode, "ELECTRO starting");
    }

    match cli.command {
        Commands::Stop => daemon::stop_daemon_cli()?,
        Commands::Start {
            daemon,
            log,
            personality,
        } => {
            if daemon {
                daemon::start_daemon(log).map_err(|e| anyhow::anyhow!(e))?;
            } else {
                daemon::write_pid_file();
                server_mode::start_server(&mut config, personality, cli.mode).await?;
            }
        }
        Commands::Chat => {
            app::chat::run_chat_mode(config, config_path).await?;
        }
        _ => {
            // Forward other commands to app::cli module
            crate::app::cli::handle_remaining_commands(cli.command, &config).await?;
        }
    }

    Ok(())
}
