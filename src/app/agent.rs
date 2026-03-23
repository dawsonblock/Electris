use anyhow::Result;
use electro_agent::AgentRuntime;
use electro_core::types::config::{ElectroConfig, ProviderConfig, ElectroMode};
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
