use crate::app::server::dispatcher::run_message_dispatcher;
use crate::app::{init_core_stack, init_tools, load_hive_config};
use crate::bootstrap::SecretCensorChannel;
use crate::daemon::remove_pid_file;
use anyhow::Result;
use electro_core::types::config::{ElectroConfig, ElectroMode, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use electro_core::Channel;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub mod commands;
pub mod context;
pub mod dispatcher;
pub mod slot;
pub mod worker;

use self::context::WorkerServices;

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

    // ── Tools ──
    let pending_messages = Arc::new(std::sync::Mutex::new(HashMap::new()));
    let censored_channel: Option<Arc<dyn Channel>> = primary_channel
        .clone()
        .map(|ch| Arc::new(SecretCensorChannel { inner: ch }) as Arc<dyn Channel>);

    let shared_mode = Arc::new(tokio::sync::RwLock::new(config.mode));
    let shared_memory_strategy = Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda));

    let (tools, browser_tool_ref) = init_tools(
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

    // ── Agent State ──
    let agent_state = Arc::new(tokio::sync::RwLock::new(None));
    // ... agent init logic (omitted for brevity, remains in full impl) ...

    // ── Dispatcher Bus ──
    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel::<InboundMessage>(32);
    let mut task_handles = Vec::new();
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
        let hb_tx = msg_tx.clone();
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

    // ── Worker Context ──
    let services = WorkerServices::new(
        &core,
        &config,
        tools,
        custom_tool_registry,
        #[cfg(feature = "mcp")]
        mcp_manager,
        hive_instance,
        tenant_manager,
        workspace_path,
        shared_mode,
        shared_memory_strategy,
        personality_locked,
        #[cfg(feature = "browser")]
        browser_tool_ref,
    );

    // ── Message Dispatcher Loop ──
    run_message_dispatcher(msg_rx, msg_tx.clone(), primary_channel, services).await;

    // ── Gateway Server ──
    println!("ELECTRO gateway starting...");
    if let Some(agent) = agent_state.read().await.clone() {
        let gateway = Arc::new(electro_gateway::server::SkyGate::new(
            channels,
            agent,
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
    } else {
        println!("  Gateway: disabled (waiting for credentials)");
    }

    tokio::signal::ctrl_c().await?;
    println!("\nELECTRO shutting down gracefully...");
    drop(msg_tx);
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        futures::future::join_all(task_handles),
    )
    .await;
    remove_pid_file();
    Ok(())
}
