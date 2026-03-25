//! Configuration management for the spine runtime.
//!
//! Supports loading from files, environment variables, and defaults.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

mod loader;

pub use loader::{load_from_file, load_from_env, merge_configs};

/// Main configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// LLM provider configuration
    pub llm: LlmConfig,
    /// Worker configuration
    pub worker: WorkerConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
    /// Additional custom settings
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Port to listen on
    pub port: u16,
    /// Host to bind to
    pub host: String,
    /// Maximum request body size in MB
    pub max_body_size_mb: usize,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            host: "0.0.0.0".to_string(),
            max_body_size_mb: 10,
            request_timeout_secs: 30,
        }
    }
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Default provider to use
    pub provider: String,
    /// API key (should be loaded from env)
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    /// Model to use
    pub model: String,
    /// Maximum tokens per request
    pub max_tokens: usize,
    /// Temperature (0.0 - 2.0)
    pub temperature: f32,
    /// Timeout in seconds
    pub timeout_secs: u64,
    /// Provider-specific settings
    #[serde(flatten)]
    pub provider_settings: HashMap<String, serde_json::Value>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            api_key: None,
            model: "gpt-4".to_string(),
            max_tokens: 4000,
            temperature: 0.7,
            timeout_secs: 60,
            provider_settings: HashMap::new(),
        }
    }
}

/// Worker execution configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Execution policy: permissive, standard, restrictive
    pub policy: String,
    /// Resource limits
    pub limits: ResourceLimitsConfig,
    /// Maximum concurrent executions
    pub max_concurrent: usize,
}

/// Resource limits configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitsConfig {
    /// Max memory in MB
    pub memory_mb: u64,
    /// Max CPU time in seconds
    pub cpu_time_secs: u64,
    /// Max output size in KB
    pub output_kb: u64,
    /// Max file size in MB
    pub max_file_size_mb: u64,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            policy: "standard".to_string(),
            limits: ResourceLimitsConfig {
                memory_mb: 512,
                cpu_time_secs: 60,
                output_kb: 1024,
                max_file_size_mb: 100,
            },
            max_concurrent: 10,
        }
    }
}

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level: trace, debug, info, warn, error
    pub level: String,
    /// Log format: json, pretty, compact
    pub format: String,
    /// Enable file logging
    pub file_logging: bool,
    /// Log file path (if file_logging enabled)
    pub log_file: Option<String>,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            file_logging: false,
            log_file: None,
        }
    }
}


impl Config {
    /// Load configuration from default sources.
    /// 
    /// Order of precedence (highest to lowest):
    /// 1. Environment variables (SPINE_*)
    /// 2. Config file (spine.toml)
    /// 3. Defaults
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Config::default();
        
        // Try to load from config file
        if let Ok(file_config) = load_from_file("spine.toml") {
            config = merge_configs(config, file_config);
        }
        
        // Override with environment variables
        let env_config = load_from_env();
        config = merge_configs(config, env_config);
        
        Ok(config)
    }
    
    /// Get the configured port.
    pub fn port(&self) -> u16 {
        self.server.port
    }
    
    /// Check if LLM is configured.
    pub fn llm_enabled(&self) -> bool {
        self.llm.api_key.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_reasonable_values() {
        let config = Config::default();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.llm.model, "gpt-4");
        assert_eq!(config.worker.policy, "standard");
    }

    #[test]
    fn llm_disabled_without_api_key() {
        let config = Config::default();
        assert!(!config.llm_enabled());
    }

    #[test]
    fn llm_enabled_with_api_key() {
        let mut config = Config::default();
        config.llm.api_key = Some("test-key".to_string());
        assert!(config.llm_enabled());
    }
}
