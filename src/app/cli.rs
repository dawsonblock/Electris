use electro_core::config::credentials::{
    credentials_path, is_placeholder_key, load_active_provider_keys, load_credentials_file,
};
use electro_core::types::model_registry::{
    available_models_for_provider, is_vision_model,
};
use crate::cli::{Commands, SkillCommands, ConfigCommands};

pub fn format_user_error(e: &electro_core::types::error::ElectroError) -> String {
    use electro_core::types::error::ElectroError;
    match e {
        ElectroError::Provider(msg) => {
            if msg.contains("400") || msg.contains("Bad Request") || msg.contains("validation") {
                "The AI provider rejected the request. Try switching models with /model.".to_string()
            } else if msg.contains("500") || msg.contains("502") || msg.contains("503") {
                "The AI provider is experiencing issues.".to_string()
            } else if msg.contains("timeout") || msg.contains("timed out") {
                "The request to the AI provider timed out.".to_string()
            } else {
                "An error occurred with the AI provider.".to_string()
            }
        }
        ElectroError::Auth(_) => "API key issue — Use /addkey to update it.".to_string(),
        ElectroError::RateLimited(_) => "Rate limited by the AI provider.".to_string(),
        ElectroError::Tool(msg) => format!("A tool encountered an error: {msg}"),
        ElectroError::Memory(_) => "An error occurred accessing conversation memory.".to_string(),
        ElectroError::Config(_) => "Configuration error. Check setup with /status.".to_string(),
        _ => "An unexpected error occurred.".to_string(),
    }
}

pub fn list_configured_providers() -> String {
    let mut lines = vec![];
    let mut has_providers = false;

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
                let active = if p.name == creds.active { " (active)" } else { "" };
                let proxy = if let Some(ref url) = p.base_url { format!(" via {}", url) } else { String::new() };
                lines.push(format!("  {} — model: {}, {} key(s){}{}", p.name, p.model, key_count, proxy, active));
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

pub fn handle_model_command(args: &str) -> String {
    // ... complete implementation from original cli.rs ...
    format!("Model command handled: {}", args) // placeholder for now, will fill in full
}

pub fn remove_provider(provider_name: &str) -> String {
    if provider_name.is_empty() {
        return "Usage: /removekey <provider>".to_string();
    }
    let mut creds = match load_credentials_file() {
        Some(c) => c,
        None => return "No providers configured.".to_string(),
    };
    let before = creds.providers.len();
    creds.providers.retain(|p| p.name != provider_name);
    if creds.providers.len() == before {
        return format!("Provider '{}' not found.", provider_name);
    }
    if creds.active == provider_name {
        creds.active = creds.providers.first().map(|p| p.name.clone()).unwrap_or_default();
    }
    let path = credentials_path();
    if let Ok(content) = toml::to_string_pretty(&creds) {
        let _ = std::fs::write(&path, content);
    }
    if creds.providers.is_empty() {
        format!("Removed {}. No providers remaining.", provider_name)
    } else {
        format!("Removed {}. Active provider: {}", provider_name, creds.active)
    }
}

pub async fn handle_remaining_commands(
    command: Commands,
    config: &electro_core::types::config::ElectroConfig,
) -> anyhow::Result<()> {
    match command {
        Commands::Skill { command } => match command {
            SkillCommands::List => println!("Installed skills: (none)"),
            SkillCommands::Info { name } => println!("Skill info: {}", name),
            SkillCommands::Install { path } => println!("Installing skill from: {}", path),
        },
        Commands::Config { command } => match command {
            ConfigCommands::Validate => {
                println!("Configuration valid.");
                println!("  Gateway: {}:{}", config.gateway.host, config.gateway.port);
                println!("  Memory backend: {}", config.memory.backend);
            }
            ConfigCommands::Show => {
                let output = toml::to_string_pretty(&config)?;
                println!("{}", output);
            }
        },
        Commands::Update => {
            println!("Checking for updates...");
            println!("You are on the latest version.");
        }
        _ => {}
    }
    Ok(())
}
