//! Mode switch tool — toggles Electro's personality mode at runtime.
//!
//! The agent can use this tool to switch between PLAY mode (:3), WORK mode (>:3), and PRO mode (professional).

use std::sync::Arc;

use async_trait::async_trait;
use electro_core::types::config::ElectroMode;
use electro_core::types::error::ElectroError;
use electro_core::{Tool, ToolContext, ToolInput, ToolOutput};
use electro_core::policy::CapabilityPolicy;

use tokio::sync::RwLock;

/// Shared runtime mode state. Wrap this in `Arc<RwLock<ElectroMode>>` and pass
/// the same handle to the tool AND the system-prompt builder so both see
/// real-time updates.
pub type SharedMode = Arc<RwLock<ElectroMode>>;

pub struct ModeSwitchTool {
    mode: SharedMode,
}

impl ModeSwitchTool {
    pub fn new(mode: SharedMode) -> Self {
        Self { mode }
    }
}

#[async_trait]
impl Tool for ModeSwitchTool {
    fn name(&self) -> &str {
        "mode_switch"
    }

    fn description(&self) -> &str {
        "Switch Tem's personality mode between PLAY (warm, chaotic, :3), \
         WORK (sharp, analytical, >:3), or PRO (professional, no emoticons). \
         Use this when the user asks to change the vibe or when a task requires a different energy."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["play", "work", "pro"],
                    "description": "The personality mode to switch to: 'play' for warm/chaotic energy, 'work' for sharp/analytical precision, 'pro' for professional/business tone"
                }
            },
            "required": ["mode"]
        })
    }

    fn declarations(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            file_access: Vec::new(),
            network_access: electro_core::net_policy::NetworkPolicy::Blocked,
            shell_access: electro_core::policy::ShellPolicy::Blocked,
browser_access: electro_core::policy::BrowserPolicy::Blocked,
        }
    }

    async fn execute(
        &self,
        input: ToolInput,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ElectroError> {
        let mode_str = input
            .arguments
            .get("mode")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ElectroError::Tool("Missing required parameter: mode".into()))?;

        let new_mode = match mode_str.to_lowercase().as_str() {
            "play" => ElectroMode::Play,
            "work" => ElectroMode::Work,
            "pro" => ElectroMode::Pro,
            other => {
                return Ok(ToolOutput {
                    content: format!("Unknown mode '{}'. Valid modes: play, work, pro", other),
                    is_error: true,
                });
            }
        };

        let old_mode = {
            let mut guard = self.mode.write().await;
            let old = *guard;
            *guard = new_mode;
            old
        };

        tracing::info!(from = %old_mode, to = %new_mode, "Electro personality mode switched");

        let message = match new_mode {
            ElectroMode::Play => "Mode switched to PLAY! Let's have some fun! :3".to_string(),
            ElectroMode::Work => "Mode switched to WORK. Ready to execute. >:3".to_string(),
            ElectroMode::Pro => "Mode switched to PRO. Professional mode engaged.".to_string(),
            // None is never reachable here — the tool only accepts play/work/pro
            // and is not registered when personality is None (locked).
            ElectroMode::None => "Mode unchanged.".to_string(),
        };

        Ok(ToolOutput {
            content: message,
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_ctx() -> ToolContext {
        ToolContext {
            workspace_path: PathBuf::from("/tmp/test"),
            session_id: "test-session".to_string(),
            chat_id: "chat-123".to_string(),
        }
    }

    fn make_input(args: serde_json::Value) -> ToolInput {
        ToolInput {
            name: "mode_switch".to_string(),
            arguments: args,
        }
    }

    #[tokio::test]
    async fn switch_to_play() {
        let mode = Arc::new(RwLock::new(ElectroMode::Work));
        let tool = ModeSwitchTool::new(mode.clone());
        let ctx = test_ctx();

        let input = make_input(serde_json::json!({"mode": "play"}));
        let output = tool.execute(input, &ctx).await.unwrap();
        assert!(!output.is_error);
        assert!(output.content.contains("PLAY"));
        assert!(output.content.contains(":3"));
        assert_eq!(*mode.read().await, ElectroMode::Play);
    }

    #[tokio::test]
    async fn switch_to_work() {
        let mode = Arc::new(RwLock::new(ElectroMode::Play));
        let tool = ModeSwitchTool::new(mode.clone());
        let ctx = test_ctx();

        let input = make_input(serde_json::json!({"mode": "work"}));
        let output = tool.execute(input, &ctx).await.unwrap();
        assert!(!output.is_error);
        assert!(output.content.contains("WORK"));
        assert!(output.content.contains(">:3"));
        assert_eq!(*mode.read().await, ElectroMode::Work);
    }

    #[tokio::test]
    async fn switch_to_pro() {
        let mode = Arc::new(RwLock::new(ElectroMode::Play));
        let tool = ModeSwitchTool::new(mode.clone());
        let ctx = test_ctx();

        let input = make_input(serde_json::json!({"mode": "pro"}));
        let output = tool.execute(input, &ctx).await.unwrap();
        assert!(!output.is_error);
        assert!(output.content.contains("PRO"));
        assert!(!output.content.contains(":3"));
        assert_eq!(*mode.read().await, ElectroMode::Pro);
    }

    #[tokio::test]
    async fn invalid_mode() {
        let mode = Arc::new(RwLock::new(ElectroMode::Play));
        let tool = ModeSwitchTool::new(mode.clone());
        let ctx = test_ctx();

        let input = make_input(serde_json::json!({"mode": "chaos"}));
        let output = tool.execute(input, &ctx).await.unwrap();
        assert!(output.is_error);
        assert!(output.content.contains("Unknown mode"));
        // Mode should not change
        assert_eq!(*mode.read().await, ElectroMode::Play);
    }

    #[tokio::test]
    async fn missing_mode_param() {
        let mode = Arc::new(RwLock::new(ElectroMode::Play));
        let tool = ModeSwitchTool::new(mode.clone());
        let ctx = test_ctx();

        let input = make_input(serde_json::json!({}));
        let result = tool.execute(input, &ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn tool_metadata() {
        let mode = Arc::new(RwLock::new(ElectroMode::Play));
        let tool = ModeSwitchTool::new(mode);

        assert_eq!(tool.name(), "mode_switch");
        assert!(tool.description().contains("personality"));
        use electro_core::policy::ShellPolicy;
        assert_eq!(tool.declarations().shell_access, ShellPolicy::Blocked);

        let schema = tool.parameters_schema();
        let props = schema.get("properties").unwrap();
        assert!(props.get("mode").is_some());
    }
}
