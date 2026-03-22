use std::path::PathBuf;
use std::sync::OnceLock;

use crate::types::error::ElectroError;

static ELECTRO_HOME_CACHE: OnceLock<PathBuf> = OnceLock::new();

const ENV_ELECTRO_HOME: &str = "ELECTRO_HOME";
const DEFAULT_ELECTRO_DIR: &str = ".electro";

pub fn electro_home() -> PathBuf {
    ELECTRO_HOME_CACHE
        .get()
        .cloned()
        .unwrap_or_else(compute_electro_home)
}

fn compute_electro_home() -> PathBuf {
    if let Some(env_path) = std::env::var_os(ENV_ELECTRO_HOME) {
        return PathBuf::from(env_path);
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(DEFAULT_ELECTRO_DIR);
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(DEFAULT_ELECTRO_DIR)
}

pub fn ensure_electro_home() -> Result<PathBuf, ElectroError> {
    let path = electro_home();

    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| {
            ElectroError::Config(format!("Failed to create electro home directory: {}", e))
        })?;
    }

    Ok(path)
}

pub fn config_file() -> PathBuf {
    electro_home().join("electro.toml")
}

pub fn agent_config_file() -> PathBuf {
    electro_home().join("agent.toml")
}

pub fn credentials_file() -> PathBuf {
    electro_home().join("credentials.toml")
}

pub fn pid_file() -> PathBuf {
    electro_home().join("electro.pid")
}

pub fn oauth_file() -> PathBuf {
    electro_home().join("oauth.json")
}

pub fn vault_file() -> PathBuf {
    electro_home().join("vault.json")
}

pub fn vault_key_file() -> PathBuf {
    electro_home().join("vault.key")
}

pub fn workspace_dir() -> PathBuf {
    electro_home().join("workspace")
}

pub fn sessions_dir() -> PathBuf {
    electro_home().join("sessions")
}

pub fn custom_tools_dir() -> PathBuf {
    electro_home().join("custom-tools")
}

pub fn allowlist_file() -> PathBuf {
    electro_home().join("allowlist.txt")
}

pub fn allowlist_toml_file() -> PathBuf {
    electro_home().join("allowlist.toml")
}

pub fn slack_allowlist_file() -> PathBuf {
    electro_home().join("slack_allowlist.txt")
}

pub fn slack_allowlist_toml_file() -> PathBuf {
    electro_home().join("slack_allowlist.toml")
}

pub fn discord_allowlist_file() -> PathBuf {
    electro_home().join("discord_allowlist.txt")
}

pub fn discord_allowlist_toml_file() -> PathBuf {
    electro_home().join("discord_allowlist.toml")
}

pub fn mcp_config_file() -> PathBuf {
    electro_home().join("mcp.toml")
}

pub fn hive_db_file() -> PathBuf {
    electro_home().join("hive.db")
}

pub fn log_file() -> PathBuf {
    electro_home().join("electro.log")
}

pub fn tui_log_file() -> PathBuf {
    electro_home().join("tui.log")
}

pub fn backup_dir(timestamp: &str) -> PathBuf {
    electro_home().join("backups").join(timestamp)
}
