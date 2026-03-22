use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use base64::Engine as _;
use clap::Parser;
use futures::FutureExt;
use electro_core::config::credentials::{
    credentials_path, detect_api_key, is_placeholder_key, load_active_provider_keys,
    load_credentials_file, load_saved_credentials, save_credentials,
};
use electro_core::types::model_registry::{
    available_models_for_provider, default_model, is_vision_model,
};
use electro_core::paths;
use electro_core::Channel;
use tokio::sync::Mutex;

// Extracted modules
mod admin;
mod bootstrap;
mod cli;
mod daemon;
mod onboarding;
mod reset;
mod server_mode;

// Re-export commonly used items
use bootstrap::{censor_secrets, SecretCensorChannel};
use cli::{format_user_error, handle_model_command, list_configured_providers, remove_provider, Cli, Commands, ConfigCommands, SkillCommands};
use daemon::{is_process_alive, pid_file_path, read_pid_file, remove_pid_file, write_pid_file};
use onboarding::{build_system_prompt, decrypt_otk_blob, onboarding_message_with_link, send_with_retry, validate_provider_key, ONBOARDING_REFERENCE};

#[cfg(feature = "codex-oauth")]
use cli::AuthCommands;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging — TUI mode writes to a file instead of stderr
    #[cfg(feature = "tui")]
    let _is_tui = matches!(cli.command, Commands::Tui);
    #[cfg(not(feature = "tui"))]
    let _is_tui = false;

    if _is_tui {
        // TUI mode: write logs to ~/.electro/tui.log so they don't corrupt the display
        let log_dir = paths::electro_home();
        std::fs::create_dir_all(&log_dir).ok();
        if let Ok(log_file) = std::fs::File::create(paths::tui_log_file()) {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .with_writer(std::sync::Mutex::new(log_file))
                .with_ansi(false)
                .json()
                .init();
        }
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .json()
            .init();
    }

    // ── TUI fast path — skip all other init, go straight to TUI ──
    #[cfg(feature = "tui")]
    if _is_tui {
        let config_path = cli.config.as_deref().map(std::path::Path::new);
        let config = electro_core::config::load_config(config_path)?;
        return electro_tui::launch_tui(config).await;
    }

    // Initialize health endpoint uptime clock
    electro_gateway::health::init_start_time();

    // ── Global panic hook — route panics through tracing ─────
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else {
            "unknown panic payload".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        tracing::error!(
            panic.payload = %payload,
            panic.location = %location,
            "PANIC caught — task will attempt recovery"
        );
    }));

    // ── Handle Reset before config loading ─────────────────
    if let Commands::Reset { confirm } = &cli.command {
        return reset::factory_reset(*confirm).await;
    }

    // Load configuration
    let config_path = cli.config.as_ref().map(std::path::Path::new);
    let mut config = electro_core::config::load_config(config_path)?;

    // ── SecurityConfig runtime enforcement ────────────────────────────
    // Each flag is now checked at startup and either actively enforced or
    // loudly warned so operators cannot silently misconfigure security.
    // This happens BEFORE spawning any background tasks to ensure thread safety
    // for environment variable modifications.
    {
        let sec = &config.security;

        // P2.9a — sandbox → browser --no-sandbox gate
        // The browser tool reads ELECTRO_BROWSER_ALLOW_NO_SANDBOX at launch
        // time. We only set it when the operator has explicitly lowered the
        // sandbox policy from the secure default ("mandatory").
        let sandbox_mandatory = matches!(sec.sandbox.trim(), "mandatory" | "strict");
        if !sandbox_mandatory {
            // SAFETY: Current thread is the only runner. No tasks spawned yet.
            unsafe {
                std::env::set_var("ELECTRO_BROWSER_ALLOW_NO_SANDBOX", "1");
            }
            tracing::warn!(
                sandbox = %sec.sandbox,
                "security.sandbox is not 'mandatory' — browser will launch with --no-sandbox. \
                 Change security.sandbox to 'mandatory' to enforce browser sandboxing."
            );
        }

        // P2.9b — auditable flag enforcement
        if !sec.audit_log {
            tracing::warn!(
                "security.audit_log = false — tool call audit logging is DISABLED. \
                 Set security.audit_log = true to restore traceability."
            );
        }

        // P2.9b — file_scanning flag enforcement
        if !sec.file_scanning {
            tracing::warn!(
                "security.file_scanning = false — uploaded file scanning is DISABLED. \
                 Malicious payloads will not be scanned before being processed by tools."
            );
        }

        // P2.9b — skill_signing flag enforcement
        if sec.skill_signing != "required" {
            tracing::warn!(
                "security.skill_signing is not 'required' — skill/plugin integrity \
                 is not verified. Set security.skill_signing = 'required' to enforce signing."
            );
        }
    }

    if !_is_tui {
        tracing::info!(mode = %cli.mode, "ELECTRO starting");
    }

    // ── Initialize observability (metrics + optional OTel) ───────────
    // Keeps the observable alive for the process lifetime.
    let _observable = match electro_observable::create_observable(&config.observability) {
        Ok(obs) => {
            tracing::info!(
                otel_enabled = config.observability.otel_enabled,
                "Observability initialized"
            );
            Some(obs)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Observability init failed — metrics collection disabled");
            None
        }
    };

    match cli.command {
        Commands::Stop => {
            match read_pid_file() {
                Some(pid) if is_process_alive(pid) => {
                    // Send SIGTERM on Unix, taskkill on Windows
                    #[cfg(unix)]
                    {
                        let status = std::process::Command::new("kill")
                            .args(["-TERM", &pid.to_string()])
                            .status();
                        match status {
                            Ok(s) if s.success() => {
                                remove_pid_file();
                                println!("ELECTRO daemon (PID {}) stopped.", pid);
                            }
                            _ => {
                                eprintln!("Failed to stop ELECTRO daemon (PID {}).", pid);
                                std::process::exit(1);
                            }
                        }
                    }
                    #[cfg(windows)]
                    {
                        let status = std::process::Command::new("taskkill")
                            .args(["/PID", &pid.to_string(), "/F"])
                            .status();
                        match status {
                            Ok(s) if s.success() => {
                                remove_pid_file();
                                println!("ELECTRO daemon (PID {}) stopped.", pid);
                            }
                            _ => {
                                eprintln!("Failed to stop ELECTRO daemon (PID {}).", pid);
                                std::process::exit(1);
                            }
                        }
                    }
                }
                Some(pid) => {
                    eprintln!(
                        "ELECTRO daemon (PID {}) is not running. Cleaning up stale PID file.",
                        pid
                    );
                    remove_pid_file();
                }
                None => {
                    eprintln!("No ELECTRO daemon running (no PID file found).");
                    std::process::exit(1);
                }
            }
        }
        Commands::Start {
            daemon,
            log,
            personality,
        } => {
            if daemon {
                daemon::start_daemon(log).map_err(|e| anyhow::anyhow!(e))?;
            } else {
                // Normal foreground start
                write_pid_file();
                server_mode::start_server(&mut config, personality, cli.mode).await?;
            }
        }
        Commands::Chat => {
            run_chat_mode(config, config_path).await?;
        }
        Commands::Status => {
            println!("ELECTRO Status");
            println!("  Mode: {}", config.electro.mode);
            println!("  Gateway: {}:{}", config.gateway.host, config.gateway.port);
            println!(
                "  Provider: {}",
                config.provider.name.as_deref().unwrap_or("not configured")
            );
            println!("  Memory: {}", config.memory.backend);
            println!("  Vault: {}", config.vault.backend);
        }
        Commands::Skill { command } => match command {
            SkillCommands::List => {
                println!("Installed skills:");
            }
            SkillCommands::Info { name } => {
                println!("Skill info: {}", name);
            }
            SkillCommands::Install { path } => {
                println!("Installing skill from: {}", path);
            }
        },
        Commands::Config { command } => match command {
            ConfigCommands::Validate => {
                println!("Configuration valid.");
                println!("  Gateway: {}:{}", config.gateway.host, config.gateway.port);
                println!(
                    "  Provider: {}",
                    config.provider.name.as_deref().unwrap_or("none")
                );
                println!("  Memory backend: {}", config.memory.backend);
                println!("  Channels: {}", config.channel.len());
            }
            ConfigCommands::Show => {
                let output = toml::to_string_pretty(&config)?;
                println!("{}", output);
            }
        },
        Commands::Update => {
            run_update().await?;
        }
        Commands::Version => {
            println!(
                "electro {} — commit: {} — date: {}",
                env!("CARGO_PKG_VERSION"),
                env!("GIT_HASH"),
                env!("BUILD_DATE")
            );
            println!("Cloud-native Rust AI agent runtime — Telegram-native");
        }
        #[cfg(feature = "codex-oauth")]
        Commands::Auth { command } => {
            run_auth_command(command).await?;
        }
        // Reset is handled before config loading — this arm is unreachable
        Commands::Reset { .. } => unreachable!(),
        #[cfg(feature = "tui")]
        Commands::Tui => {
            electro_tui::launch_tui(config).await?;
        }
    }

    Ok(())
}

async fn run_chat_mode(
    config: electro_core::types::config::ElectroConfig,
    config_path: Option<&std::path::Path>,
) -> Result<()> {
    println!("ELECTRO interactive chat");
    println!("Type '/quit' or '/exit' to quit.\n");

    // Check hive config for CLI chat path
    let hive_enabled_early = check_hive_enabled().await;

    // ── Resolve API credentials ────────────────────────
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
                    .unwrap_or_else(|| default_model(&name).to_string());
                Some((name, key.clone(), model))
            } else {
                load_saved_credentials()
            }
        } else {
            load_saved_credentials()
        }
    };

    // ── Memory backend ─────────────────────────────────
    let memory_url = config.memory.path.clone().unwrap_or_else(|| {
        let data_dir = paths::electro_home();
        if let Err(e) = std::fs::create_dir_all(&data_dir) {
            tracing::warn!(error = %e, path = %data_dir.display(), "Failed to create directory");
        }
        format!("sqlite:{}/memory.db?mode=rwc", data_dir.display())
    });
    let memory: Arc<dyn electro_core::Memory> = Arc::from(
        electro_memory::create_memory_backend(&config.memory.backend, &memory_url).await?,
    );

    // ── CLI channel ────────────────────────────────────
    let workspace = paths::workspace_dir();
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        tracing::warn!(error = %e, path = %workspace.display(), "Failed to create directory");
    }
    let mut cli_channel = electro_channels::CliChannel::new(workspace.clone());
    let cli_rx = cli_channel.take_receiver();
    cli_channel.start().await?;
    let cli_arc: Arc<dyn electro_core::Channel> = Arc::new(cli_channel);

    // ── OTK state ──────────────────────────────────────
    let setup_tokens = electro_gateway::SetupTokenStore::new();

    // ── Usage store ──────────────────────────────────────
    let usage_store: Arc<dyn electro_core::UsageStore> =
        Arc::new(electro_memory::SqliteUsageStore::new(&memory_url).await?);

    // ── Vault (encrypted credential store) ───────────────
    let vault: Option<Arc<dyn electro_core::Vault>> = match electro_vault::LocalVault::new()
        .await
    {
        Ok(v) => {
            tracing::info!("Vault initialized (CLI)");
            Some(Arc::new(v) as Arc<dyn electro_core::Vault>)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Vault initialization failed — browser authenticate disabled");
            None
        }
    };

    // ── Tools ──────────────────────────────────────────
    let pending_messages: electro_tools::PendingMessages =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let censored_cli: Arc<dyn Channel> = Arc::new(SecretCensorChannel {
        inner: cli_arc.clone(),
    });
    let shared_mode: electro_tools::SharedMode =
        Arc::new(tokio::sync::RwLock::new(config.mode));
    let shared_memory_strategy: Arc<
        tokio::sync::RwLock<electro_core::types::config::MemoryStrategy>,
    > = Arc::new(tokio::sync::RwLock::new(
        electro_core::types::config::MemoryStrategy::Lambda,
    ));
    
    #[cfg(feature = "browser")]
    let (mut tools_template, cli_browser_ref) = electro_tools::create_tools_with_browser(
        &config.tools,
        Some(censored_cli),
        Some(pending_messages.clone()),
        Some(memory.clone()),
        Some(Arc::new(setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(usage_store.clone()),
        Some(shared_mode.clone()),
        vault.clone(),
    );
    #[cfg(not(feature = "browser"))]
    let mut tools_template = electro_tools::create_tools(
        &config.tools,
        Some(censored_cli),
        Some(pending_messages.clone()),
        Some(memory.clone()),
        Some(Arc::new(setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(usage_store.clone()),
        Some(shared_mode.clone()),
        vault.clone(),
    );

    // ── Custom script tools (user/agent-authored) ──────
    let custom_tool_registry = Arc::new(electro_tools::CustomToolRegistry::new());
    {
        let custom_tools = custom_tool_registry.load_tools();
        if !custom_tools.is_empty() {
            tracing::info!(count = custom_tools.len(), "Custom script tools loaded");
            tools_template.extend(custom_tools);
        }
    }

    // ── MCP servers (external tool sources) ──────────
    #[cfg(feature = "mcp")]
    let mcp_manager: Arc<electro_mcp::McpManager> = {
        let mgr = Arc::new(electro_mcp::McpManager::new());
        mgr.connect_all().await;
        let tool_names: Vec<String> = tools_template
            .iter()
            .map(|t| t.name().to_string())
            .collect();
        let mcp_tools = mgr.bridge_tools(&tool_names).await;
        if !mcp_tools.is_empty() {
            tracing::info!(count = mcp_tools.len(), "MCP bridge tools loaded");
            tools_template.extend(mcp_tools);
        }
        mgr
    };

    let base_url = config.provider.base_url.clone();

    // ── Build agent (if credentials available) ─────────
    let max_turns = config.agent.max_turns;
    let max_ctx = config.agent.max_context_tokens;
    let max_rounds = config.agent.max_tool_rounds;
    let max_task_duration = config.agent.max_task_duration_secs;
    let max_spend = config.agent.max_spend_usd;
    let v2_opt = config.agent.v2_optimizations;
    let pp_opt = config.agent.parallel_phases;

    let mut agent_opt: Option<electro_agent::AgentRuntime> = None;

    if let Some((pname, key, model)) = credentials {
        if !is_placeholder_key(&key) {
            let (all_keys, saved_base_url) = load_active_provider_keys()
                .map(|(_, keys, _, burl)| {
                    let valid: Vec<String> = keys
                        .into_iter()
                        .filter(|k| !is_placeholder_key(k))
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
            
            match create_provider(&provider_config, &pname, &model).await {
                Ok(provider) => {
                    let system_prompt = Some(build_system_prompt());
                    agent_opt = Some(
                        electro_agent::AgentRuntime::with_limits(
                            provider,
                            memory.clone(),
                            tools_template.clone(),
                            model.clone(),
                            system_prompt,
                            max_turns,
                            max_ctx,
                            max_rounds,
                            max_task_duration,
                            max_spend,
                        )
                        .with_v2_optimizations(v2_opt)
                        .with_parallel_phases(pp_opt)
                        .with_hive_enabled(hive_enabled_early)
                        .with_shared_mode(shared_mode.clone())
                        .with_shared_memory_strategy(shared_memory_strategy.clone()),
                    );
                    println!("Connected to {} (model: {})", pname, model);
                    if max_spend > 0.0 {
                        println!("Budget: ${:.2} per session", max_spend);
                    } else {
                        println!("Budget: unlimited");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to create provider: {}", e);
                }
            }
        }
    }

    if agent_opt.is_none() {
        // Check if Codex OAuth tokens exist
        #[cfg(feature = "codex-oauth")]
        {
            if electro_codex_oauth::TokenStore::exists() {
                let model = "gpt-5.4".to_string();
                match electro_codex_oauth::TokenStore::load() {
                    Ok(store) => {
                        let token_store = std::sync::Arc::new(store);
                        let provider: Arc<dyn electro_core::Provider> =
                            Arc::new(electro_codex_oauth::CodexResponsesProvider::new(
                                model.clone(),
                                token_store,
                            ));
                        let system_prompt = Some(build_system_prompt());
                        agent_opt = Some(
                            electro_agent::AgentRuntime::with_limits(
                                provider,
                                memory.clone(),
                                tools_template.clone(),
                                model.clone(),
                                system_prompt,
                                max_turns,
                                max_ctx,
                                max_rounds,
                                max_task_duration,
                                max_spend,
                            )
                            .with_v2_optimizations(v2_opt)
                            .with_parallel_phases(pp_opt)
                            .with_shared_mode(shared_mode.clone())
                            .with_shared_memory_strategy(shared_memory_strategy.clone()),
                        );
                        println!(
                            "Connected to openai-codex via Codex OAuth (model: {})",
                            model
                        );
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Codex OAuth tokens exist but failed to load");
                    }
                }
            }
        }
    }
    
    if agent_opt.is_none() {
        println!("No API key configured — running in onboarding mode.");
        let otk = setup_tokens.generate("cli").await;
        let otk_hex = hex::encode(otk);
        let link = format!("https://electro-labs.github.io/electro/setup#{}", otk_hex);
        println!("\n{}", onboarding_message_with_link(&link));
        println!("\n{}", ONBOARDING_REFERENCE);
    }
    println!("---\n");

    // ── Message loop ───────────────────────────────────
    let Some(mut rx) = cli_rx else {
        eprintln!("CLI channel receiver unavailable");
        return Ok(());
    };
    
    // ── Restore CLI conversation history from memory backend ──
    let cli_history_key = "chat_history:cli".to_string();
    let mut history: Vec<electro_core::types::message::ChatMessage> =
        match memory.get(&cli_history_key).await {
            Ok(Some(entry)) => match serde_json::from_str(&entry.content) {
                Ok(h) => {
                    let count = Vec::<electro_core::types::message::ChatMessage>::len(&h);
                    if count > 0 {
                        println!("  Restored {} messages from previous session.", count);
                    }
                    h
                }
                Err(_) => Vec::new(),
            },
            _ => Vec::new(),
        };

    while let Some(msg) = rx.recv().await {
        let msg_text = msg.text.as_deref().unwrap_or("");
        let cmd_lower = msg_text.trim().to_lowercase();

        // Handle commands
        if cmd_lower == "/quit" || cmd_lower == "/exit" {
            break;
        }

        // /addkey — secure OTK flow
        if cmd_lower == "/addkey" {
            let otk = setup_tokens.generate(&msg.chat_id).await;
            let otk_hex = hex::encode(otk);
            let link = format!("https://electro-labs.github.io/electro/setup#{}", otk_hex);
            println!(
                "\nSecure key setup:\n\n\
                 1. Open this link:\n{}\n\n\
                 2. Paste your API key in the form\n\
                 3. Copy the encrypted blob\n\
                 4. Paste it back here\n\n\
                 Link expires in 10 minutes.\n\n\
                 For a quick (less secure) method: /addkey unsafe\n",
                link
            );
            eprint!("electro> ");
            continue;
        }

        // /addkey unsafe
        if cmd_lower == "/addkey unsafe" {
            println!("\nPaste your API key below.");
            println!("Warning: the key will be visible in terminal history.");
            println!("For a secure method, use /addkey instead.\n");
            eprint!("electro> ");
            continue;
        }

        // /keys
        if cmd_lower == "/keys" {
            println!("\n{}\n", list_configured_providers());
            eprint!("electro> ");
            continue;
        }

        // /removekey <provider>
        if cmd_lower.starts_with("/removekey") {
            let provider_arg = msg_text.trim()["/removekey".len()..].trim();
            println!("\n{}\n", remove_provider(provider_arg));
            if !provider_arg.is_empty() && load_active_provider_keys().is_none() {
                agent_opt = None;
                println!("All providers removed — agent offline.\n");
            }
            eprint!("electro> ");
            continue;
        }

        // /usage — show usage summary
        if cmd_lower == "/usage" {
            match usage_store.usage_summary(&msg.chat_id).await {
                Ok(summary) => {
                    if summary.turn_count == 0 {
                        println!("\nNo usage records for this chat yet.\n");
                    } else {
                        println!(
                            "\nUsage Summary\nTurns: {}\nAPI Calls: {}\nInput Tokens: {}\nOutput Tokens: {}\nCombined Tokens: {}\nTools Used: {}\nTotal Cost: ${:.4}\n",
                            summary.turn_count,
                            summary.total_api_calls,
                            summary.total_input_tokens,
                            summary.total_output_tokens,
                            summary.combined_tokens(),
                            summary.total_tools_used,
                            summary.total_cost_usd,
                        );
                    }
                }
                Err(e) => eprintln!("Failed to query usage: {}", e),
            }
            eprint!("electro> ");
            continue;
        }

        // /help — list available commands
        if cmd_lower == "/help" {
            print_help();
            eprint!("electro> ");
            continue;
        }

        // /memory — switch memory strategy
        if cmd_lower == "/memory" || cmd_lower.starts_with("/memory ") {
            let args = if cmd_lower == "/memory" {
                ""
            } else {
                msg_text.trim()["/memory".len()..].trim()
            };
            let args_lower = args.to_lowercase();
            if args_lower.is_empty() || args_lower == "status" {
                let current = shared_memory_strategy.read().await;
                println!(
                    "\nMemory Strategy: {}\n\n\
                     Available strategies:\n\
                     • /memory lambda — λ-Memory: decay-scored, cross-session persistence, hash-based recall (default)\n\
                     • /memory echo — Echo Memory: keyword search over current context window, no persistence\n",
                    *current,
                );
            } else if args_lower == "lambda" || args_lower == "λ" {
                *shared_memory_strategy.write().await =
                    electro_core::types::config::MemoryStrategy::Lambda;
                println!("\nSwitched to λ-Memory\nDecay-scored fidelity tiers • cross-session persistence • hash-based recall\n");
            } else if args_lower == "echo" {
                *shared_memory_strategy.write().await =
                    electro_core::types::config::MemoryStrategy::Echo;
                println!("\nSwitched to Echo Memory\nKeyword search over context window • no persistence between sessions\n");
            } else {
                println!("\nUnknown strategy. Use: /memory lambda or /memory echo\n");
            }
            eprint!("electro> ");
            continue;
        }

        // /model command
        if cmd_lower == "/model" || cmd_lower.starts_with("/model ") {
            let args = if cmd_lower == "/model" {
                ""
            } else {
                msg_text.trim()["/model".len()..].trim()
            };
            let result = handle_model_command(args);
            println!("\n{}\n", result);
            eprint!("electro> ");
            continue;
        }

        // enc:v1: — encrypted blob from OTK flow
        if msg_text.trim().starts_with("enc:v1:") {
            let blob_b64 = &msg_text.trim()["enc:v1:".len()..];
            match decrypt_otk_blob(blob_b64, &setup_tokens, &msg.chat_id).await {
                Ok(api_key_text) => {
                    if let Some(cred) = detect_api_key(&api_key_text) {
                        let model = default_model(cred.provider).to_string();
                        let effective_base_url =
                            cred.base_url.clone().or_else(|| base_url.clone());
                        let test_config = electro_core::types::config::ProviderConfig {
                            name: Some(cred.provider.to_string()),
                            api_key: Some(cred.api_key.clone()),
                            keys: vec![cred.api_key.clone()],
                            model: Some(model.clone()),
                            base_url: effective_base_url,
                            extra_headers: std::collections::HashMap::new(),
                        };
                        match validate_provider_key(&test_config).await {
                            Ok(validated_provider) => {
                                if let Err(e) = save_credentials(
                                    cred.provider,
                                    &cred.api_key,
                                    &model,
                                    cred.base_url.as_deref(),
                                )
                                .await
                                {
                                    eprintln!("Failed to save credentials: {}", e);
                                }
                                let system_prompt = Some(build_system_prompt());
                                agent_opt = Some(
                                    electro_agent::AgentRuntime::with_limits(
                                        validated_provider,
                                        memory.clone(),
                                        tools_template.clone(),
                                        model.clone(),
                                        system_prompt,
                                        max_turns,
                                        max_ctx,
                                        max_rounds,
                                        max_task_duration,
                                        max_spend,
                                    )
                                    .with_v2_optimizations(v2_opt)
                                    .with_parallel_phases(pp_opt)
                                    .with_shared_mode(shared_mode.clone())
                                    .with_shared_memory_strategy(
                                        shared_memory_strategy.clone(),
                                    ),
                                );
                                println!(
                                    "\nAPI key securely received and verified! Configured {} with model {}.",
                                    cred.provider, model
                                );
                                println!("ELECTRO is online.\n");
                            }
                            Err(err) => {
                                eprintln!(
                                    "\nKey decrypted but validation failed — {} returned:\n{}\nCheck the key and try /addkey again.\n",
                                    cred.provider, err
                                );
                            }
                        }
                    } else {
                        eprintln!(
                            "\nDecrypted successfully but couldn't detect the provider.\nMake sure you pasted a valid API key in the setup page.\n"
                        );
                    }
                }
                Err(err) => {
                    eprintln!("\n{}\n", err);
                }
            }
            eprint!("electro> ");
            continue;
        }

        // Detect raw API key paste
        if let Some(cred) = detect_api_key(msg_text) {
            let model = default_model(cred.provider).to_string();
            let effective_base_url = cred.base_url.clone().or_else(|| base_url.clone());
            let test_config = electro_core::types::config::ProviderConfig {
                name: Some(cred.provider.to_string()),
                api_key: Some(cred.api_key.clone()),
                keys: vec![cred.api_key.clone()],
                model: Some(model.clone()),
                base_url: effective_base_url,
                extra_headers: std::collections::HashMap::new(),
            };
            match validate_provider_key(&test_config).await {
                Ok(validated_provider) => {
                    if let Err(e) = save_credentials(
                        cred.provider,
                        &cred.api_key,
                        &model,
                        cred.base_url.as_deref(),
                    )
                    .await
                    {
                        eprintln!("Failed to save credentials: {}", e);
                    }
                    let system_prompt = Some(build_system_prompt());
                    agent_opt = Some(
                        electro_agent::AgentRuntime::with_limits(
                            validated_provider,
                            memory.clone(),
                            tools_template.clone(),
                            model.clone(),
                            system_prompt,
                            max_turns,
                            max_ctx,
                            max_rounds,
                            max_task_duration,
                            max_spend,
                        )
                        .with_v2_optimizations(v2_opt)
                        .with_parallel_phases(pp_opt)
                        .with_hive_enabled(hive_enabled_early)
                        .with_shared_mode(shared_mode.clone())
                        .with_shared_memory_strategy(shared_memory_strategy.clone()),
                    );
                    println!(
                        "\nAPI key verified! Configured {} with model {}.",
                        cred.provider, model
                    );
                    println!("ELECTRO is online.\n");
                }
                Err(err) => {
                    eprintln!(
                        "\nInvalid API key — {} returned:\n{}\nCheck the key and try again.\n",
                        cred.provider, err
                    );
                }
            }
            eprint!("electro> ");
            continue;
        }

        // ── Normal agent processing ────────────────────
        if let Some(ref agent) = agent_opt {
            let mut session = electro_core::types::session::SessionContext {
                session_id: "cli-cli".to_string(),
                user_id: msg.user_id.clone(),
                channel: msg.channel.clone(),
                chat_id: msg.chat_id.clone(),
                history: history.clone(),
                workspace_path: workspace.clone(),
            };

            // Early reply channel for LLM classifier
            let (early_tx, mut early_rx) = tokio::sync::mpsc::unbounded_channel::<
                electro_core::types::message::OutboundMessage,
            >();
            let cli_for_early = cli_arc.clone();
            tokio::spawn(async move {
                while let Some(mut early_msg) = early_rx.recv().await {
                    early_msg.text = censor_secrets(&early_msg.text);
                    cli_for_early.send_message(early_msg).await.ok();
                }
            });

            let process_result = AssertUnwindSafe(agent.process_message(
                &msg,
                &mut session,
                None,
                None,
                Some(early_tx),
                None,
                None,
            ))
            .catch_unwind()
            .await;

            match process_result {
                Ok(Ok((mut reply, turn_usage))) => {
                    reply.text = censor_secrets(&reply.text);
                    cli_arc.send_message(reply).await.ok();

                    // Record usage
                    let record = electro_core::UsageRecord {
                        id: uuid::Uuid::new_v4().to_string(),
                        chat_id: msg.chat_id.clone(),
                        session_id: "cli-cli".to_string(),
                        timestamp: chrono::Utc::now(),
                        api_calls: turn_usage.api_calls,
                        input_tokens: turn_usage.input_tokens,
                        output_tokens: turn_usage.output_tokens,
                        tools_used: turn_usage.tools_used,
                        total_cost_usd: turn_usage.total_cost_usd,
                        provider: turn_usage.provider.clone(),
                        model: turn_usage.model.clone(),
                    };
                    if let Err(e) = usage_store.record_usage(record).await {
                        tracing::error!(error = %e, "Failed to record usage");
                    }

                    // Display usage summary if enabled
                    if turn_usage.api_calls > 0 {
                        if let Ok(enabled) =
                            usage_store.is_usage_display_enabled(&msg.chat_id).await
                        {
                            if enabled {
                                println!("\n{}", turn_usage.format_summary());
                            }
                        }
                    }
                }
                Ok(Err(electro_core::types::error::ElectroError::HiveRoute(hive_msg))) => {
                    // CLI pack path — simplified version
                    println!("  [Many Tems: Alpha decomposing into pack tasks...]");
                    // For CLI, fall back to single-agent
                    if let Some(ref mut agent) = agent_opt {
                        let non_hive = electro_agent::AgentRuntime::with_limits(
                            agent.provider_arc(),
                            agent.memory_arc(),
                            agent.tools().to_vec(),
                            agent.model().to_string(),
                            None,
                            max_turns,
                            max_ctx,
                            max_rounds,
                            max_task_duration,
                            max_spend,
                        )
                        .with_v2_optimizations(v2_opt)
                        .with_parallel_phases(pp_opt);
                        let re_msg = electro_core::types::message::InboundMessage {
                            id: uuid::Uuid::new_v4().to_string(),
                            channel: "cli".into(),
                            chat_id: "cli".into(),
                            user_id: "local".into(),
                            username: None,
                            text: Some(hive_msg),
                            attachments: vec![],
                            reply_to: None,
                            timestamp: chrono::Utc::now(),
                        };
                        match non_hive
                            .process_message(
                                &re_msg,
                                &mut session,
                                None,
                                None,
                                None,
                                None,
                                None,
                            )
                            .await
                        {
                            Ok((reply, _usage)) => {
                                if !reply.text.trim().is_empty() {
                                    println!("\n{}\n", reply.text);
                                }
                            }
                            Err(e) => eprintln!("  [{}]", format_user_error(&e)),
                        }
                    }
                    eprint!("electro> ");
                }
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "CLI agent processing error");
                    eprintln!("  [{}]", format_user_error(&e));
                    eprint!("electro> ");
                }
                Err(panic_info) => {
                    let panic_msg = if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.clone()
                    } else if let Some(s) = panic_info.downcast_ref::<&str>() {
                        s.to_string()
                    } else {
                        "internal error".to_string()
                    };
                    eprintln!("  [panic recovered: {}]", panic_msg);
                    tracing::error!(panic = %panic_msg, "PANIC RECOVERED in CLI processing");
                    session.history = history.clone();
                }
            }

            history = session.history;

            // ── Save CLI conversation history to memory backend ──
            if let Ok(json) = serde_json::to_string(&history) {
                let entry = electro_core::MemoryEntry {
                    id: cli_history_key.clone(),
                    content: json,
                    metadata: serde_json::json!({"chat_id": "cli"}),
                    timestamp: chrono::Utc::now(),
                    session_id: Some("cli".to_string()),
                    entry_type: electro_core::MemoryEntryType::Conversation,
                };
                if let Err(e) = memory.store(entry).await {
                    tracing::warn!(error = %e, "Failed to persist CLI conversation history");
                }
            }
        } else {
            // Auto-generate fresh OTK for onboarding
            let otk = setup_tokens.generate("cli").await;
            let otk_hex = hex::encode(otk);
            let link = format!("https://electro-labs.github.io/electro/setup#{}", otk_hex);
            println!("\n{}", onboarding_message_with_link(&link));
            println!("\n{}\n", ONBOARDING_REFERENCE);
            eprint!("electro> ");
        }
    }

    println!("\nELECTRO chat ended.");
    Ok(())
}

fn print_help() {
    println!(
        "\nelectro {} — commit: {} — date: {}\n\n\
         Available commands:\n\n\
         /help — Show this help message\n\
         /addkey — Securely add an API key (encrypted OTK flow)\n\
         /addkey unsafe — Add an API key by pasting directly\n\
         /keys — List configured providers and active model\n\
         /model — Show current model and available models\n\
         /model <name> — Switch to a different model\n\
         /removekey <provider> — Remove a provider's API key\n\
         /usage — Show token usage and cost summary\n\
         /memory — Show current memory strategy\n\
         /memory lambda — Switch to λ-Memory (decay + persistence)\n\
         /memory echo — Switch to Echo Memory (context window only)\n\
         /quit — Exit the CLI chat\n\n\
         Just type a message to chat with the AI agent.\n",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_DATE"),
    );
}

async fn check_hive_enabled() -> bool {
    #[derive(serde::Deserialize, Default)]
    struct HC {
        #[serde(default)]
        hive: HE,
    }
    #[derive(serde::Deserialize, Default)]
    struct HE {
        #[serde(default)]
        enabled: bool,
    }
    std::fs::read_to_string(paths::config_file()).ok()
        .or_else(|| std::fs::read_to_string("electro.toml").ok())
        .and_then(|c| toml::from_str::<HC>(&c).ok())
        .map(|c| c.hive.enabled)
        .unwrap_or(false)
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

async fn run_update() -> Result<()> {
    println!("ELECTRO Update");
    println!("Current version: {}\n", env!("CARGO_PKG_VERSION"));

    // 1. Check if we're in a git repo
    let git_check = std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output();
    match git_check {
        Ok(out) if out.status.success() => {}
        _ => {
            eprintln!("Error: Not a git repository. Run `electro update` from the cloned repo directory.");
            std::process::exit(1);
        }
    }

    // 2. Fetch remote
    println!("Fetching latest changes...");
    let fetch = std::process::Command::new("git")
        .args(["fetch", "origin"])
        .output();
    if let Err(e) = fetch {
        eprintln!("Error: Failed to fetch from remote: {}", e);
        std::process::exit(1);
    }

    // 3. Compare local vs remote
    let local_head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let remote_branch = {
        let check_main = std::process::Command::new("git")
            .args(["rev-parse", "--verify", "origin/main"])
            .output();
        if check_main.is_ok_and(|o| o.status.success()) {
            "origin/main"
        } else {
            "origin/master"
        }
    };

    let remote_head = std::process::Command::new("git")
        .args(["rev-parse", remote_branch])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if local_head == remote_head {
        println!("Already up to date.");
        return Ok(());
    }

    // 4. Show what's new
    let log_range = format!("HEAD..{}", remote_branch);
    let log_output = std::process::Command::new("git")
        .args(["log", "--oneline", "--no-decorate", &log_range])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let commit_count = log_output.lines().count();
    println!("{} new commit(s):\n", commit_count);
    for line in log_output.lines().take(20) {
        println!("  {}", line);
    }
    if commit_count > 20 {
        println!("  ... and {} more", commit_count - 20);
    }
    println!();

    // 5. Check for dirty working tree
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    if !status.trim().is_empty() {
        eprintln!("Warning: You have uncommitted changes. Stashing before update...");
        let stash = std::process::Command::new("git")
            .args(["stash", "push", "-m", "electro-update-autostash"])
            .output();
        if stash.map_or(true, |o| !o.status.success()) {
            eprintln!("Error: Failed to stash changes. Commit or stash manually first.");
            std::process::exit(1);
        }
        println!("Changes stashed.\n");
    }

    // 6. Pull
    let branch = remote_branch.strip_prefix("origin/").unwrap_or("main");
    println!("Pulling from origin/{}...", branch);
    let pull = std::process::Command::new("git")
        .args(["pull", "origin", branch])
        .output();
    match pull {
        Ok(out) if out.status.success() => {
            println!("{}", String::from_utf8_lossy(&out.stdout));
        }
        Ok(out) => {
            eprintln!(
                "Error: git pull failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error: git pull failed: {}", e);
            std::process::exit(1);
        }
    }

    // 7. Build release binary
    println!("Building release binary... (this may take a few minutes)");
    let build = std::process::Command::new("cargo")
        .args(["build", "--release", "--bin", "electro"])
        .status();
    match build {
        Ok(s) if s.success() => {
            println!("\nUpdate complete!");
            println!("Restart with: electro start");
        }
        Ok(s) => {
            eprintln!("\nBuild failed with exit code: {:?}", s.code());
            eprintln!("The source was updated but the binary was not rebuilt.");
            eprintln!("Run `cargo build --release --bin electro` manually to retry.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("\nBuild failed: {}", e);
            eprintln!("The source was updated but the binary was not rebuilt.");
            std::process::exit(1);
        }
    }

    // 8. Pop stash if we stashed earlier
    let stash_list = std::process::Command::new("git")
        .args(["stash", "list"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    if stash_list.contains("electro-update-autostash") {
        println!("Restoring stashed changes...");
        let _ = std::process::Command::new("git")
            .args(["stash", "pop"])
            .output();
    }

    Ok(())
}

#[cfg(feature = "codex-oauth")]
async fn run_auth_command(command: AuthCommands) -> Result<()> {
    match command {
        AuthCommands::Login { headless, output } => {
            println!("ELECTRO — OpenAI Codex OAuth Login");
            println!("Authenticating with your ChatGPT subscription...\n");

            match electro_codex_oauth::login(headless).await {
                Ok(store) => {
                    let email = store.email().await;
                    let expires = store.expires_in().await;
                    println!("\n  Authenticated successfully!");
                    println!("  Email:   {}", email);
                    println!("  Expires: {}", expires);
                    println!("  Model:   gpt-5.4 (default)");

                    if let Some(ref out_path) = output {
                        let path = std::path::PathBuf::from(out_path);
                        let tokens = store.get_tokens().await;
                        let dir = path.parent().unwrap_or(std::path::Path::new("."));
                        if let Err(e) = std::fs::create_dir_all(dir) {
                            eprintln!("Failed to create directory {}: {}", dir.display(), e);
                            std::process::exit(1);
                        }
                        let content = serde_json::to_string_pretty(&tokens).unwrap();
                        if let Err(e) = std::fs::write(&path, content) {
                            eprintln!("Failed to write {}: {}", path.display(), e);
                            std::process::exit(1);
                        }
                        println!("  Exported: {}", path.display());
                    }

                    println!("\n  Run `electro start` to go online.");
                }
                Err(e) => {
                    eprintln!("Authentication failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        AuthCommands::Status => {
            if !electro_codex_oauth::TokenStore::exists() {
                println!("Not authenticated. Run `electro auth login` to connect your ChatGPT account.");
                return Ok(());
            }
            match electro_codex_oauth::TokenStore::load() {
                Ok(store) => {
                    let email = store.email().await;
                    let account = store.account_id().await;
                    let expires = store.expires_in().await;
                    let expired = store.is_expired().await;
                    println!("ELECTRO — Codex OAuth Status");
                    println!("  Email:      {}", email);
                    println!("  Account:    {}", account);
                    println!(
                        "  Token:      {}",
                        if expired { "expired" } else { "valid" }
                    );
                    println!("  Expires in: {}", expires);
                }
                Err(e) => {
                    eprintln!("Failed to read OAuth tokens: {}", e);
                }
            }
        }
        AuthCommands::Logout => match electro_codex_oauth::TokenStore::delete() {
            Ok(()) => {
                println!("Logged out. OAuth tokens removed.");
            }
            Err(e) => {
                eprintln!("Failed to remove tokens: {}", e);
            }
        },
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_anthropic_key() {
        let result = detect_api_key("sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAA");
        assert_eq!(result.unwrap().provider, "anthropic");
    }

    #[test]
    fn detect_openai_key() {
        let result = detect_api_key("sk-proj-abcdefghijklmnopqrstuv");
        assert_eq!(result.unwrap().provider, "openai");
    }

    #[test]
    fn detect_unknown_key_returns_none() {
        assert!(detect_api_key("unknown-key-format-here").is_none());
    }

    #[test]
    fn placeholder_key_rejects_common_fakes() {
        assert!(is_placeholder_key("PASTE_YOUR_KEY_HERE"));
        assert!(is_placeholder_key("your_api_key"));
        assert!(!is_placeholder_key("sk-ant-api03-abc123"));
    }
}
