//! Configuration loading utilities.

use crate::Config;
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// Load configuration from a TOML file.
pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Config> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

/// Load configuration from environment variables.
/// 
/// Environment variables should be prefixed with SPINE_ and use __ as separator.
/// Example: SPINE_SERVER__PORT=8080
pub fn load_from_env() -> Config {
    let mut config = Config::default();
    
    // Server settings
    if let Ok(port) = env::var("SPINE_SERVER__PORT") {
        if let Ok(port_num) = port.parse() {
            config.server.port = port_num;
        }
    }
    
    if let Ok(host) = env::var("SPINE_SERVER__HOST") {
        config.server.host = host;
    }
    
    // LLM settings
    if let Ok(provider) = env::var("SPINE_LLM__PROVIDER") {
        config.llm.provider = provider;
    }
    
    if let Ok(api_key) = env::var("SPINE_LLM__API_KEY") {
        config.llm.api_key = Some(api_key);
    }
    
    if let Ok(model) = env::var("SPINE_LLM__MODEL") {
        config.llm.model = model;
    }
    
    if let Ok(max_tokens) = env::var("SPINE_LLM__MAX_TOKENS") {
        if let Ok(tokens) = max_tokens.parse() {
            config.llm.max_tokens = tokens;
        }
    }
    
    // Worker settings
    if let Ok(policy) = env::var("SPINE_WORKER__POLICY") {
        config.worker.policy = policy;
    }
    
    // Logging settings
    if let Ok(level) = env::var("SPINE_LOGGING__LEVEL") {
        config.logging.level = level;
    }
    
    if let Ok(format) = env::var("SPINE_LOGGING__FORMAT") {
        config.logging.format = format;
    }
    
    config
}

/// Merge two configurations, with `other` taking precedence.
pub fn merge_configs(base: Config, other: Config) -> Config {
    Config {
        server: ServerConfig {
            port: other.server.port,
            host: other.server.host,
            max_body_size_mb: other.server.max_body_size_mb,
            request_timeout_secs: other.server.request_timeout_secs,
        },
        llm: LlmConfig {
            provider: other.llm.provider,
            api_key: other.llm.api_key.or(base.llm.api_key),
            model: other.llm.model,
            max_tokens: other.llm.max_tokens,
            temperature: other.llm.temperature,
            timeout_secs: other.llm.timeout_secs,
            provider_settings: merge_maps(base.llm.provider_settings, other.llm.provider_settings),
        },
        worker: WorkerConfig {
            policy: other.worker.policy,
            limits: ResourceLimitsConfig {
                memory_mb: other.worker.limits.memory_mb,
                cpu_time_secs: other.worker.limits.cpu_time_secs,
                output_kb: other.worker.limits.output_kb,
                max_file_size_mb: other.worker.limits.max_file_size_mb,
            },
            max_concurrent: other.worker.max_concurrent,
        },
        logging: LoggingConfig {
            level: other.logging.level,
            format: other.logging.format,
            file_logging: other.logging.file_logging,
            log_file: other.logging.log_file.or(base.logging.log_file),
        },
        extra: merge_maps(base.extra, other.extra),
    }
}

/// Merge two hash maps, with `b` taking precedence.
fn merge_maps<K, V>(a: HashMap<K, V>, b: HashMap<K, V>) -> HashMap<K, V>
where
    K: std::hash::Hash + Eq,
{
    let mut result = a;
    result.extend(b);
    result
}

// Import types for merge_configs
use crate::{LoggingConfig, LlmConfig, ResourceLimitsConfig, ServerConfig, WorkerConfig};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_from_env_sets_port() {
        env::set_var("SPINE_SERVER__PORT", "9090");
        let config = load_from_env();
        assert_eq!(config.server.port, 9090);
        env::remove_var("SPINE_SERVER__PORT");
    }

    #[test]
    fn load_from_env_sets_api_key() {
        env::set_var("SPINE_LLM__API_KEY", "test-api-key");
        let config = load_from_env();
        assert_eq!(config.llm.api_key, Some("test-api-key".to_string()));
        env::remove_var("SPINE_LLM__API_KEY");
    }

    #[test]
    fn merge_configs_takes_other_values() {
        let base = Config::default();
        let mut other = Config::default();
        other.server.port = 9090;
        other.llm.model = "gpt-3.5".to_string();
        
        let merged = merge_configs(base, other);
        assert_eq!(merged.server.port, 9090);
        assert_eq!(merged.llm.model, "gpt-3.5");
    }
}
