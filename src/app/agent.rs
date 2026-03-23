use anyhow::Result;
use electro_agent::AgentRuntime;
use electro_core::config::credentials::{
    is_placeholder_key, load_active_provider_keys, load_saved_credentials,
};
use electro_core::types::config::{ElectroConfig, ElectroMode, ProviderConfig};
use electro_core::types::model_registry::default_model;
use electro_core::{Memory, Provider, Tool};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn create_agent(
    config: &ElectroConfig,
    provider: Arc<dyn Provider>,
    memory: Arc<dyn Memory>,
    tools: Vec<Arc<dyn Tool>>,
    model: String,
    system_prompt: Option<String>,
    hive_enabled: bool,
    shared_mode: Arc<RwLock<ElectroMode>>,
    shared_memory_strategy: Arc<RwLock<electro_core::types::config::MemoryStrategy>>,
) -> AgentRuntime {
    AgentRuntime::with_limits(
        provider,
        memory,
        tools,
        model,
        system_prompt,
        config.agent.max_turns,
        config.agent.max_context_tokens,
        config.agent.max_tool_rounds,
        config.agent.max_task_duration_secs,
        config.agent.max_spend_usd,
    )
    .with_v2_optimizations(config.agent.v2_optimizations)
    .with_parallel_phases(config.agent.parallel_phases)
    .with_hive_enabled(hive_enabled)
    .with_shared_mode(shared_mode)
    .with_shared_memory_strategy(shared_memory_strategy)
}

pub async fn create_provider(
    provider_config: &ProviderConfig,
    pname: &str,
    _model: &str,
) -> Result<Arc<dyn Provider>> {
    #[cfg(feature = "codex-oauth")]
    if pname == "openai-codex" {
        let token_store = std::sync::Arc::new(electro_codex_oauth::TokenStore::load()?);
        return Ok(Arc::new(electro_codex_oauth::CodexResponsesProvider::new(
            _model.to_string(),
            token_store,
        )));
    }

    Ok(Arc::from(electro_providers::create_provider(
        provider_config,
    )?))
}

pub fn resolve_credentials(config: &ElectroConfig) -> Option<(String, String, String)> {
    if let Some(ref key) = config.provider.api_key {
        if !key.is_empty() && !key.starts_with("${") {
            let name = config
                .provider
                .name
                .clone()
                .unwrap_or_else(|| "anthropic".to_string());
            let model = config
                .provider
                .model
                .clone()
                .unwrap_or_else(|| default_model(&name).to_string());
            return Some((name, key.clone(), model));
        }
    }

    load_saved_credentials()
}

pub fn build_provider_config(
    config: &ElectroConfig,
    pname: &str,
    key: &str,
    model: &str,
) -> ProviderConfig {
    let (all_keys, saved_base_url) = load_active_provider_keys()
        .map(|(_, keys, _, burl)| {
            let valid: Vec<String> = keys
                .into_iter()
                .filter(|candidate| !is_placeholder_key(candidate))
                .collect();
            (valid, burl)
        })
        .unwrap_or_else(|| (vec![key.to_string()], None));

    let effective_base_url = saved_base_url.or_else(|| config.provider.base_url.clone());

    ProviderConfig {
        name: Some(pname.to_string()),
        api_key: Some(key.to_string()),
        keys: all_keys,
        model: Some(model.to_string()),
        base_url: effective_base_url,
        extra_headers: config.provider.extra_headers.clone(),
    }
}
