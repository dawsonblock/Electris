use crate::app::agent::{build_provider_config, resolve_credentials};
use crate::app::onboarding::build_system_prompt;
use crate::app::server::dispatcher::run_message_dispatcher;
use crate::app::{create_agent, create_provider, init_core_stack, init_tools, load_hive_config};
use crate::bootstrap::SecretCensorChannel;
use crate::daemon::remove_pid_file;
use anyhow::Result;
use electro_core::traits::Observable;
use electro_core::types::config::{ElectroConfig, ElectroMode, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use electro_core::Channel;
use electro_tools::policy::{set_runtime_policy, ToolPolicy};
use electro_runtime::{RuntimeConfig, RuntimeHandle, ToolPolicyConfig};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod commands;
pub mod dispatcher;
pub mod scheduler;
pub mod slot;
pub mod worker;

pub async fn start_server(
    config: &mut ElectroConfig,
    personality: String,
    _cli_mode: String,
) -> Result<()> {
    // ── Parse personality mode ──
    let electro_mode = match personality.to_lowercase().as_str() {
        "work" => ElectroMode::Work,
        "pro" => ElectroMode::Pro,
        "none" => ElectroMode::None,
        _ => ElectroMode::Play,
    };
    let personality_locked = !matches!(electro_mode, ElectroMode::Play);
    config.mode = electro_mode;
    tracing::info!(personality = %electro_mode, locked = personality_locked, "Electro personality mode");

    tracing::info!("Starting ELECTRO gateway");

    // ── Core Stack ──
    let core = init_core_stack(config).await?;
    let observable: Option<Arc<dyn Observable>> =
        match electro_observable::create_observable(&config.observability) {
            Ok(observable) => Some(Arc::from(observable)),
            Err(error) => {
                tracing::warn!(error = %error, "failed to initialize observability");
                None
            }
        };

    // ── Channel bootstrapping ──
    let mut channels: Vec<Arc<dyn Channel>> = Vec::new();
    let mut primary_channel: Option<Arc<dyn Channel>> = None;
    let mut channel_receivers = Vec::new();

    // Auto-inject Telegram config from env var
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

    #[cfg(feature = "telegram")]
    if let Some(tg_config) = config.channel.get("telegram") {
        if tg_config.enabled {
            if let Ok(mut tg) = electro_channels::TelegramChannel::new(tg_config) {
                if tg.start().await.is_ok() {
                    if let Some(rx) = tg.take_receiver() {
                        channel_receivers.push(rx);
                    }
                    let arc: Arc<dyn Channel> = Arc::new(tg);
                    if primary_channel.is_none() {
                        primary_channel = Some(arc.clone());
                    }
                    channels.push(arc);
                }
            }
        }
    }

    // (Discord, Slack flags follow same pattern...)
    #[cfg(feature = "discord")]
    if let Some(dc_config) = config.channel.get("discord") {
        if dc_config.enabled {
            if let Ok(mut dc) = electro_channels::DiscordChannel::new(dc_config) {
                if dc.start().await.is_ok() {
                    if let Some(rx) = dc.take_receiver() {
                        channel_receivers.push(rx);
                    }
                    let arc: Arc<dyn Channel> = Arc::new(dc);
                    if primary_channel.is_none() {
                        primary_channel = Some(arc.clone());
                    }
                    channels.push(arc);
                }
            }
        }
    }

    #[cfg(feature = "slack")]
    if let Some(sl_config) = config.channel.get("slack") {
        if sl_config.enabled {
            if let Ok(mut sl) = electro_channels::SlackChannel::new(sl_config) {
                if sl.start().await.is_ok() {
                    if let Some(rx) = sl.take_receiver() {
                        channel_receivers.push(rx);
                    }
                    let arc: Arc<dyn Channel> = Arc::new(sl);
                    if primary_channel.is_none() {
                        primary_channel = Some(arc.clone());
                    }
                    channels.push(arc);
                }
            }
        }
    }

    // ── Runtime Contract ──
    let pending_messages = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let censored_channel: Option<Arc<dyn Channel>> = primary_channel
        .clone()
        .map(|ch| Arc::new(SecretCensorChannel { inner: ch }) as Arc<dyn Channel>);
    let shared_mode = Arc::new(tokio::sync::RwLock::new(config.mode));
    let shared_memory_strategy = Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda));
    let (queue_tx, msg_rx) = tokio::sync::mpsc::channel::<InboundMessage>(32);
    let remote_workers = std::env::var("ELECTRO_REMOTE_WORKERS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    set_runtime_policy(ToolPolicy {
        allow_shell: config.tools.shell,
        allow_network: config.tools.http,
        allow_filesystem: config.tools.file || config.tools.git,
        writable_roots: vec![electro_core::paths::workspace_dir()],
    });

    let runtime = RuntimeHandle::new_with_config_and_observable(
        queue_tx,
        shared_mode.clone(),
        shared_memory_strategy.clone(),
        RuntimeConfig {
            max_concurrency: 8,
            worker_timeout: config.agent.max_task_duration_secs,
            tool_timeout_secs: 60,
            max_queue: 1024,
            max_active_per_chat: 1,
            remote_threshold_chars: 500,
            remote_workers,
            remote_auth_token: std::env::var("ELECTRO_REMOTE_AUTH_TOKEN").ok(),
            remote_retries: 3,
            tool_policy: ToolPolicyConfig {
                allow_shell: config.tools.shell,
                allow_network: config.tools.http,
                allow_filesystem: config.tools.file || config.tools.git,
                writable_roots: vec![electro_core::paths::workspace_dir().display().to_string()],
            },
        },
        observable,
    );

    // ── Tools ──
    let (tools, _browser_tool_ref) = init_tools(
        &config.tools,
        censored_channel,
        Some(pending_messages.clone()),
        Some(core.memory.clone()),
        Some(Arc::new(core.setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(core.usage_store.clone()),
        if personality_locked {
            None
        } else {
            Some(shared_mode.clone())
        },
        core.vault.clone(),
    );

    // ── Custom Registry ──
    let custom_tool_registry = Arc::new(electro_tools::CustomToolRegistry::new());

    // ── MCP Manager ──
    #[cfg(feature = "mcp")]
    let mcp_manager = Arc::new(electro_mcp::McpManager::new());
    #[cfg(feature = "mcp")]
    mcp_manager.connect_all().await;

    // ── Dispatcher Bus ──
    let mut task_handles = Vec::new();
    for mut ch_rx in channel_receivers {
        let tx = runtime.queue_tx.clone();
        task_handles.push(tokio::spawn(async move {
            while let Some(msg) = ch_rx.recv().await {
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        }));
    }

    // ── Workspace & Heartbeat ──
    let workspace_path = electro_core::paths::workspace_dir();
    std::fs::create_dir_all(&workspace_path).ok();
    if config.heartbeat.enabled {
        let runner = electro_automation::HeartbeatRunner::new(
            config.heartbeat.clone(),
            workspace_path.clone(),
            config
                .heartbeat
                .report_to
                .clone()
                .unwrap_or_else(|| "heartbeat".to_string()),
        );
        let hb_tx = runtime.queue_tx.clone();
        task_handles.push(tokio::spawn(async move {
            runner.run(hb_tx).await;
        }));
    }

    // ── Hive ──
    let hive_config = load_hive_config().await;
    let hive_instance = if hive_config.enabled {
        let hive_url = format!(
            "sqlite:{}?mode=rwc",
            electro_core::paths::hive_db_file().display()
        );
        electro_hive::Hive::new(&hive_config, &hive_url)
            .await
            .ok()
            .map(Arc::new)
    } else {
        None
    };

    // ── Tenant Manager ──
    let tenant_manager = Arc::new(electro_core::tenant_impl::create_tenant_manager(&config));

    // ── Agent State ──
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
                        model,
                        Some(build_system_prompt()),
                        hive_config.enabled,
                        runtime.shared_mode.clone(),
                        runtime.shared_memory_strategy.clone(),
                    )
                    .await;
                    runtime.set_agent(agent).await;
                    runtime.set_active_provider(provider_name).await;
                }
                Err(error) => {
                    tracing::warn!(%provider_name, %error, "Failed to initialize server agent");
                }
            }
        }
    }

    // ── Message Dispatcher Loop ──
    let dispatcher_runtime = runtime.clone();
    let dispatcher_memory = core.memory.clone();
    let dispatcher_usage_store = core.usage_store.clone();
    let dispatcher_setup_tokens = core.setup_tokens.clone();
    let dispatcher_vault = core.vault.clone();
    let dispatcher_config = config.clone();
    task_handles.push(tokio::spawn(async move {
        run_message_dispatcher(
            msg_rx,
            primary_channel,
            dispatcher_runtime,
            dispatcher_memory,
            tools,
            custom_tool_registry,
            #[cfg(feature = "mcp")]
            mcp_manager,
            dispatcher_config,
            pending_messages,
            dispatcher_setup_tokens,
            Arc::new(Mutex::new(HashSet::new())), // pending_raw_keys
            #[cfg(feature = "browser")]
            Arc::new(Mutex::new(HashMap::new())), // login_sessions
            dispatcher_usage_store,
            hive_instance,
            workspace_path,
            personality_locked,
            tenant_manager,
            #[cfg(feature = "browser")]
            browser_tool_ref,
            dispatcher_vault,
        )
        .await;
    }));

    // ── Gateway Server ──
    println!("ELECTRO gateway starting...");
    let gateway = Arc::new(electro_gateway::server::SkyGate::new(
        channels,
        runtime.clone(),
        config.gateway.clone(),
    ));
    let listener = gateway.bind().await?;
    println!(
        "  Gateway: http://{}:{}",
        config.gateway.host, config.gateway.port
    );
    let gw_clone = gateway.clone();
    task_handles.push(tokio::spawn(async move {
        let _ = gw_clone.serve(listener).await;
    }));

    tokio::signal::ctrl_c().await?;
    println!("\nELECTRO shutting down gracefully...");
    drop(runtime);
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        futures::future::join_all(task_handles),
    )
    .await;
    remove_pid_file();
    Ok(())
}
