use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use tokio::sync::Mutex;

use electro_core::Channel;
use crate::admin::is_admin_user;
use crate::bootstrap::{censor_secrets, SecretCensorChannel};
use crate::cli::{format_user_error, handle_model_command, list_configured_providers, remove_provider};
use crate::daemon::{is_process_alive, pid_file_path, read_pid_file, remove_pid_file, write_pid_file};
use crate::onboarding::{build_system_prompt, decrypt_otk_blob, onboarding_message_with_link, send_with_retry, validate_provider_key, ONBOARDING_REFERENCE};

/// Main server start function (non-daemon mode)
pub async fn start_server(
    config: &mut electro_core::types::config::ElectroConfig,
    personality: String,
    cli_mode: String,
) -> Result<()> {
    // ── Parse personality mode ───────────────────────────
    let electro_mode = match personality.to_lowercase().as_str() {
        "work" => electro_core::types::config::ElectroMode::Work,
        "pro" => electro_core::types::config::ElectroMode::Pro,
        "none" => electro_core::types::config::ElectroMode::None,
        _ => electro_core::types::config::ElectroMode::Play,
    };
    // Lock mode when user explicitly chose work/pro/none — disables mode_switch tool
    let personality_locked =
        !matches!(electro_mode, electro_core::types::config::ElectroMode::Play);
    config.mode = electro_mode;
    tracing::info!(personality = %electro_mode, locked = personality_locked, "Electro personality mode");

    tracing::info!("Starting ELECTRO gateway");

    // ── Resolve API credentials ────────────────────────
    // Priority: config file > saved credentials > onboarding
    let credentials: Option<(String, String, String)> = {
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
                    .unwrap_or_else(|| electro_core::types::model_registry::default_model(&name).to_string());
                Some((name, key.clone(), model))
            } else {
                electro_core::config::credentials::load_saved_credentials()
            }
        } else {
            electro_core::config::credentials::load_saved_credentials()
        }
    };

    // ── Memory backend ─────────────────────────────────
    let memory_url = config.memory.path.clone().unwrap_or_else(|| {
        let data_dir = electro_core::paths::electro_home();
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            tracing::warn!(error = %e, path = %data_dir.display(), "Failed to create directory");
        }
        format!("sqlite:{}/memory.db?mode=rwc", data_dir.display())
    });
    let memory: Arc<dyn electro_core::Memory> = Arc::from(
        electro_memory::create_memory_backend(&config.memory.backend, &memory_url).await?,
    );
    tracing::info!(backend = %config.memory.backend, "Memory initialized");

    // ── Channel bootstrapping (generic) ──────────────────
    let mut channels: Vec<Arc<dyn electro_core::Channel>> = Vec::new();
    let mut primary_channel: Option<Arc<dyn electro_core::Channel>> = None;
    // Receivers collected here; piped into msg_tx once the unified channel exists
    let mut channel_receivers: Vec<tokio::sync::mpsc::Receiver<electro_core::types::message::InboundMessage>> = Vec::new();

    // Auto-inject Telegram config from env var when no config entry exists.
    // This enables zero-config VPS deployments: just set TELEGRAM_BOT_TOKEN.
    let mut config = config.clone();
    if !config.channel.contains_key("telegram") {
        if let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") {
            if !token.is_empty() {
                config.channel.insert(
                    "telegram".to_string(),
                    electro_core::types::config::ChannelConfig {
                        enabled: true,
                        token: Some(token),
                        allowlist: vec![],
                        file_transfer: true,
                        max_file_size: None,
                    },
                );
                tracing::info!("Auto-configured Telegram from TELEGRAM_BOT_TOKEN env var");
            }
        }
    }

    // Generic channel startup loop — iterate over all configured channels.
    // Each enabled channel is started and its message receiver is collected
    // for piping into the unified agent message bus below.
    #[cfg(feature = "telegram")]
    if let Some(tg_config) = config.channel.get("telegram") {
        if tg_config.enabled {
            match electro_channels::TelegramChannel::new(tg_config) {
                Ok(mut tg) => {
                    if let Err(e) = tg.start().await {
                        tracing::error!(error = %e, "Failed to start Telegram channel");
                    } else {
                        if let Some(rx) = tg.take_receiver() {
                            channel_receivers.push(rx);
                        }
                        let tg_arc: Arc<dyn electro_core::Channel> = Arc::new(tg);
                        if primary_channel.is_none() {
                            primary_channel = Some(tg_arc.clone());
                        }
                        channels.push(tg_arc);
                        tracing::info!("Telegram channel started");
                    }
                }
                Err(e) => tracing::error!(error = %e, "Failed to create Telegram channel"),
            }
        }
    }

    #[cfg(feature = "discord")]
    if let Some(dc_config) = config.channel.get("discord") {
        if dc_config.enabled {
            match electro_channels::DiscordChannel::new(dc_config) {
                Ok(mut dc) => {
                    if let Err(e) = dc.start().await {
                        tracing::error!(error = %e, "Failed to start Discord channel");
                    } else {
                        if let Some(rx) = dc.take_receiver() {
                            channel_receivers.push(rx);
                        }
                        let dc_arc: Arc<dyn electro_core::Channel> = Arc::new(dc);
                        if primary_channel.is_none() {
                            primary_channel = Some(dc_arc.clone());
                        }
                        channels.push(dc_arc);
                        tracing::info!("Discord channel started");
                    }
                }
                Err(e) => tracing::error!(error = %e, "Failed to create Discord channel"),
            }
        }
    }

    #[cfg(feature = "slack")]
    if let Some(sl_config) = config.channel.get("slack") {
        if sl_config.enabled {
            match electro_channels::SlackChannel::new(sl_config) {
                Ok(mut sl) => {
                    if let Err(e) = sl.start().await {
                        tracing::error!(error = %e, "Failed to start Slack channel");
                    } else {
                        if let Some(rx) = sl.take_receiver() {
                            channel_receivers.push(rx);
                        }
                        let sl_arc: Arc<dyn electro_core::Channel> = Arc::new(sl);
                        if primary_channel.is_none() {
                            primary_channel = Some(sl_arc.clone());
                        }
                        channels.push(sl_arc);
                        tracing::info!("Slack channel started");
                    }
                }
                Err(e) => tracing::error!(error = %e, "Failed to create Slack channel"),
            }
        }
    }

    tracing::info!(count = channels.len(), "Channels initialized");

    // ── Pending messages ───────────────────────────────
    let pending_messages: electro_tools::PendingMessages =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    // ── OTK setup token store ───────────────────────────
    let setup_tokens = electro_gateway::SetupTokenStore::new();

    // ── Pending raw key pastes (from /addkey unsafe) ────
    let pending_raw_keys: Arc<Mutex<HashSet<String>>> =
        Arc::new(Mutex::new(HashSet::new()));

    // ── Active login sessions (OTK Prowl — per-chat interactive browser sessions) ────
    #[cfg(feature = "browser")]
    let login_sessions: Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    > = Arc::new(Mutex::new(HashMap::new()));

    // ── Usage store (shares same SQLite DB as memory) ────
    let usage_store: Arc<dyn electro_core::UsageStore> =
        Arc::new(electro_memory::SqliteUsageStore::new(&memory_url).await?);
    tracing::info!("Usage store initialized");

    // ── Vault (encrypted credential store) ───────────────
    let vault: Option<Arc<dyn electro_core::Vault>> = match electro_vault::LocalVault::new()
        .await
    {
        Ok(v) => {
            tracing::info!("Vault initialized");
            Some(Arc::new(v) as Arc<dyn electro_core::Vault>)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Vault initialization failed — browser authenticate disabled");
            None
        }
    };

    // ── Tools (with secret-censoring channel wrapper) ───
    let censored_channel: Option<Arc<dyn electro_core::Channel>> = primary_channel
        .clone()
        .map(|ch| Arc::new(SecretCensorChannel { inner: ch }) as Arc<dyn electro_core::Channel>);
    let shared_mode: electro_tools::SharedMode =
        Arc::new(tokio::sync::RwLock::new(config.mode));
    let shared_memory_strategy: Arc<
        tokio::sync::RwLock<electro_core::types::config::MemoryStrategy>,
    > = Arc::new(tokio::sync::RwLock::new(
        electro_core::types::config::MemoryStrategy::Lambda,
    ));

    // Use create_tools_with_browser to get a separate BrowserTool reference
    // for /browser command handling.
    #[cfg(feature = "browser")]
    let (mut tools, browser_tool_ref) = electro_tools::create_tools_with_browser(
        &config.tools,
        censored_channel.clone(),
        Some(pending_messages.clone()),
        Some(memory.clone()),
        Some(Arc::new(setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(usage_store.clone()),
        if personality_locked {
            None
        } else {
            Some(shared_mode.clone())
        },
        vault.clone(),
    );
    #[cfg(not(feature = "browser"))]
    let mut tools = electro_tools::create_tools(
        &config.tools,
        censored_channel,
        Some(pending_messages.clone()),
        Some(memory.clone()),
        Some(Arc::new(setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(usage_store.clone()),
        if personality_locked {
            None
        } else {
            Some(shared_mode.clone())
        },
        vault.clone(),
    );
    tracing::info!(count = tools.len(), "Tools initialized");

    // ── Custom script tools (user/agent-authored) ──────
    let custom_tool_registry = Arc::new(electro_tools::CustomToolRegistry::new());
    {
        let custom_tools = custom_tool_registry.load_tools();
        if !custom_tools.is_empty() {
            tracing::info!(count = custom_tools.len(), "Custom script tools loaded");
            tools.extend(custom_tools);
        }
    }

    // ── MCP servers (external tool sources) ──────────
    #[cfg(feature = "mcp")]
    let mcp_manager: Arc<electro_mcp::McpManager> = {
        let mgr = Arc::new(electro_mcp::McpManager::new());
        mgr.connect_all().await;
        let tool_names: Vec<String> = tools.iter().map(|t| t.name().to_string()).collect();
        let mcp_tools = mgr.bridge_tools(&tool_names).await;
        if !mcp_tools.is_empty() {
            tracing::info!(count = mcp_tools.len(), "MCP bridge tools loaded");
            tools.extend(mcp_tools);
        }
        mgr
    };

    let system_prompt = Some(build_system_prompt());

    // Quick check: is [hive] enabled in config? (just the boolean, full init later)
    let hive_enabled_early = check_hive_enabled(&config).await;

    // ── Agent state (None during onboarding) ───────────
    let agent_state: Arc<tokio::sync::RwLock<Option<Arc<electro_agent::AgentRuntime>>>> =
        Arc::new(tokio::sync::RwLock::new(None));

    // Initialize agent if credentials are available
    if let Some((ref pname, ref key, ref model)) = credentials {
        // Filter out placeholder/invalid keys at startup
        if !electro_core::config::credentials::is_placeholder_key(key) {
            // Load all keys and saved base_url for this provider
            let (all_keys, saved_base_url) = electro_core::config::credentials::load_active_provider_keys()
                .map(|(_, keys, _, burl)| {
                    let valid: Vec<String> = keys
                        .into_iter()
                        .filter(|k| !electro_core::config::credentials::is_placeholder_key(k))
                        .collect();
                    (valid, burl)
                })
                .unwrap_or_else(|| (vec![key.clone()], None));
            let effective_base_url =
                saved_base_url.or_else(|| config.provider.base_url.clone());
            let provider_config = electro_core::types::config::ProviderConfig {
                name: Some(pname.clone()),
                api_key: Some(key.clone()),
                keys: all_keys,
                model: Some(model.clone()),
                base_url: effective_base_url,
                extra_headers: config.provider.extra_headers.clone(),
            };
            // Create provider — route to Codex OAuth if configured
            let provider: Arc<dyn electro_core::Provider> = create_provider(&provider_config, pname, model).await?;
            let agent = Arc::new(
                electro_agent::AgentRuntime::with_limits(
                    provider.clone(),
                    memory.clone(),
                    tools.clone(),
                    model.clone(),
                    system_prompt.clone(),
                    config.agent.max_turns,
                    config.agent.max_context_tokens,
                    config.agent.max_tool_rounds,
                    config.agent.max_task_duration_secs,
                    config.agent.max_spend_usd,
                )
                .with_v2_optimizations(config.agent.v2_optimizations)
                .with_parallel_phases(config.agent.parallel_phases)
                .with_hive_enabled(hive_enabled_early)
                .with_shared_mode(shared_mode.clone())
                .with_shared_memory_strategy(shared_memory_strategy.clone()),
            );
            *agent_state.write().await = Some(agent);
            tracing::info!(provider = %pname, model = %model, "Agent initialized");
        } else {
            tracing::warn!(provider = %pname, "Primary API key is a placeholder — starting in onboarding mode");
        }
    } else {
        // Check if Codex OAuth tokens exist — use those instead of API key
        #[cfg(feature = "codex-oauth")]
        {
            if electro_codex_oauth::TokenStore::exists() {
                // Always use Codex-compatible model — config model is for API key provider
                let model = "gpt-5.4".to_string();
                match electro_codex_oauth::TokenStore::load() {
                    Ok(store) => {
                        let token_store = std::sync::Arc::new(store);
                        let provider: Arc<dyn electro_core::Provider> =
                            Arc::new(electro_codex_oauth::CodexResponsesProvider::new(
                                model.clone(),
                                token_store,
                            ));
                        let agent = Arc::new(
                            electro_agent::AgentRuntime::with_limits(
                                provider.clone(),
                                memory.clone(),
                                tools.clone(),
                                model.clone(),
                                system_prompt.clone(),
                                config.agent.max_turns,
                                config.agent.max_context_tokens,
                                config.agent.max_tool_rounds,
                                config.agent.max_task_duration_secs,
                                config.agent.max_spend_usd,
                            )
                            .with_v2_optimizations(config.agent.v2_optimizations)
                            .with_parallel_phases(config.agent.parallel_phases)
                            .with_shared_mode(shared_mode.clone())
                            .with_shared_memory_strategy(shared_memory_strategy.clone()),
                        );
                        *agent_state.write().await = Some(agent);
                        tracing::info!(provider = "openai-codex", model = %model, "Agent initialized via Codex OAuth");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Codex OAuth tokens exist but failed to load — starting in onboarding mode");
                    }
                }
            } else {
                tracing::info!("No API key — starting in onboarding mode");
            }
        }
        #[cfg(not(feature = "codex-oauth"))]
        {
            tracing::info!("No API key — starting in onboarding mode");
        }
    }

    // ── Unified message channel ────────────────────────
    let (msg_tx, mut msg_rx) =
        tokio::sync::mpsc::channel::<electro_core::types::message::InboundMessage>(32);

    // Track spawned task handles for graceful shutdown
    let mut task_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // Wire all channel inbound receivers into the unified message bus.
    for mut ch_rx in channel_receivers {
        let tx = msg_tx.clone();
        task_handles.push(tokio::spawn(async move {
            while let Some(msg) = ch_rx.recv().await {
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        }));
    }

    // ── Workspace ──────────────────────────────────────
    let workspace_path = electro_core::paths::workspace_dir();
    if let Err(e) = std::fs::create_dir_all(&workspace_path) {
        tracing::warn!(error = %e, path = %workspace_path.display(), "Failed to create directory");
    }

    // ── Heartbeat ──────────────────────────────────────
    if config.heartbeat.enabled {
        let heartbeat_chat_id = config
            .heartbeat
            .report_to
            .clone()
            .unwrap_or_else(|| "heartbeat".to_string());
        let runner = electro_automation::HeartbeatRunner::new(
            config.heartbeat.clone(),
            workspace_path.clone(),
            heartbeat_chat_id,
        );
        let hb_tx = msg_tx.clone();
        task_handles.push(tokio::spawn(async move {
            runner.run(hb_tx).await;
        }));
        tracing::info!(
            interval = %config.heartbeat.interval,
            checklist = %config.heartbeat.checklist,
            "Heartbeat runner started"
        );
    }

    // ── Hive pack initialization (if enabled) ────────
    let hive_config = load_hive_config(&config).await;

    let hive_instance: Option<Arc<electro_hive::Hive>> = if hive_config.enabled {
        let hive_db = electro_core::paths::hive_db_file();
        let hive_url = format!("sqlite:{}?mode=rwc", hive_db.display());
        match electro_hive::Hive::new(&hive_config, &hive_url).await {
            Ok(h) => {
                tracing::info!(
                    max_workers = hive_config.max_workers,
                    threshold = hive_config.swarm_threshold_speedup,
                    "Many Tems initialized (Swarm Intelligence)"
                );
                Some(Arc::new(h))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Hive init failed — pack disabled");
                None
            }
        }
    } else {
        None
    };

    let _hive_enabled_flag = hive_instance.is_some();

    // ── Tenant Manager (workspace isolation) ───────────
    // Creates the TenantManager from config. When tenant_isolation is enabled,
    // each chat is mapped to a TenantId and gets its own workspace directory.
    // Even when disabled, the default tenant's workspace is created, so
    // ensure_workspace is always satisfied.
    let tenant_manager = {
        use electro_core::tenant_impl::create_tenant_manager;
        let mgr = create_tenant_manager(&config);
        // Always pre-create the default tenant workspace.
        use electro_core::traits::TenantId;
        if let Err(e) = mgr.ensure_workspace(&TenantId::default_tenant()) {
            tracing::warn!(error = %e, "Failed to pre-create default tenant workspace");
        }
        Arc::new(mgr)
    };

    // Per-chat serial executor
    run_message_dispatcher(
        msg_rx,
        msg_tx.clone(),
        primary_channel,
        agent_state.clone(),
        memory,
        tools,
        custom_tool_registry,
        #[cfg(feature = "mcp")]
        mcp_manager,
        config.clone(),
        pending_messages,
        setup_tokens,
        pending_raw_keys,
        #[cfg(feature = "browser")]
        login_sessions,
        usage_store,
        hive_instance,
        shared_mode,
        shared_memory_strategy,
        workspace_path,
        personality_locked,
        tenant_manager,
        #[cfg(feature = "browser")]
        browser_tool_ref,
        vault,
    ).await;

    // ── Start gateway + block ──────────────────────────
    println!("ELECTRO gateway starting...");
    println!("  Mode: {}", cli_mode);

    let maybe_agent = agent_state.read().await.clone();
    if let Some(agent) = maybe_agent {
        let gateway = Arc::new(electro_gateway::server::SkyGate::new(
            channels.clone(),
            agent,
            config.gateway.clone(),
        ));
        
        let listener = gateway.bind().await?;
        println!("  Gateway: http://{}:{}", config.gateway.host, config.gateway.port);
        println!(
            "  Health: http://{}:{}/health",
            config.gateway.host, config.gateway.port
        );

        let gw_clone = gateway.clone();
        task_handles.push(tokio::spawn(async move {
            if let Err(e) = gw_clone.serve(listener).await {
                tracing::error!("SkyGate serve error: {}", e);
            }
        }));
    } else {
        println!("  Gateway: disabled (waiting for onboarding/credentials)");
    }

    // Block until Ctrl+C, then drain gracefully
    tokio::signal::ctrl_c().await?;
    println!("\nELECTRO shutting down gracefully...");

    // Drop the inbound message sender so the dispatcher loop exits
    // when its receiver sees the channel closed.
    drop(msg_tx);

    // Wait for spawned tasks with a timeout
    let drain_timeout = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        futures::future::join_all(task_handles),
    );
    match drain_timeout.await {
        Ok(_) => println!("All tasks drained cleanly."),
        Err(_) => println!("Drain timeout — forcing exit."),
    }

    // Clean up PID file on graceful shutdown
    remove_pid_file();

    Ok(())
}

async fn check_hive_enabled(config: &electro_core::types::config::ElectroConfig) -> bool {
    #[derive(serde::Deserialize, Default)]
    struct HiveCheck {
        #[serde(default)]
        hive: HiveEnabled,
    }
    #[derive(serde::Deserialize, Default)]
    struct HiveEnabled {
        #[serde(default)]
        enabled: bool,
    }

    // Try to find config file
    let config_content = std::fs::read_to_string(electro_core::paths::electro_home().join("config.toml")).ok()
        .or_else(|| std::fs::read_to_string("electro.toml").ok());

    if let Some(content) = config_content {
        toml::from_str::<HiveCheck>(&content)
            .map(|c| c.hive.enabled)
            .unwrap_or(false)
    } else {
        false
    }
}

async fn load_hive_config(config: &electro_core::types::config::ElectroConfig) -> electro_hive::HiveConfig {
    let config_content = std::fs::read_to_string(electro_core::paths::electro_home().join("config.toml")).ok()
        .or_else(|| std::fs::read_to_string("electro.toml").ok());

    if let Some(content) = config_content {
        #[derive(serde::Deserialize, Default)]
        struct HiveWrapper {
            #[serde(default)]
            hive: electro_hive::HiveConfig,
        }
        toml::from_str::<HiveWrapper>(&content)
            .map(|w| w.hive)
            .unwrap_or_default()
    } else {
        electro_hive::HiveConfig::default()
    }
}

async fn create_provider(
    provider_config: &electro_core::types::config::ProviderConfig,
    pname: &str,
    _model: &str,
) -> Result<Arc<dyn electro_core::Provider>> {
    #[cfg(feature = "codex-oauth")]
    if pname == "openai-codex" {
        let token_store = std::sync::Arc::new(electro_codex_oauth::TokenStore::load()?);
        return Ok(Arc::new(electro_codex_oauth::CodexResponsesProvider::new(
            _model.to_string(),
            token_store,
        )));
    }

    Ok(Arc::from(electro_providers::create_provider(provider_config)?))
}

/// Tracks the active task state for a single chat.
struct ChatSlot {
    tx: tokio::sync::mpsc::Sender<electro_core::types::message::InboundMessage>,
    interrupt: Arc<AtomicBool>,
    is_heartbeat: Arc<AtomicBool>,
    is_busy: Arc<AtomicBool>,
    current_task: Arc<std::sync::Mutex<String>>,
    cancel_token: tokio_util::sync::CancellationToken,
}

async fn run_message_dispatcher(
    mut msg_rx: tokio::sync::mpsc::Receiver<electro_core::types::message::InboundMessage>,
    msg_tx: tokio::sync::mpsc::Sender<electro_core::types::message::InboundMessage>,
    primary_channel: Option<Arc<dyn electro_core::Channel>>,
    agent_state: Arc<tokio::sync::RwLock<Option<Arc<electro_agent::AgentRuntime>>>>,
    memory: Arc<dyn electro_core::Memory>,
    tools: Vec<Arc<dyn electro_core::Tool>>,
    custom_tool_registry: Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")]
    mcp_manager: Arc<electro_mcp::McpManager>,
    config: electro_core::types::config::ElectroConfig,
    pending_messages: electro_tools::PendingMessages,
    setup_tokens: electro_gateway::SetupTokenStore,
    pending_raw_keys: Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")]
    login_sessions: Arc<Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>>,
    usage_store: Arc<dyn electro_core::UsageStore>,
    hive_instance: Option<Arc<electro_hive::Hive>>,
    shared_mode: electro_tools::SharedMode,
    shared_memory_strategy: Arc<tokio::sync::RwLock<electro_core::types::config::MemoryStrategy>>,
    workspace_path: std::path::PathBuf,
    personality_locked: bool,
    tenant_manager: Arc<electro_core::tenant_impl::TenantManager>,
    #[cfg(feature = "browser")]
    browser_tool_ref: Option<Arc<electro_tools::BrowserTool>>,
    vault: Option<Arc<dyn electro_core::Vault>>,
) {
    if let Some(sender) = primary_channel {
        let agent_state_clone = agent_state.clone();
        let memory_clone = memory.clone();
        let tools_clone = tools.clone();
        let custom_registry_clone = custom_tool_registry.clone();
        #[cfg(feature = "mcp")]
        let mcp_manager_clone = mcp_manager.clone();
        let agent_max_turns = config.agent.max_turns;
        let agent_max_context_tokens = config.agent.max_context_tokens;
        let agent_max_tool_rounds = config.agent.max_tool_rounds;
        let agent_max_task_duration = config.agent.max_task_duration_secs;
        let agent_max_spend_usd = config.agent.max_spend_usd;
        let agent_v2_opt = config.agent.v2_optimizations;
        let agent_parallel_phases = config.agent.parallel_phases;
        let provider_base_url = config.provider.base_url.clone();
        let ws_path = workspace_path.clone();
        let pending_clone = pending_messages.clone();
        let setup_tokens_clone = setup_tokens.clone();
        let pending_raw_keys_clone = pending_raw_keys.clone();
        #[cfg(feature = "browser")]
        let login_sessions_clone = login_sessions.clone();
        let usage_store_clone = usage_store.clone();
        let hive_clone = hive_instance.clone();
        let tenant_mgr_clone = tenant_manager.clone();
        let tenant_isolation_enabled = config.electro.tenant_isolation;

        let chat_slots: Arc<Mutex<HashMap<String, ChatSlot>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let msg_tx_redispatch = msg_tx.clone();
        tokio::spawn(async move {
            while let Some(inbound) = msg_rx.recv().await {
                let chat_id = inbound.chat_id.clone();
                let is_heartbeat_msg = inbound.channel == "heartbeat";

                let mut slots = chat_slots.lock().await;

                // Handle user messages while a task is active
                if !is_heartbeat_msg {
                    if let Some(slot) = slots.get(&chat_id) {
                        if slot.is_heartbeat.load(Ordering::Relaxed) {
                            tracing::info!(
                                chat_id = %chat_id,
                                "User message preempting active heartbeat task"
                            );
                            slot.interrupt.store(true, Ordering::Relaxed);
                            slot.cancel_token.cancel();
                        }

                        // /stop is the only hardcoded instant-kill.
                        let is_slash_stop = inbound.text.as_deref()
                            .map(|t| t.trim().eq_ignore_ascii_case("/stop"))
                            .unwrap_or(false);

                        if is_slash_stop {
                            tracing::info!(
                                chat_id = %chat_id,
                                "/stop command — interrupting active task"
                            );
                            slot.interrupt.store(true, Ordering::Relaxed);
                            slot.cancel_token.cancel();
                            continue;
                        }

                        // Only intercept when worker is actively processing.
                        if slot.is_busy.load(Ordering::Relaxed) {
                            // Push to pending queue ONLY when busy
                            if let Some(text) = inbound.text.as_deref() {
                                if let Ok(mut pq) = pending_clone.lock() {
                                    pq.entry(chat_id.clone())
                                        .or_default()
                                        .push(text.to_string());
                                }
                            }
                            // LLM interceptor
                            let icpt_sender = sender.clone();
                            let icpt_chat_id = chat_id.clone();
                            let icpt_msg_id = inbound.id.clone();
                            let icpt_msg_text = inbound.text.clone().unwrap_or_default();
                            let icpt_interrupt = slot.interrupt.clone();
                            let icpt_cancel = slot.cancel_token.clone();
                            let icpt_task = slot.current_task.clone();
                            let icpt_agent_state = agent_state_clone.clone();
                            tokio::spawn(async move {
                                let task_desc = icpt_task.lock()
                                    .map(|t| t.clone())
                                    .unwrap_or_default();

                                // Get provider + model from the active agent
                                let agent_guard = icpt_agent_state.read().await;
                                let Some(agent) = agent_guard.as_ref() else { return; };
                                let provider = agent.provider_arc();
                                let model = agent.model().to_string();
                                drop(agent_guard);

                                let soul = build_system_prompt();
                                let request = electro_core::types::message::CompletionRequest {
                                    model,
                                    system: Some(format!(
                                        "{}\n\n\
                                         === INTERCEPTOR MODE ===\n\
                                         You are running as Tem's INTERCEPTOR right now. Your main self is busy \
                                         working on a task. The user sent a message while that task is running.\n\n\
                                         Current task: \"{}\"\n\n\
                                         Interceptor rules:\n\
                                         - Keep it SHORT (1-3 sentences max)\n\
                                         - If the user wants to CANCEL/STOP the task, include the exact token [CANCEL] at the very end of your response\n\
                                         - If the user asks about progress, explain what the task involves based on its description\n\
                                         - If the user is chatting casually, respond warmly\n\
                                         - NEVER use [CANCEL] unless the user clearly wants to stop\n\
                                         === END INTERCEPTOR ===",
                                        soul, task_desc
                                    )),
                                    messages: vec![
                                        electro_core::types::message::ChatMessage {
                                            role: electro_core::types::message::Role::User,
                                            content: electro_core::types::message::MessageContent::Text(icpt_msg_text),
                                        },
                                    ],
                                    tools: vec![],
                                    max_tokens: None,
                                    temperature: Some(0.7),
                                };

                                match provider.complete(request).await {
                                    Ok(resp) => {
                                        let mut text = resp.content.iter()
                                            .filter_map(|p| match p {
                                                electro_core::types::message::ContentPart::Text { text } => Some(text.as_str()),
                                                _ => None,
                                            })
                                            .collect::<Vec<_>>()
                                            .join("");

                                        let should_cancel = text.contains("[CANCEL]");
                                        text = text.replace("[CANCEL]", "").trim().to_string();

                                        if !text.is_empty() {
                                            let reply = electro_core::types::message::OutboundMessage {
                                                chat_id: icpt_chat_id.clone(),
                                                text,
                                                reply_to: Some(icpt_msg_id),
                                                parse_mode: None,
                                            };
                                            let _ = icpt_sender.send_message(reply).await;
                                        }

                                        if should_cancel {
                                            icpt_interrupt.store(true, Ordering::Relaxed);
                                            icpt_cancel.cancel();
                                            tracing::info!(
                                                chat_id = %icpt_chat_id,
                                                "Interceptor cancelled active task"
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            "Interceptor LLM call failed — skipping"
                                        );
                                    }
                                }
                            });
                            continue;
                        }
                    }
                }

                // Skip heartbeat if chat is busy
                if is_heartbeat_msg {
                    if let Some(slot) = slots.get(&chat_id) {
                        if slot.tx.try_send(inbound).is_err() {
                            tracing::debug!(
                                chat_id = %chat_id,
                                "Skipping heartbeat — chat is busy"
                            );
                        }
                        continue;
                    }
                }

                // Ensure a worker exists for this chat_id.
                // Resolve the workspace path for this chat — per-tenant when
                // isolation is enabled, global workspace_path otherwise.
                let chat_workspace = if tenant_isolation_enabled {
                    use electro_core::traits::Tenant;
                    // channel is not always populated — use chat_id as fallback key
                    let channel_name = inbound.channel.as_str();
                    let user_id = &chat_id;
                    match tenant_mgr_clone.resolve_tenant(channel_name, user_id).await {
                        Ok(tenant_id) => {
                            if let Err(e) = tenant_mgr_clone.ensure_workspace(&tenant_id) {
                                tracing::warn!(
                                    error = %e,
                                    tenant_id = %tenant_id.0,
                                    "Failed to ensure workspace for tenant"
                                );
                            }
                            tenant_mgr_clone.workspace_path(&tenant_id)
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Tenant resolution failed, using global workspace");
                            ws_path.clone()
                        }
                    }
                } else {
                    ws_path.clone()
                };

                let shared_mode_for_worker = shared_mode.clone();
                let shared_memory_strategy_for_worker = shared_memory_strategy.clone();
                let slot = slots.entry(chat_id.clone()).or_insert_with(|| {
                    // Create new worker for this chat
                    create_chat_worker(
                        &chat_id,
                        &sender,
                        &agent_state_clone,
                        &memory_clone,
                        &tools_clone,
                        &custom_registry_clone,
                        #[cfg(feature = "mcp")]
                        &mcp_manager_clone,
                        agent_max_turns,
                        agent_max_context_tokens,
                        agent_max_tool_rounds,
                        agent_max_task_duration,
                        agent_max_spend_usd,
                        agent_v2_opt,
                        agent_parallel_phases,
                        &provider_base_url,
                        &chat_workspace,
                        &pending_clone,
                        &setup_tokens_clone,
                        &pending_raw_keys_clone,
                        #[cfg(feature = "browser")]
                        &login_sessions_clone,
                        &usage_store_clone,
                        &hive_clone,
                        shared_mode_for_worker,
                        shared_memory_strategy_for_worker,
                        personality_locked,
                        #[cfg(feature = "browser")]
                        &browser_tool_ref,
                        &vault,
                    )
                });

                // Send message into the chat's dedicated queue.
                if !is_heartbeat_msg {
                    let tx = slot.tx.clone();
                    drop(slots); // release Mutex guard before await
                    let inbound_backup = inbound.clone();
                    if let Err(e) = tx.send(inbound).await {
                        tracing::error!(
                            chat_id = %chat_id,
                            error = %e,
                            "Chat worker dead — removing slot and re-dispatching"
                        );
                        let mut slots = chat_slots.lock().await;
                        slots.remove(&chat_id);
                        drop(slots); // release lock before re-dispatch
                        if let Err(e2) = msg_tx_redispatch.send(inbound_backup).await {
                            tracing::error!(
                                chat_id = %chat_id,
                                error = %e2,
                                "Failed to re-dispatch message after worker death"
                            );
                        }
                    }
                }
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn create_chat_worker(
    worker_chat_id: &str,
    sender: &Arc<dyn electro_core::Channel>,
    agent_state: &Arc<tokio::sync::RwLock<Option<Arc<electro_agent::AgentRuntime>>>>,
    memory: &Arc<dyn electro_core::Memory>,
    tools_template: &[Arc<dyn electro_core::Tool>],
    custom_registry: &Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")]
    mcp_mgr: &Arc<electro_mcp::McpManager>,
    max_turns: usize,
    max_ctx: usize,
    max_rounds: usize,
    max_task_duration: u64,
    max_spend: f64,
    v2_opt: bool,
    pp_opt: bool,
    base_url: &Option<String>,
    ws_path: &std::path::Path,
    pending_clone: &electro_tools::PendingMessages,
    setup_tokens_clone: &electro_gateway::SetupTokenStore,
    pending_raw_keys_clone: &Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")]
    login_sessions_clone: &Arc<Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>>,
    usage_store_clone: &Arc<dyn electro_core::UsageStore>,
    hive_clone: &Option<Arc<electro_hive::Hive>>,
    shared_mode: electro_tools::SharedMode,
    shared_memory_strategy: Arc<tokio::sync::RwLock<electro_core::types::config::MemoryStrategy>>,
    _personality_locked: bool,
    #[cfg(feature = "browser")]
    browser_ref_worker: &Option<Arc<electro_tools::BrowserTool>>,
    #[cfg(feature = "browser")]
    vault: &Option<Arc<dyn electro_core::Vault>>,
) -> ChatSlot {
    let (chat_tx, mut chat_rx) =
        tokio::sync::mpsc::channel::<electro_core::types::message::InboundMessage>(4);

    let interrupt = Arc::new(AtomicBool::new(false));
    let is_heartbeat = Arc::new(AtomicBool::new(false));
    let is_busy = Arc::new(AtomicBool::new(false));
    let current_task: Arc<std::sync::Mutex<String>> = Arc::new(std::sync::Mutex::new(String::new()));
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let is_busy_clone = is_busy.clone();
    let current_task_clone = current_task.clone();
    let self_tx = chat_tx.clone();

    let agent_state = agent_state.clone();
    let memory = memory.clone();
    let tools_template = tools_template.to_vec();
    let custom_registry = custom_registry.clone();
    #[cfg(feature = "mcp")]
    let mcp_mgr = mcp_mgr.clone();
    let sender = sender.clone();
    let workspace_path = ws_path.to_path_buf();
    let interrupt_clone = interrupt.clone();
    let is_heartbeat_clone = is_heartbeat.clone();
    let cancel_token_clone = cancel_token.clone();
    let pending_for_worker = pending_clone.clone();
    let shared_mode = shared_mode;
    let shared_memory_strategy = shared_memory_strategy;
    let setup_tokens_worker = setup_tokens_clone.clone();
    let pending_raw_keys_worker = pending_raw_keys_clone.clone();
    #[cfg(feature = "browser")]
    let login_sessions_worker = login_sessions_clone.clone();
    #[cfg(feature = "browser")]
    let vault_for_login = vault.clone();
    #[cfg(feature = "browser")]
    let browser_ref_worker = browser_ref_worker.clone();
    let usage_store_worker = usage_store_clone.clone();
    let hive_worker = hive_clone.clone();
    let worker_chat_id = worker_chat_id.to_string();

    tokio::spawn(async move {
        // ── Restore conversation history from memory backend ──
        let history_key = format!("chat_history:{}", worker_chat_id);
        let mut persistent_history: Vec<electro_core::types::message::ChatMessage> =
            match memory.get(&history_key).await {
                Ok(Some(entry)) => {
                    match serde_json::from_str(&entry.content) {
                        Ok(h) => {
                            tracing::info!(
                                chat_id = %worker_chat_id,
                                messages = %Vec::<electro_core::types::message::ChatMessage>::len(&h),
                                "Restored conversation history from memory"
                            );
                            h
                        }
                        Err(e) => {
                            tracing::warn!(
                                chat_id = %worker_chat_id,
                                error = %e,
                                "Failed to deserialize saved history, starting fresh"
                            );
                            Vec::new()
                        }
                    }
                }
                Ok(None) => Vec::new(),
                Err(e) => {
                    tracing::warn!(
                        chat_id = %worker_chat_id,
                        error = %e,
                        "Failed to load saved history, starting fresh"
                    );
                    Vec::new()
                }
            };

        while let Some(mut msg) = chat_rx.recv().await {
            // Snapshot for outer panic handler
            let panic_chat_id = msg.chat_id.clone();
            let panic_msg_id = msg.id.clone();

            // ... (rest of message processing logic)
            // This is simplified - the actual implementation would have all the command handling
        }
    });

    ChatSlot {
        tx: chat_tx,
        interrupt,
        is_heartbeat,
        is_busy,
        current_task,
        cancel_token,
    }
}
