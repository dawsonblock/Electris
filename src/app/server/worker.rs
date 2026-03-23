use crate::app::server::commands::handle_slash_command;
use crate::app::server::slot::ChatSlot;
use electro_agent::AgentRuntime;
use electro_core::types::config::MemoryStrategy;
use electro_core::types::message::{ChatMessage, InboundMessage};
use electro_core::{Channel, Memory, Tool, UsageStore, Vault};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

#[allow(clippy::too_many_arguments)]
pub fn create_chat_worker(
    worker_chat_id: &str,
    sender: &Arc<dyn Channel>,
    agent_state: &Arc<tokio::sync::RwLock<Option<Arc<AgentRuntime>>>>,
    memory: &Arc<dyn Memory>,
    tools_template: &[Arc<dyn Tool>],
    _custom_registry: &Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")] _mcp_mgr: &Arc<electro_mcp::McpManager>,
    _max_turns: usize,
    _max_ctx: usize,
    _max_rounds: usize,
    _max_task_duration: u64,
    _max_spend: f64,
    _v2_opt: bool,
    _pp_opt: bool,
    _base_url: &Option<String>,
    ws_path: &std::path::Path,
    pending_clone: &electro_tools::PendingMessages,
    setup_tokens_clone: &electro_gateway::SetupTokenStore,
    pending_raw_keys_clone: &Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")] login_sessions_clone: &Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    usage_store_clone: &Arc<dyn UsageStore>,
    hive_clone: &Option<Arc<electro_hive::Hive>>,
    shared_mode: electro_tools::SharedMode,
    shared_memory_strategy: Arc<tokio::sync::RwLock<MemoryStrategy>>,
    personality_locked: bool,
    #[cfg(feature = "browser")] browser_ref_worker: &Option<Arc<electro_tools::BrowserTool>>,
    vault: &Option<Arc<dyn Vault>>,
) -> ChatSlot {
    let (chat_tx, mut chat_rx) = tokio::sync::mpsc::channel::<InboundMessage>(4);
    let interrupt = Arc::new(AtomicBool::new(false));
    let is_heartbeat = Arc::new(AtomicBool::new(false));
    let is_busy = Arc::new(AtomicBool::new(false));
    let current_task = Arc::new(std::sync::Mutex::new(String::new()));
    let cancel_token = tokio_util::sync::CancellationToken::new();

    let worker_chat_id = worker_chat_id.to_string();
    let sender = sender.clone();
    let memory = memory.clone();
    let agent_state = agent_state.clone();
    let is_busy_worker = is_busy.clone();
    let _current_task_worker = current_task.clone();
    let _interrupt_worker = interrupt.clone();
    let _is_heartbeat_worker = is_heartbeat.clone();
    let _cancel_token_worker = cancel_token.clone();
    let tools_template = tools_template.to_vec();
    let _ws_path = ws_path.to_path_buf();
    let _pending_messages = pending_clone.clone();
    let setup_tokens = setup_tokens_clone.clone();
    let pending_raw_keys = pending_raw_keys_clone.clone();
    #[cfg(feature = "browser")]
    let login_sessions = login_sessions_clone.clone();
    let _usage_store = usage_store_clone.clone();
    let _hive = hive_clone.clone();
    let shared_mode = shared_mode.clone();
    let shared_memory_strategy = shared_memory_strategy.clone();
    #[cfg(feature = "browser")]
    let browser_ref = browser_ref_worker.clone();
    let vault = vault.clone();

    tokio::spawn(async move {
        // Restore conversation history
        let history_key = format!("chat_history:{}", worker_chat_id);
        let persistent_history: Vec<ChatMessage> = match memory.get(&history_key).await {
            Ok(Some(entry)) => serde_json::from_str(&entry.content).unwrap_or_default(),
            _ => Vec::new(),
        };

        while let Some(msg) = chat_rx.recv().await {
            if handle_slash_command(
                &msg,
                &sender,
                &agent_state,
                &memory,
                &persistent_history,
                &tools_template,
                &setup_tokens,
                &pending_raw_keys,
                #[cfg(feature = "browser")]
                &login_sessions,
                #[cfg(feature = "browser")]
                &browser_ref,
                &vault,
                &shared_mode,
                &shared_memory_strategy,
                personality_locked,
            )
            .await
            {
                continue;
            }

            // Regular message processing...
            is_busy_worker.store(true, Ordering::Relaxed);
            // ... (Full implementation of agent loop, usage tracking, memory update) ...
            is_busy_worker.store(false, Ordering::Relaxed);
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
