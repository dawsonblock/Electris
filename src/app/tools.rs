use electro_core::types::config::ToolsConfig;
use electro_core::{Channel, Memory, SetupLinkGenerator, Tool, UsageStore, Vault};
use electro_tools::{BrowserTool, CustomToolRegistry, PendingMessages, SharedMode};
use std::sync::Arc;

#[allow(clippy::too_many_arguments)]
pub fn init_tools(
    config: &ToolsConfig,
    channel: Option<Arc<dyn Channel>>,
    pending_messages: Option<PendingMessages>,
    memory: Option<Arc<dyn Memory>>,
    link_gen: Option<Arc<dyn SetupLinkGenerator>>,
    usage_store: Option<Arc<dyn UsageStore>>,
    shared_mode: Option<SharedMode>,
    vault: Option<Arc<dyn Vault>>,
) -> (Vec<Arc<dyn Tool>>, Option<Arc<BrowserTool>>) {
    #[cfg(feature = "browser")]
    {
        let (mut tools, browser_ref) = electro_tools::create_tools_with_browser(
            config,
            channel,
            pending_messages,
            memory,
            link_gen,
            usage_store,
            shared_mode,
            vault,
        );
        // Load custom tools
        let custom_registry = CustomToolRegistry::new();
        let custom_tools = custom_registry.load_tools();
        if !custom_tools.is_empty() {
            tracing::info!(count = custom_tools.len(), "Custom script tools loaded");
            tools.extend(custom_tools);
        }
        (tools, browser_ref)
    }
    #[cfg(not(feature = "browser"))]
    {
        let mut tools = electro_tools::create_tools(
            config,
            channel,
            pending_messages,
            memory,
            link_gen,
            usage_store,
            shared_mode,
            vault,
        );
        // Load custom tools
        let custom_registry = CustomToolRegistry::new();
        let custom_tools = custom_registry.load_tools();
        if !custom_tools.is_empty() {
            tracing::info!(count = custom_tools.len(), "Custom script tools loaded");
            tools.extend(custom_tools);
        }
        (tools, None)
    }
}
