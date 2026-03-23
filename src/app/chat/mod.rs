use std::sync::Arc;
use anyhow::Result;
use crate::app::{init_core_stack, create_agent, create_provider, init_tools, check_hive_enabled};
use electro_core::paths;
use electro_core::types::config::{ElectroConfig, MemoryStrategy};
use electro_core::types::model_registry::{default_model};
use electro_core::config::credentials::{load_saved_credentials, load_active_provider_keys, save_credentials, detect_api_key, is_placeholder_key};
use crate::app::onboarding::{build_system_prompt, decrypt_otk_blob, onboarding_message_with_link, ONBOARDING_REFERENCE, validate_provider_key};
use crate::app::cli::{list_configured_providers, remove_provider, handle_model_command};
use electro_core::Channel;
use crate::bootstrap::{SecretCensorChannel};
use electro_core::types::session::SessionContext;

pub async fn run_chat_mode(
    config: ElectroConfig,
    _config_path: Option<&std::path::Path>,
) -> Result<()> {
    println!("ELECTRO interactive chat");
    println!("Type '/quit' or '/exit' to quit.\n");

    let hive_enabled = check_hive_enabled().await;

    // ── Core Stack ──
    let core = init_core_stack(&config).await?;

    // ── Resolve Initial Credentials ──
    let credentials = resolve_credentials(&config);

    // ── CLI Channel ──
    let workspace = paths::workspace_dir();
    std::fs::create_dir_all(&workspace).ok();
    let mut cli_channel = electro_channels::CliChannel::new(workspace.clone());
    let cli_rx = cli_channel.take_receiver();
    cli_channel.start().await?;
    let cli_arc: Arc<dyn electro_core::Channel> = Arc::new(cli_channel);

    // ── Tools ──
    let pending_messages = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let censored_cli: Arc<dyn Channel> = Arc::new(SecretCensorChannel {
        inner: cli_arc.clone(),
    });
    let shared_mode = Arc::new(tokio::sync::RwLock::new(config.mode));
    let shared_memory_strategy = Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda));

    let (tools, _browser_ref) = init_tools(
        &config.tools,
        Some(censored_cli),
        Some(pending_messages.clone()),
        Some(core.memory.clone()),
        Some(Arc::new(core.setup_tokens.clone()) as Arc<dyn electro_core::SetupLinkGenerator>),
        Some(core.usage_store.clone()),
        Some(shared_mode.clone()),
        core.vault.clone(),
    );

    // ── Agent ──
    let mut agent_opt = None;
    if let Some((pname, key, model)) = credentials {
        if !is_placeholder_key(&key) {
             let provider_config = build_provider_config(&config, &pname, &key, &model);
             match create_provider(&provider_config, &pname, &model).await {
                 Ok(provider) => {
                     agent_opt = Some(create_agent(
                         &config,
                         provider,
                         core.memory.clone(),
                         tools.clone(),
                         model.clone(),
                         Some(build_system_prompt()),
                         hive_enabled,
                         shared_mode.clone(),
                         shared_memory_strategy.clone(),
                     ).await);
                     println!("Connected to {} (model: {})", pname, model);
                 }
                 Err(e) => eprintln!("Failed to create provider: {}", e),
             }
        }
    }

    if agent_opt.is_none() {
        println!("No API key configured — running in onboarding mode.");
        let otk = core.setup_tokens.generate("cli").await;
        let link = format!("https://dawsonblock.github.io/Electro/setup#{}", hex::encode(otk));
        println!("\n{}", onboarding_message_with_link(&link));
        println!("\n{}", ONBOARDING_REFERENCE);
    }
    println!("---\n");

    // ── Message Loop ──
    let mut rx = cli_rx.expect("CLI channel receiver unavailable");
    
    // Restore history
    let mut history = restore_history(core.memory.clone()).await;
    if !history.is_empty() {
        println!("  Restored {} messages from previous session.", history.len());
    }

    while let Some(msg) = rx.recv().await {
        let text = msg.text.as_deref().unwrap_or("");
        let lower = text.trim().to_lowercase();

        if lower == "/quit" || lower == "/exit" {
            break;
        }

        if lower == "/help" || lower == "?" {
            print_help();
            continue;
        }

        // Handle Chat Commands
        if handle_chat_command(
            &lower, 
            text, 
            &core, 
            &mut agent_opt, 
            &config, 
            &tools, 
            &hive_enabled,
            &shared_mode,
            &shared_memory_strategy
        ).await? {
            continue;
        }

        // Handle Encrypted Blobs
        if text.trim().starts_with("enc:v1:") {
            if let Some(agent) = handle_encrypted_blob(text, &core, &config, &tools, &hive_enabled, &shared_mode, &shared_memory_strategy).await? {
                agent_opt = Some(agent);
            }
            continue;
        }

        // Normal Agent Interaction
        if let Some(ref mut agent) = agent_opt {
             let mut session_ctx = SessionContext {
                 session_id: "cli".to_string(),
                 channel: "cli".to_string(),
                 chat_id: msg.chat_id.clone(),
                 user_id: msg.user_id.clone(),
                 history: history.clone(),
                 workspace_path: workspace.clone(),
             };

             let (status_tx, mut status_rx) = tokio::sync::watch::channel(electro_agent::AgentTaskStatus::default());
             
             // Monitor status updates in background
             let status_handle = tokio::spawn(async move {
                 use std::io::Write;
                 let mut last_phase = None;
                 while status_rx.changed().await.is_ok() {
                     let status = status_rx.borrow().clone();
                     if Some(status.phase.clone()) != last_phase {
                         match &status.phase {
                             electro_agent::AgentTaskPhase::Preparing => print!("\r[Preparing] "),
                             electro_agent::AgentTaskPhase::Classifying => print!("\r[Classifying] "),
                             electro_agent::AgentTaskPhase::CallingProvider { round } => print!("\r[Round {}: Calling LLM] ", round),
                             electro_agent::AgentTaskPhase::ExecutingTool { round, tool_name, .. } => print!("\r[Round {}: Tool {}] ", round, tool_name),
                             electro_agent::AgentTaskPhase::Finishing => print!("\r[Finishing] "),
                             electro_agent::AgentTaskPhase::Done => print!("\r[Done] "),
                             electro_agent::AgentTaskPhase::Interrupted { .. } => print!("\r[Interrupted] "),
                         }
                         std::io::stdout().flush().ok();
                         last_phase = Some(status.phase.clone());
                     }
                 }
             });

             match agent.process_message(&msg, &mut session_ctx, None, None, None, Some(status_tx), None).await {
                 Ok((reply, usage)) => {
                     status_handle.abort();
                     println!("\r{}", reply.text);
                     println!("\nUsage: {} calls, {} tokens, ${:.4}", usage.api_calls, usage.combined_tokens(), usage.total_cost_usd);
                     
                     // Update history and sync back
                     history = session_ctx.history;
                     
                     // Save to memory
                     let history_json = serde_json::to_string(&history)?;
                     core.memory.store(electro_core::MemoryEntry {
                         id: "chat_history:cli".to_string(),
                         content: history_json,
                         metadata: serde_json::json!({}),
                         timestamp: chrono::Utc::now(),
                         session_id: Some("cli".to_string()),
                         entry_type: electro_core::MemoryEntryType::Conversation,
                     }).await.ok();
                 }
                 Err(e) => {
                     status_handle.abort();
                     eprintln!("\nError: {}", e);
                 }
             }
        } else {
            println!("Agent offline. Use /help to see commands or add an API key.");
        }
        print!("electro> ");
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

    Ok(())
}

fn resolve_credentials(config: &ElectroConfig) -> Option<(String, String, String)> {
    if let Some(ref key) = config.provider.api_key {
        if !key.is_empty() && !key.starts_with("${") {
            let name = config.provider.name.clone().unwrap_or_else(|| "anthropic".to_string());
            let model = config.provider.model.clone().unwrap_or_else(|| default_model(&name).to_string());
            return Some((name, key.clone(), model));
        }
    }
    load_saved_credentials()
}

fn build_provider_config(config: &ElectroConfig, pname: &str, key: &str, model: &str) -> electro_core::types::config::ProviderConfig {
    let (all_keys, saved_base_url) = load_active_provider_keys()
        .map(|(_, keys, _, burl)| {
            let valid: Vec<String> = keys.into_iter().filter(|k| !is_placeholder_key(k)).collect();
            (valid, burl)
        })
        .unwrap_or_else(|| (vec![key.to_string()], None));
    
    let effective_base_url = saved_base_url.or_else(|| config.provider.base_url.clone());
    
    electro_core::types::config::ProviderConfig {
        name: Some(pname.to_string()),
        api_key: Some(key.to_string()),
        keys: all_keys,
        model: Some(model.to_string()),
        base_url: effective_base_url,
        extra_headers: config.provider.extra_headers.clone(),
    }
}

async fn restore_history(memory: Arc<dyn electro_core::Memory>) -> Vec<electro_core::types::message::ChatMessage> {
    match memory.get("chat_history:cli").await {
        Ok(Some(entry)) => serde_json::from_str(&entry.content).unwrap_or_default(),
        _ => Vec::new(),
    }
}

async fn handle_chat_command(
    lower: &str,
    text: &str,
    core: &crate::app::CoreStack,
    agent_opt: &mut Option<electro_agent::AgentRuntime>,
    config: &ElectroConfig,
    tools: &[Arc<dyn electro_core::Tool>],
    hive_enabled: &bool,
    shared_mode: &Arc<tokio::sync::RwLock<electro_core::types::config::ElectroMode>>,
    shared_memory_strategy: &Arc<tokio::sync::RwLock<MemoryStrategy>>,
) -> Result<bool> {
    if lower == "/addkey" {
        let otk = core.setup_tokens.generate("cli").await;
        let link = format!("https://dawsonblock.github.io/Electro/setup#{}", hex::encode(otk));
        println!("\nSecure key setup: {}\n", link);
        return Ok(true);
    }
    if lower == "/keys" {
        println!("\n{}\n", list_configured_providers());
        return Ok(true);
    }
    if lower.starts_with("/removekey") {
        let provider = text["/removekey".len()..].trim();
        println!("\n{}\n", remove_provider(provider));
        return Ok(true); 
    }
    if lower == "/usage" {
        // ... usage logic ...
        return Ok(true);
    }
    if lower == "/model" || lower.starts_with("/model ") {
        let args = if lower == "/model" { "" } else { text["/model".len()..].trim() };
        println!("\n{}\n", handle_model_command(args));
        return Ok(true);
    }
    Ok(false)
}

async fn handle_encrypted_blob(
    text: &str,
    core: &crate::app::CoreStack,
    config: &ElectroConfig,
    tools: &[Arc<dyn electro_core::Tool>],
    _hive_enabled: &bool,
    shared_mode: &Arc<tokio::sync::RwLock<electro_core::types::config::ElectroMode>>,
    shared_memory_strategy: &Arc<tokio::sync::RwLock<MemoryStrategy>>,
) -> Result<Option<electro_agent::AgentRuntime>> {
    let blob = &text.trim()["enc:v1:".len()..];
    if let Ok(key_text) = decrypt_otk_blob(blob, &core.setup_tokens, "cli").await {
        if let Some(cred) = detect_api_key(&key_text) {
             let model = default_model(cred.provider).to_string();
             let provider_config = build_provider_config(config, cred.provider, &cred.api_key, &model);
             if let Ok(provider) = validate_provider_key(&provider_config).await {
                 save_credentials(cred.provider, &cred.api_key, &model, cred.base_url.as_deref()).await.ok();
                 return Ok(Some(create_agent(
                     config,
                     provider,
                     core.memory.clone(),
                     tools.to_vec(),
                     model,
                     Some(build_system_prompt()),
                     *_hive_enabled,
                     shared_mode.clone(),
                     shared_memory_strategy.clone(),
                 ).await));
             }
        }
    }
    Ok(None)
}

fn print_help() {
    println!("Available commands:");
    println!("  /quit, /exit         - Quit the application");
    println!("  /addkey              - Securely add an API key using OTK flow");
    println!("  /keys                - List configured API keys");
    println!("  /removekey <name>    - Remove a configured API key");
    println!("  /usage               - Show usage summary for this chat");
    println!("  /model <name>        - Switch active model");
    println!("  /memory [lambda|echo]- Switch memory strategy");
    println!("  /help                - Show this help message");
}
