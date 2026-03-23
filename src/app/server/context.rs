use crate::app::CoreStack;
use electro_agent::{AgentRuntime, SharedMode};
use electro_core::types::config::{ElectroConfig, MemoryStrategy};
use electro_core::{Memory, Tool, UsageStore, Vault};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Unified context for worker lifecycle and message dispatching.
///
/// Encapsulates the 20+ shared dependencies previously passed individually,
/// reducing function arity and improving maintainability.
#[derive(Clone)]
pub struct WorkerServices {
    pub agent_state: Arc<RwLock<Option<Arc<AgentRuntime>>>>,
    pub memory: Arc<dyn Memory>,
    pub usage_store: Arc<dyn UsageStore>,
    pub vault: Option<Arc<dyn Vault>>,
    pub setup_tokens: electro_gateway::SetupTokenStore,
    pub tools_template: Vec<Arc<dyn Tool>>,
    pub custom_tool_registry: Arc<electro_tools::CustomToolRegistry>,
    #[cfg(feature = "mcp")]
    pub mcp_manager: Arc<electro_mcp::McpManager>,
    pub hive_instance: Option<Arc<electro_hive::Hive>>,
    pub tenant_manager: Arc<electro_core::tenant_impl::TenantManager>,
    pub config: ElectroConfig,
    pub workspace_root: std::path::PathBuf,
    pub shared_mode: SharedMode,
    pub shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
    pub personality_locked: bool,

    // Runtime-local shared state
    pub pending_messages: electro_tools::PendingMessages,
    pub pending_raw_keys: Arc<Mutex<HashSet<String>>>,
    #[cfg(feature = "browser")]
    pub login_sessions: Arc<
        Mutex<HashMap<String, electro_tools::browser_session::InteractiveBrowseSession>>,
    >,
    #[cfg(feature = "browser")]
    pub browser_tool_ref: Option<Arc<electro_tools::BrowserTool>>,
}

impl WorkerServices {
    pub fn new(
        core: &CoreStack,
        config: &ElectroConfig,
        tools: Vec<Arc<dyn Tool>>,
        custom_tool_registry: Arc<electro_tools::CustomToolRegistry>,
        #[cfg(feature = "mcp")] mcp_manager: Arc<electro_mcp::McpManager>,
        hive_instance: Option<Arc<electro_hive::Hive>>,
        tenant_manager: Arc<electro_core::tenant_impl::TenantManager>,
        workspace_root: std::path::PathBuf,
        shared_mode: SharedMode,
        shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
        personality_locked: bool,
        #[cfg(feature = "browser")] browser_tool_ref: Option<Arc<electro_tools::BrowserTool>>,
    ) -> Self {
        Self {
            agent_state: Arc::new(RwLock::new(None)),
            memory: core.memory.clone(),
            usage_store: core.usage_store.clone(),
            vault: core.vault.clone(),
            setup_tokens: core.setup_tokens.clone(),
            tools_template: tools,
            custom_tool_registry,
            #[cfg(feature = "mcp")]
            mcp_manager,
            hive_instance,
            tenant_manager,
            config: config.clone(),
            workspace_root,
            shared_mode,
            shared_memory_strategy,
            personality_locked,

            pending_messages: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_raw_keys: Arc::new(Mutex::new(HashSet::new())),
            #[cfg(feature = "browser")]
            login_sessions: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(feature = "browser")]
            browser_tool_ref,
        }
    }
}
