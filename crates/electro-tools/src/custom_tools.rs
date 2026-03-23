//! Custom script-based tools — agent-authored tools that persist across sessions.
//!
//! The agent can create new tools at runtime by writing scripts (bash/python/node).
//! Tools are saved to `~/.electro/custom-tools/` with a companion metadata JSON file.
//! Hardened default: loading and creating custom tools is disabled unless
//! `ELECTRO_ENABLE_CUSTOM_TOOLS=1` is explicitly set by the operator.
//! A `ScriptToolAdapter` wraps each script as a native `Tool` trait implementation.

use async_trait::async_trait;
use electro_core::paths;
use electro_core::policy::CapabilityPolicy;
use electro_core::types::error::ElectroError;
use electro_core::{Tool, ToolContext, ToolInput, ToolOutput};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tracing::{debug, info, warn};

#[cfg(test)]
fn custom_tools_enabled() -> bool {
    true
}

#[cfg(not(test))]
fn custom_tools_enabled() -> bool {
    matches!(
        std::env::var("ELECTRO_ENABLE_CUSTOM_TOOLS")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

/// Metadata for a custom script tool, stored as `{name}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptToolMeta {
    pub name: String,
    pub description: String,
    pub language: String,              // "bash", "python", "node"
    pub parameters: serde_json::Value, // JSON Schema
}

/// Adapter that wraps a script file as a ELECTRO Tool.
pub struct ScriptToolAdapter {
    meta: ScriptToolMeta,
    script_path: PathBuf,
}

impl ScriptToolAdapter {
    /// Load a script tool from its metadata file.
    pub fn from_meta_file(meta_path: &Path) -> Result<Self, ElectroError> {
        let content = std::fs::read_to_string(meta_path).map_err(|e| {
            ElectroError::Tool(format!(
                "Cannot read tool metadata {}: {}",
                meta_path.display(),
                e
            ))
        })?;
        let meta: ScriptToolMeta = serde_json::from_str(&content).map_err(|e| {
            ElectroError::Tool(format!(
                "Invalid tool metadata {}: {}",
                meta_path.display(),
                e
            ))
        })?;

        // Resolve script path (same directory, same name, language extension)
        let ext = match meta.language.as_str() {
            "python" => "py",
            "node" => "js",
            _ => "sh", // default to bash
        };
        let script_path = meta_path.with_extension(ext);
        if !script_path.exists() {
            return Err(ElectroError::Tool(format!(
                "Script file not found: {}",
                script_path.display()
            )));
        }

        Ok(Self { meta, script_path })
    }
}

#[async_trait]
impl Tool for ScriptToolAdapter {
    fn name(&self) -> &str {
        &self.meta.name
    }

    fn description(&self) -> &str {
        &self.meta.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.meta.parameters.clone()
    }

    fn declarations(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            file_access: vec![],
            network_access: electro_core::net_policy::NetworkPolicy::Blocked,
            shell_access: electro_core::policy::ShellPolicy::Allowed,
            browser_access: electro_core::policy::BrowserPolicy::Blocked, // scripts require shell
        }
    }

    async fn execute(
        &self,
        input: ToolInput,
        _ctx: &ToolContext,
    ) -> Result<ToolOutput, ElectroError> {
        let interpreter = match self.meta.language.as_str() {
            "python" => "python3",
            "node" => "node",
            _ => "bash",
        };

        debug!(
            tool = %self.meta.name,
            script = %self.script_path.display(),
            interpreter = %interpreter,
            "Executing custom script tool"
        );

        let input_json = serde_json::to_string(&input.arguments).unwrap_or_default();

        if !custom_tools_enabled() {
            return Ok(ToolOutput {
                content: "Custom tool execution is disabled. Set ELECTRO_ENABLE_CUSTOM_TOOLS=1 only after manual review of persisted scripts.".to_string(),
                is_error: true,
            });
        }

        // To run in the isolated shell container, the script must be inside the workspace.
        let ext = match self.meta.language.as_str() {
            "python" => "py",
            "node" => "js",
            _ => "sh",
        };
        let script_file_name = format!(".electro-custom-tool-{}.{}", self.meta.name, ext);
        let workspace_script_path = _ctx.workspace_path.join(&script_file_name);

        if let Err(e) = std::fs::copy(&self.script_path, &workspace_script_path) {
            return Ok(ToolOutput {
                content: format!("Failed to copy script to workspace: {}", e),
                is_error: true,
            });
        }

        // chmod +x on Unix just in case
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(
                &workspace_script_path,
                std::fs::Permissions::from_mode(0o755),
            );
        }

        let parsed = crate::shell::ParsedCommand {
            program: interpreter.to_string(),
            args: vec![script_file_name.clone()],
        };

        let policy = crate::shell::load_policy();
        let backend_res = crate::shell::resolve_backend(&policy).await;

        let result = match backend_res {
            Ok(crate::shell::ResolvedBackend::Host) => {
                crate::shell::run_host_command(&parsed, 30, _ctx, Some(input_json.into_bytes()))
                    .await
            }
            Ok(
                backend @ (crate::shell::ResolvedBackend::Docker
                | crate::shell::ResolvedBackend::Podman),
            ) => {
                crate::shell::run_container_command(
                    backend,
                    &policy,
                    &parsed,
                    30,
                    _ctx,
                    Some(input_json.into_bytes()),
                )
                .await
            }
            Err(e) => Ok(ToolOutput {
                content: e,
                is_error: true,
            }),
        };

        // Clean up the temporary script
        let _ = std::fs::remove_file(workspace_script_path);

        result
    }
}

// ── Custom Tool Registry ────────────────────────────────────────────────────

/// Manages custom script tools — loading, creating, change detection.
pub struct CustomToolRegistry {
    tools_dir: PathBuf,
    tools_changed: AtomicBool,
}

impl CustomToolRegistry {
    pub fn new() -> Self {
        let tools_dir = paths::custom_tools_dir();
        let _ = paths::ensure_electro_home();
        Self {
            tools_dir,
            tools_changed: AtomicBool::new(false),
        }
    }

    /// Load all custom tools from `~/.electro/custom-tools/`.
    pub fn load_tools(&self) -> Vec<Arc<dyn Tool>> {
        let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

        if !custom_tools_enabled() {
            info!("Custom tools disabled; skipping load");
            return tools;
        }

        if !self.tools_dir.exists() {
            return tools;
        }

        let entries = match std::fs::read_dir(&self.tools_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "Cannot read custom tools directory");
                return tools;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match ScriptToolAdapter::from_meta_file(&path) {
                    Ok(tool) => {
                        info!(tool = %tool.meta.name, "Loaded custom tool");
                        tools.push(Arc::new(tool));
                    }
                    Err(e) => {
                        warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to load custom tool"
                        );
                    }
                }
            }
        }

        if !tools.is_empty() {
            info!(count = tools.len(), "Custom tools loaded");
        }

        tools
    }

    /// Create a new custom tool: write script + metadata, mark as changed.
    pub fn create_tool(
        &self,
        name: &str,
        description: &str,
        language: &str,
        script_content: &str,
        parameters: serde_json::Value,
    ) -> Result<String, ElectroError> {
        if !custom_tools_enabled() {
            return Err(ElectroError::Tool(
                "Custom tools are disabled. Set ELECTRO_ENABLE_CUSTOM_TOOLS=1 only after manual review and only in a trusted environment.".to_string(),
            ));
        }

        // Validate name
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ElectroError::Tool(
                "Tool name must be non-empty and contain only [a-zA-Z0-9_-]".to_string(),
            ));
        }

        // Validate language
        let ext = match language {
            "bash" | "sh" => "sh",
            "python" | "py" => "py",
            "node" | "js" | "javascript" => "js",
            other => {
                return Err(ElectroError::Tool(format!(
                    "Unsupported language '{}'. Use: bash, python, or node.",
                    other
                )));
            }
        };

        // Normalize language name
        let lang = match ext {
            "py" => "python",
            "js" => "node",
            _ => "bash",
        };

        if script_content.trim().is_empty() {
            return Err(ElectroError::Tool(
                "Script content cannot be empty.".to_string(),
            ));
        }

        // Create directory
        std::fs::create_dir_all(&self.tools_dir).map_err(|e| {
            ElectroError::Tool(format!(
                "Cannot create custom tools directory {}: {}",
                self.tools_dir.display(),
                e
            ))
        })?;

        // Write script file
        let script_path = self.tools_dir.join(format!("{}.{}", name, ext));
        std::fs::write(&script_path, script_content).map_err(|e| {
            ElectroError::Tool(format!(
                "Cannot write script {}: {}",
                script_path.display(),
                e
            ))
        })?;

        // Make executable on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
        }

        // Write metadata
        let meta = ScriptToolMeta {
            name: name.to_string(),
            description: description.to_string(),
            language: lang.to_string(),
            parameters,
        };
        let meta_path = self.tools_dir.join(format!("{}.json", name));
        let meta_json = serde_json::to_string_pretty(&meta)
            .map_err(|e| ElectroError::Tool(format!("Cannot serialize tool metadata: {}", e)))?;
        std::fs::write(&meta_path, meta_json).map_err(|e| {
            ElectroError::Tool(format!(
                "Cannot write metadata {}: {}",
                meta_path.display(),
                e
            ))
        })?;

        // Signal tools changed
        self.tools_changed.store(true, Ordering::Relaxed);

        info!(tool = %name, language = %lang, "Custom tool created");

        Ok(format!(
            "Tool '{}' created successfully at {}.\n\
             Script: {}\n\
             The tool is now available — use it in your next action.",
            name,
            meta_path.display(),
            script_path.display()
        ))
    }

    /// List all custom tools (name + description).
    pub fn list_tools(&self) -> Vec<(String, String)> {
        let mut result = Vec::new();
        if !self.tools_dir.exists() {
            return result;
        }
        if let Ok(entries) = std::fs::read_dir(&self.tools_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(meta) = serde_json::from_str::<ScriptToolMeta>(&content) {
                            result.push((meta.name, meta.description));
                        }
                    }
                }
            }
        }
        result
    }

    /// Delete a custom tool by name.
    pub fn delete_tool(&self, name: &str) -> Result<String, ElectroError> {
        let meta_path = self.tools_dir.join(format!("{}.json", name));
        if !meta_path.exists() {
            return Err(ElectroError::Tool(format!(
                "Custom tool '{}' not found.",
                name
            )));
        }

        // Read metadata to find script extension
        if let Ok(content) = std::fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<ScriptToolMeta>(&content) {
                let ext = match meta.language.as_str() {
                    "python" => "py",
                    "node" => "js",
                    _ => "sh",
                };
                let script_path = self.tools_dir.join(format!("{}.{}", name, ext));
                let _ = std::fs::remove_file(script_path);
            }
        }

        let _ = std::fs::remove_file(&meta_path);
        self.tools_changed.store(true, Ordering::Relaxed);

        info!(tool = %name, "Custom tool deleted");
        Ok(format!("Custom tool '{}' deleted.", name))
    }

    /// Check and clear the tools_changed flag.
    pub fn take_tools_changed(&self) -> bool {
        self.tools_changed.swap(false, Ordering::Relaxed)
    }
}

impl Default for CustomToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_and_load_tool() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        let result = registry.create_tool(
            "hello",
            "Says hello",
            "bash",
            "#!/bin/bash\necho \"Hello from custom tool!\"",
            serde_json::json!({"type": "object", "properties": {}}),
        );
        assert!(result.is_ok());

        let tools = registry.load_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name(), "hello");
        assert!(registry.take_tools_changed());
    }

    #[test]
    fn invalid_tool_name() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        let result = registry.create_tool(
            "bad name!",
            "desc",
            "bash",
            "echo hi",
            serde_json::json!({}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn empty_script_rejected() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        let result = registry.create_tool("test", "desc", "bash", "   ", serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn delete_tool() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        registry
            .create_tool(
                "temp",
                "Temp tool",
                "bash",
                "echo temp",
                serde_json::json!({}),
            )
            .unwrap();
        assert_eq!(registry.load_tools().len(), 1);

        registry.delete_tool("temp").unwrap();
        assert_eq!(registry.load_tools().len(), 0);
    }

    #[test]
    fn delete_nonexistent() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        let result = registry.delete_tool("nope");
        assert!(result.is_err());
    }

    #[test]
    fn list_empty() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        let tools = registry.list_tools();
        assert!(tools.is_empty());
    }

    #[test]
    fn unsupported_language() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        let result =
            registry.create_tool("test", "desc", "ruby", "puts 'hi'", serde_json::json!({}));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn execute_bash_script() {
        let dir = tempdir().unwrap();
        let registry = CustomToolRegistry {
            tools_dir: dir.path().to_path_buf(),
            tools_changed: AtomicBool::new(false),
        };

        registry
            .create_tool(
                "greet",
                "Says hello",
                "bash",
                "#!/bin/bash\nread input\necho \"Hello from script!\"",
                serde_json::json!({"type": "object", "properties": {}}),
            )
            .unwrap();

        let tools = registry.load_tools();
        assert_eq!(tools.len(), 1);

        let ctx = ToolContext {
            workspace_path: std::path::PathBuf::from("/tmp"),
            session_id: "test".to_string(),
            chat_id: "test".to_string(),
        };
        let input = ToolInput {
            name: "greet".to_string(),
            arguments: serde_json::json!({}),
        };
        // Enable host shell and allow bash for the test to succeed without Docker.
        std::env::set_var("ELECTRO_SHELL_BACKEND", "host");
        std::env::set_var("ELECTRO_ENABLE_HOST_SHELL", "1");
        std::env::set_var("ELECTRO_SHELL_ALLOW_HOST_LAUNCHER", "1");

        let output = tools[0].execute(input, &ctx).await.unwrap();
        assert!(!output.is_error);
        assert!(output.content.contains("Hello from script!"));
    }
}
