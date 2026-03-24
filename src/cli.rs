use clap::{Parser, Subcommand};

/// Main CLI struct for ELECTRO
#[derive(Parser)]
#[command(name = "electro")]
#[command(about = "Cloud-native Rust AI agent runtime — Telegram-native")]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " — commit: ", env!("GIT_HASH"), " — date: ", env!("BUILD_DATE")))]
pub struct Cli {
    /// Path to config file
    #[arg(short, long)]
    pub config: Option<String>,

    /// Runtime mode: cloud, local, or auto
    #[arg(long, default_value = "auto")]
    pub mode: String,

    #[command(subcommand)]
    pub command: Commands,
}

/// Main commands enum
#[derive(Subcommand)]
pub enum Commands {
    /// Start the ELECTRO gateway daemon
    Start {
        /// Run as a background daemon (requires prior setup via `electro start` first)
        #[arg(short, long)]
        daemon: bool,
        /// Log file path when running as daemon (default: ~/.electro/electro.log)
        #[arg(long)]
        log: Option<String>,
        /// Electro personality mode: play (warm, chaotic :3), work (sharp, precise >:3), pro (professional, no emoticons), or none (no personality, minimal identity)
        #[arg(long, default_value = "play")]
        personality: String,
    },
    /// Stop a running daemon
    Stop,
    /// Interactive CLI chat with the agent
    Chat,
    /// Show gateway status, connected channels, provider health
    Status,
    /// Manage skills
    Skill {
        #[command(subcommand)]
        command: SkillCommands,
    },
    /// Manage configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// Show version information
    Version,
    /// Check for updates and install if available
    Update,
    /// Factory reset — wipe all local state and start fresh
    Reset {
        /// Skip confirmation prompt (for scripted use)
        #[arg(long)]
        confirm: bool,
    },
    /// Manage OpenAI Codex OAuth authentication
    #[cfg(feature = "codex-oauth")]
    Auth {
        #[command(subcommand)]
        command: AuthCommands,
    },
    #[cfg(feature = "tui")]
    Tui,
}

/// Authentication commands for Codex OAuth
#[cfg(feature = "codex-oauth")]
#[derive(Subcommand)]
pub enum AuthCommands {
    /// Authenticate with your ChatGPT Plus/Pro subscription via OAuth
    Login {
        /// Use headless mode (paste URL instead of browser redirect)
        #[arg(long)]
        headless: bool,
        /// Export oauth.json to a custom path (for Docker/remote deployments)
        #[arg(long)]
        output: Option<String>,
    },
    /// Show current OAuth authentication status
    Status,
    /// Remove OAuth tokens and log out
    Logout,
}

/// Skill management commands
#[derive(Subcommand)]
pub enum SkillCommands {
    /// List installed skills
    List,
    /// Show skill details
    Info { name: String },
    /// Install a skill from a path
    Install { path: String },
}

/// Configuration management commands
#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Validate the current configuration
    Validate,
    /// Show resolved configuration
    Show,
}

// All implementation logic has been moved to crate::app::cli
