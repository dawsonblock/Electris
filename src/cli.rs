use clap::{Parser, Subcommand};
use electro_core::config::credentials::{
    credentials_path, is_placeholder_key, load_active_provider_keys, load_credentials_file,
};
use electro_core::types::model_registry::{
    available_models_for_provider, default_model, is_vision_model,
};

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
    /// Interactive TUI with rich rendering, observability, and slash commands
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

/// Format a ElectroError into a user-friendly message for chat.
///
/// Translates raw error variants into human-readable explanations with
/// actionable suggestions.  Raw JSON bodies and internal details are
/// never exposed to end-users.
pub fn format_user_error(e: &electro_core::types::error::ElectroError) -> String {
    use electro_core::types::error::ElectroError;
    match e {
        ElectroError::Provider(msg) => {
            // Detect common sub-categories from the raw message
            if msg.contains("400") || msg.contains("Bad Request") || msg.contains("validation") {
                "The AI provider rejected the request. This can happen when the model \
                 doesn't support certain features (like tool calling). Try switching \
                 models with /model."
                    .to_string()
            } else if msg.contains("500") || msg.contains("502") || msg.contains("503") {
                "The AI provider is experiencing issues. Please try again in a moment.".to_string()
            } else if msg.contains("timeout") || msg.contains("timed out") {
                "The request to the AI provider timed out. Please try again.".to_string()
            } else {
                "An error occurred with the AI provider. Please try again or switch \
                 models with /model."
                    .to_string()
            }
        }
        ElectroError::Auth(_) => {
            "API key issue — your key may be invalid or expired. Use /addkey to \
             update it."
                .to_string()
        }
        ElectroError::RateLimited(_) => {
            "Rate limited by the AI provider. Please wait a moment and try again.".to_string()
        }
        ElectroError::Tool(msg) => {
            format!("A tool encountered an error: {msg}")
        }
        ElectroError::Memory(_) => {
            "An error occurred accessing conversation memory. Your message wasn't \
             lost — please try again."
                .to_string()
        }
        ElectroError::Config(_) => {
            "Configuration error. Please check your setup with /status.".to_string()
        }
        _ => {
            // Generic fallback — still never shows raw internals
            "An unexpected error occurred. Please try again.".to_string()
        }
    }
}

/// List configured providers (names only, never keys).
pub fn list_configured_providers() -> String {
    let mut lines = vec![];
    let mut has_providers = false;

    // Check Codex OAuth first
    #[cfg(feature = "codex-oauth")]
    if electro_codex_oauth::TokenStore::exists() {
        has_providers = true;
        lines.push("Configured providers:".to_string());
        lines.push("  openai-codex — model: gpt-5.4, OAuth (active)".to_string());
    }

    if let Some(creds) = load_credentials_file() {
        if !creds.providers.is_empty() {
            if !has_providers {
                lines.push("Configured providers:".to_string());
            }
            has_providers = true;
            for p in &creds.providers {
                let key_count = p.keys.iter().filter(|k| !is_placeholder_key(k)).count();
                let active = if p.name == creds.active && !has_providers {
                    " (active)"
                } else {
                    ""
                };
                let proxy = if let Some(ref url) = p.base_url {
                    format!(" via {}", url)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "  {} — model: {}, {} key(s){}{}",
                    p.name, p.model, key_count, proxy, active
                ));
            }
        }
    }

    if !has_providers {
        return "No providers configured. Use /addkey to add one.".to_string();
    }

    lines.push(String::new());
    lines.push("Use /addkey to add a new key, /removekey <provider> to remove one.".to_string());
    lines.join("\n")
}

/// Handle the /model command.
///
/// - `/model` (no args) → show current model + all available models per provider
/// - `/model <exact-name>` → switch to that model on the active provider
pub fn handle_model_command(args: &str) -> String {
    // Check Codex OAuth first — if active and no args, show Codex model info
    #[cfg(feature = "codex-oauth")]
    {
        let has_creds = load_credentials_file()
            .map(|c| !c.providers.is_empty())
            .unwrap_or(false);
        if !has_creds && electro_codex_oauth::TokenStore::exists() {
            if args.is_empty() {
                let codex_models = [
                    "gpt-5.4",
                    "gpt-5.3-codex",
                    "gpt-5.3-codex-spark",
                    "gpt-5.2",
                    "gpt-5.2-codex",
                    "gpt-5.1-codex",
                    "gpt-5.1-codex-mini",
                    "gpt-5",
                    "gpt-5-codex",
                    "gpt-5-codex-mini",
                    "gpt-5-mini",
                    "gpt-4.1",
                    "gpt-4.1-mini",
                    "gpt-4.1-nano",
                    "o4-mini",
                ];
                let mut lines = vec![
                    "Current: gpt-5.4 on openai-codex provider (OAuth)".to_string(),
                    String::new(),
                    "Available Codex models:".to_string(),
                ];
                for m in &codex_models {
                    let current = if *m == "gpt-5.4" { " ← current" } else { "" };
                    lines.push(format!("    {}{}", m, current));
                }
                lines.push(String::new());
                lines.push("Switch model: /model <exact-model-name>".to_string());
                lines.push("Example: /model gpt-5.2-codex".to_string());
                return lines.join("\n");
            } else {
                let target = args.trim();
                // Return "Model switched:" so the caller rebuilds the agent
                return format!("Model switched: codex-oauth → {}\nCodex OAuth", target);
            }
        }
    }

    let creds = match load_credentials_file() {
        Some(c) => c,
        None => return "No providers configured. Use /addkey to add one.".to_string(),
    };

    if creds.providers.is_empty() {
        return "No providers configured. Use /addkey to add one.".to_string();
    }

    // ── No args: show current + available models ──────────────
    if args.is_empty() {
        let mut lines = Vec::new();

        // Current model
        if let Some(active) = creds.providers.iter().find(|p| p.name == creds.active) {
            lines.push(format!(
                "Current: {} on {} provider",
                active.model, active.name
            ));
        }

        lines.push(String::new());
        lines.push("Available models per provider:".to_string());
        for p in &creds.providers {
            let models = available_models_for_provider(&p.name);
            let active_marker = if p.name == creds.active {
                " (active)"
            } else {
                ""
            };
            let is_proxy = p.base_url.is_some() || p.name == "openrouter";
            lines.push(format!("  {}{}:", p.name, active_marker));
            if is_proxy {
                let current_vision = if is_vision_model(&p.model) {
                    " [vision]"
                } else {
                    ""
                };
                lines.push(format!("    {} ← current{}", p.model, current_vision));
                lines.push("    (proxy — any model name accepted)".to_string());
            } else {
                for m in &models {
                    let vision = if is_vision_model(m) { " [vision]" } else { "" };
                    let current = if *m == p.model { " ← current" } else { "" };
                    lines.push(format!("    {}{}{}", m, vision, current));
                }
            }
        }

        lines.push(String::new());
        lines.push("Switch model: /model <exact-model-name>".to_string());
        lines.push("Example: /model claude-sonnet-4-6".to_string());
        return lines.join("\n");
    }

    // ── Switch to specific model ──────────────────────────────
    let target = args.trim();

    // Find active provider
    let active_provider = match creds.providers.iter().find(|p| p.name == creds.active) {
        Some(p) => p.clone(),
        None => return "Active provider not found in credentials.".to_string(),
    };

    if active_provider.model == target {
        return format!("Already using {}.", target);
    }

    // Validate model against known list for the active provider.
    // Skip validation for proxy/OpenRouter providers (custom base_url) — they accept any model.
    let is_proxy = active_provider.base_url.is_some() || active_provider.name == "openrouter";
    let known = available_models_for_provider(&active_provider.name);
    if !is_proxy && !known.is_empty() && !known.contains(&target) {
        let list = known
            .iter()
            .map(|m| {
                let v = if is_vision_model(m) { " [vision]" } else { "" };
                format!("  {}{}", m, v)
            })
            .collect::<Vec<_>>()
            .join("\n");
        return format!(
            "Unknown model '{}' for provider '{}'.\n\nAvailable models:\n{}\n\nUse exact name: /model <model-name>",
            target, active_provider.name, list
        );
    }

    // Update the model in credentials.toml
    let mut updated = creds.clone();
    for p in &mut updated.providers {
        if p.name == creds.active {
            p.model = target.to_string();
        }
    }

    let path = credentials_path();
    match toml::to_string_pretty(&updated) {
        Ok(content) => {
            if let Err(e) = std::fs::write(&path, &content) {
                return format!("Failed to write credentials: {}", e);
            }
            tracing::info!(
                old_model = %active_provider.model,
                new_model = %target,
                "Model switched via /model command"
            );
            format!(
                "Model switched: {} → {}\nHot-reload will apply after this response.",
                active_provider.model, target
            )
        }
        Err(e) => format!("Failed to serialize credentials: {}", e),
    }
}

/// Remove a provider from credentials.
pub fn remove_provider(provider_name: &str) -> String {
    if provider_name.is_empty() {
        return "Usage: /removekey <provider>\nExample: /removekey openai".to_string();
    }
    let mut creds = match load_credentials_file() {
        Some(c) => c,
        None => return "No providers configured.".to_string(),
    };
    let before = creds.providers.len();
    creds.providers.retain(|p| p.name != provider_name);
    if creds.providers.len() == before {
        return format!(
            "Provider '{}' not found. Use /keys to see configured providers.",
            provider_name
        );
    }
    // If we removed the active provider, switch to first remaining
    if creds.active == provider_name {
        creds.active = creds
            .providers
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_default();
    }
    let path = credentials_path();
    match toml::to_string_pretty(&creds) {
        Ok(content) => {
            if let Err(e) = std::fs::write(&path, content) {
                return format!("Failed to save: {}", e);
            }
        }
        Err(e) => return format!("Failed to serialize: {}", e),
    }
    if creds.providers.is_empty() {
        format!(
            "Removed {}. No providers remaining — send a new API key to configure one.",
            provider_name
        )
    } else {
        format!(
            "Removed {}. Active provider: {} (model: {})",
            provider_name,
            creds.active,
            creds
                .providers
                .first()
                .map(|p| p.model.as_str())
                .unwrap_or("unknown")
        )
    }
}
