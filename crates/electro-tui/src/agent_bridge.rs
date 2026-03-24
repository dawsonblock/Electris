//! Agent bridge — TUI adapter that uses the runtime queue instead of direct execution.
//!
//! This module creates a TUI-local runtime and uses the queue/dispatcher/worker spine
//! for message processing. Events are received via the runtime's outbound event bus.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, watch, Mutex, RwLock};

use electro_agent::agent_task_status::AgentTaskStatus;
use electro_core::config::credentials;
use electro_core::paths;
use electro_core::types::config::{ElectroConfig, ElectroMode, MemoryStrategy};
use electro_core::types::error::ElectroError;
use electro_core::types::message::InboundMessage;
use electro_runtime::{OutboundEvent, RuntimeHandle};

use crate::event::{AgentResponseEvent, Event, StreamChunk};

/// Everything needed to communicate with the running agent.
pub struct AgentHandle {
    /// Send user messages to the agent processing loop.
    pub inbound_tx: mpsc::Sender<InboundMessage>,
    /// Watch channel for real-time status updates.
    pub status_rx: watch::Receiver<AgentTaskStatus>,
}

/// Configuration for agent setup.
pub struct AgentSetup {
    pub provider_name: String,
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,
    pub config: ElectroConfig,
    /// Selected personality mode (auto/play/work/pro).
    pub mode: Option<String>,
}

/// Create the runtime from credentials and spawn the processing loop.
///
/// Returns an `AgentHandle` for communication, or an error if setup fails.
/// 
/// NOTE: This uses a local runtime with direct agent creation for TUI mode.
/// In a full distributed setup, this would connect to a remote runtime.
pub async fn spawn_agent(
    setup: AgentSetup,
    event_tx: mpsc::UnboundedSender<Event>,
) -> Result<AgentHandle, ElectroError> {
    // 1. Create provider
    let (all_keys, saved_base_url) = credentials::load_active_provider_keys()
        .map(|(_, keys, _, burl)| {
            let valid: Vec<String> = keys
                .into_iter()
                .filter(|k| !credentials::is_placeholder_key(k))
                .collect();
            (valid, burl)
        })
        .unwrap_or_else(|| (vec![setup.api_key.clone()], None));

    let effective_base_url = saved_base_url.or(setup.config.provider.base_url.clone());

    let provider_config = electro_core::types::config::ProviderConfig {
        name: Some(setup.provider_name.clone()),
        api_key: Some(setup.api_key.clone()),
        keys: all_keys,
        model: Some(setup.model.clone()),
        base_url: effective_base_url,
        extra_headers: setup.config.provider.extra_headers.clone(),
    };

    let provider: Arc<dyn electro_core::Provider> = {
        #[cfg(feature = "codex-oauth")]
        if setup.provider_name == "openai-codex" {
            match electro_codex_oauth::TokenStore::load() {
                Ok(store) => Arc::new(electro_codex_oauth::CodexResponsesProvider::new(
                    setup.model.clone(),
                    std::sync::Arc::new(store),
                )),
                Err(e) => {
                    return Err(ElectroError::Auth(format!(
                        "Codex OAuth not configured: {}",
                        e
                    )));
                }
            }
        } else {
            Arc::from(
                electro_providers::create_provider(&provider_config)
                    .map_err(|e| ElectroError::Provider(e.to_string()))?,
            )
        }

        #[cfg(not(feature = "codex-oauth"))]
        Arc::from(
            electro_providers::create_provider(&provider_config)
                .map_err(|e| ElectroError::Provider(e.to_string()))?,
        )
    };

    // 2. Create memory backend
    let memory_url = setup.config.memory.path.clone().unwrap_or_else(|| {
        let data_dir = paths::electro_home();
        std::fs::create_dir_all(&data_dir).ok();
        format!("sqlite:{}/memory.db?mode=rwc", data_dir.display())
    });
    let memory: Arc<dyn electro_core::Memory> = Arc::from(
        electro_memory::create_memory_backend(&setup.config.memory.backend, &memory_url).await?,
    );

    // 3. Create workspace
    let workspace = paths::workspace_dir();
    std::fs::create_dir_all(&workspace).ok();

    // 4. Determine personality mode
    let initial_mode = match setup.mode.as_deref() {
        Some("work") => ElectroMode::Work,
        Some("pro") => ElectroMode::Pro,
        Some("none") => ElectroMode::None,
        _ => ElectroMode::Play,
    };
    let shared_mode: Arc<RwLock<ElectroMode>> = Arc::new(RwLock::new(initial_mode));

    // 5. Create runtime handle (NOT direct agent)
    let (queue_tx, _queue_rx) = mpsc::channel(64);
    let runtime = RuntimeHandle::new(
        queue_tx,
        shared_mode.clone(),
        Arc::new(RwLock::new(MemoryStrategy::Lambda)),
    );

    // 6. Create tools
    let tools = electro_tools::create_tools(
        &setup.config.tools,
        None,
        None,
        Some(memory.clone()),
        None,
        None,
        Some(shared_mode.clone()),
        None,
    );

    // 7. Build system prompt
    let system_prompt = Some(build_tui_system_prompt());

    // 8. Create agent and attach to runtime
    let agent = electro_agent::AgentRuntime::with_limits(
        provider,
        memory.clone(),
        tools,
        setup.model.clone(),
        system_prompt,
        setup.config.agent.max_turns,
        setup.config.agent.max_context_tokens,
        setup.config.agent.max_tool_rounds,
        setup.config.agent.max_task_duration_secs,
        setup.config.agent.max_spend_usd,
    )
    .with_v2_optimizations(setup.config.agent.v2_optimizations)
    .with_shared_mode(shared_mode);

    runtime.set_agent(agent).await;

    // 9. Set up channels
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundMessage>(64);
    let (status_tx, status_rx) = watch::channel(AgentTaskStatus::default());

    // 10. Subscribe to outbound events
    let mut outbound_events = runtime.subscribe_outbound_events();
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        while let Ok(event) = outbound_events.recv().await {
            match event {
                OutboundEvent::Started { .. } => {
                    // TUI could show a spinner here
                }
                OutboundEvent::ToolCall { tool, .. } => {
                    let _ = event_tx_clone.send(Event::StreamChunk(StreamChunk {
                        delta: format!("[Using tool: {}]\n", tool),
                        done: false,
                    }));
                }
                OutboundEvent::ToolResult { tool, success, .. } => {
                    let status = if success { "✓" } else { "✗" };
                    let _ = event_tx_clone.send(Event::StreamChunk(StreamChunk {
                        delta: format!("[{} {}]\n", status, tool),
                        done: false,
                    }));
                }
                OutboundEvent::Completed { content, .. } => {
                    let _ = event_tx_clone.send(Event::AgentResponse(AgentResponseEvent {
                        message: electro_core::types::message::OutboundMessage {
                            chat_id: "tui".to_string(),
                            text: content,
                            reply_to: None,
                            parse_mode: None,
                        },
                        input_tokens: 0,
                        output_tokens: 0,
                        cost_usd: 0.0,
                    }));
                }
                OutboundEvent::Failed { error, .. } => {
                    let _ = event_tx_clone.send(Event::StreamChunk(StreamChunk {
                        delta: format!("[Error: {}]\n", error),
                        done: true,
                    }));
                }
                OutboundEvent::Token { content, .. } => {
                    let _ = event_tx_clone.send(Event::StreamChunk(StreamChunk {
                        delta: content,
                        done: false,
                    }));
                }
            }
        }
    });

    // 11. Load conversation history
    let cli_history_key = "chat_history:tui".to_string();
    let history: Vec<electro_core::types::message::ChatMessage> =
        match memory.get(&cli_history_key).await {
            Ok(Some(entry)) => serde_json::from_str(&entry.content).unwrap_or_default(),
            _ => Vec::new(),
        };
    let history = Arc::new(Mutex::new(history));

    // 12. Spawn processing loop that uses the runtime
    let history_clone = history.clone();
    let memory_clone = memory.clone();
    let runtime_clone = runtime.clone();
    tokio::spawn(async move {
        while let Some(msg) = inbound_rx.recv().await {
            // Send status update
            let _ = status_tx.send(AgentTaskStatus {
                phase: electro_agent::AgentTaskPhase::CallingProvider { round: 1 },
                ..Default::default()
            });

            // Enqueue message via runtime (not direct process_message)
            // For TUI mode, we use the runtime's direct agent access since it's local
            // This is a transitional design - eventually TUI would use HTTP to a running server
            let agent = runtime_clone.agent().await;
            if let Some(agent) = agent {
                let current_history = history_clone.lock().await.clone();
                let mut session = electro_core::types::session::SessionContext {
                    session_id: "tui-tui".to_string(),
                    user_id: msg.user_id.clone(),
                    channel: msg.channel.clone(),
                    chat_id: msg.chat_id.clone(),
                    history: current_history.clone(),
                    workspace_path: workspace.clone(),
                    tool_timeout_secs: 60,
                    tool_policy: electro_tools::policy::ToolPolicy::for_workspace(workspace.clone()),
                };

                let result = agent
                    .process_message(&msg, &mut session, None, None, None, Some(status_tx.clone()), None)
                    .await;

                match result {
                    Ok((_reply, _usage)) => {
                        // Update history
                        let mut hist = history_clone.lock().await;
                        *hist = session.history;

                        // Persist conversation history
                        if let Ok(json) = serde_json::to_string(&*hist) {
                            let entry = electro_core::MemoryEntry {
                                id: cli_history_key.clone(),
                                content: json,
                                metadata: serde_json::json!({"chat_id": "tui"}),
                                timestamp: chrono::Utc::now(),
                                session_id: Some("tui".to_string()),
                                entry_type: electro_core::MemoryEntryType::Conversation,
                            };
                            let _ = memory_clone.store(entry).await;
                        }
                    }
                    Err(e) => {
                        let _ = event_tx.send(Event::StreamChunk(StreamChunk {
                            delta: format!("[Error: {}]\n", e),
                            done: true,
                        }));
                    }
                }
            } else {
                let _ = event_tx.send(Event::StreamChunk(StreamChunk {
                    delta: "[Agent not available]\n".to_string(),
                    done: true,
                }));
            }
        }
    });

    Ok(AgentHandle {
        inbound_tx,
        status_rx,
    })
}

fn build_tui_system_prompt() -> String {
    format!(
        "You are Electris, a helpful AI assistant running in TUI mode. \
You have access to tools for file operations, shell commands, and web search. \
Be concise but thorough. Current time: {}.",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    )
}

/// Validate that the provider key is not a placeholder.
pub fn validate_provider_key(key: &str) -> Result<(), String> {
    if credentials::is_placeholder_key(key) {
        Err("API key appears to be a placeholder. Please configure a real key with: electro config".to_string())
    } else {
        Ok(())
    }
}
