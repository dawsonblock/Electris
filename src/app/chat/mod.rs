use crate::app::agent::{
    build_provider_config, create_agent, create_provider, resolve_credentials,
};
use crate::app::onboarding::{
    build_system_prompt, onboarding_message_with_link, ONBOARDING_REFERENCE,
};
use crate::app::server::dispatcher::run_message_dispatcher;
use crate::app::{check_hive_enabled, init_core_stack, init_tools};
use crate::bootstrap::SecretCensorChannel;
use anyhow::Result;
use electro_core::paths;
use electro_core::types::config::{ElectroConfig, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use electro_core::Channel;
use electro_runtime::RuntimeHandle;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn run_chat_mode(
    config: ElectroConfig,
    _config_path: Option<&std::path::Path>,
) -> Result<()> {
    println!("ELECTRO interactive chat");
    println!("Type '/quit' or '/exit' to quit.\n");

    let hive_enabled = check_hive_enabled().await;
    let core = init_core_stack(&config).await?;

    let workspace = paths::workspace_dir();
    std::fs::create_dir_all(&workspace).ok();

    let mut cli_channel = electro_channels::CliChannel::new(workspace.clone());
    let cli_rx = cli_channel
        .take_receiver()
        .expect("CLI channel receiver unavailable");
    cli_channel.start().await?;
    let cli_arc: Arc<dyn electro_core::Channel> = Arc::new(cli_channel);

    let pending_messages = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let shared_mode = Arc::new(tokio::sync::RwLock::new(config.mode));
    let shared_memory_strategy = Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda));
    let censored_cli: Arc<dyn Channel> = Arc::new(SecretCensorChannel {
        inner: cli_arc.clone(),
    });
    let (tools, browser_tool_ref) = init_tools(
        &config.tools,
        Some(censored_cli),
        Some(pending_messages.clone()),
        Some(core.memory.clone()),
        Some(Arc::new(core.setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(core.usage_store.clone()),
        Some(shared_mode.clone()),
        core.vault.clone(),
    );

    let (queue_tx, msg_rx) = tokio::sync::mpsc::channel::<InboundMessage>(32);
    let runtime = RuntimeHandle::new(
        queue_tx,
        shared_mode.clone(),
        shared_memory_strategy.clone(),
    );

    if let Some((provider_name, api_key, model)) = resolve_credentials(&config) {
        if !electro_core::config::credentials::is_placeholder_key(&api_key) {
            let provider_config = build_provider_config(&config, &provider_name, &api_key, &model);
            match create_provider(&provider_config, &provider_name, &model).await {
                Ok(provider) => {
                    let agent = create_agent(
                        &config,
                        provider,
                        core.memory.clone(),
                        tools.clone(),
                        model.clone(),
                        Some(build_system_prompt()),
                        hive_enabled,
                        runtime.shared_mode.clone(),
                        runtime.shared_memory_strategy.clone(),
                    )
                    .await;
                    runtime.set_agent(agent).await;
                    runtime.set_active_provider(provider_name.clone()).await;
                    println!("Connected to {} (model: {})", provider_name, model);
                }
                Err(error) => {
                    eprintln!("Failed to create provider: {error}");
                }
            }
        }
    } else {
        println!("No API key configured — running in onboarding mode.");
        let otk = core.setup_tokens.generate("cli").await;
        let link = format!(
            "https://electro-labs.github.io/electro/setup#{}",
            hex::encode(otk)
        );
        println!("\n{}", onboarding_message_with_link(&link));
        println!("\n{}", ONBOARDING_REFERENCE);
    }
    println!("---\n");

    let custom_tool_registry = Arc::new(electro_tools::CustomToolRegistry::new());
    #[cfg(feature = "mcp")]
    let mcp_manager = Arc::new(electro_mcp::McpManager::new());
    #[cfg(feature = "mcp")]
    mcp_manager.connect_all().await;

    run_message_dispatcher(
        msg_rx,
        Some(cli_arc.clone()),
        runtime.clone(),
        core.memory.clone(),
        tools,
        custom_tool_registry,
        #[cfg(feature = "mcp")]
        mcp_manager,
        config.clone(),
        pending_messages,
        core.setup_tokens.clone(),
        Arc::new(Mutex::new(HashSet::new())),
        #[cfg(feature = "browser")]
        Arc::new(Mutex::new(HashMap::new())),
        core.usage_store.clone(),
        None,
        workspace,
        false,
        Arc::new(electro_core::tenant_impl::create_tenant_manager(&config)),
        #[cfg(feature = "browser")]
        browser_tool_ref,
        core.vault.clone(),
    )
    .await;

    let mut cli_rx = cli_rx;
    while let Some(msg) = cli_rx.recv().await {
        if runtime.queue_tx.send(msg).await.is_err() {
            break;
        }
    }
    Ok(())
}
