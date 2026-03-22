//! File tool — read, write, and list files within the session workspace.

use async_trait::async_trait;
use electro_core::types::error::ElectroError;
use electro_core::{Tool, ToolContext, ToolInput, ToolOutput};
use electro_core::policy::{CapabilityPolicy, FileAccessPolicy};


/// Maximum file read size (32 KB — keeps tool output within token budget).
const MAX_READ_SIZE: usize = 32 * 1024;

#[derive(Default)]
pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file inside the session workspace. Absolute paths and home-directory expansion are blocked."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to read, relative to the workspace"
                }
            },
            "required": ["path"]
        })
    }

    fn declarations(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            file_access: vec![FileAccessPolicy::Read(".".into())],
            network_access: electro_core::net_policy::NetworkPolicy::Blocked,
            shell_access: electro_core::policy::ShellPolicy::Blocked,
            browser_access: electro_core::policy::BrowserPolicy::Blocked,
        }
    }

    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ElectroError> {
        let path_str = input
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ElectroError::Tool("Missing required parameter: path".into()))?;

        let user_path = std::path::Path::new(path_str);
        let path = match electro_core::path_policy::resolve_safe_path(&ctx.workspace_path, user_path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Path error: {e}"),
                    is_error: true,
                });
            }
        };

        match tokio::fs::read_to_string(&path).await {
            Ok(mut content) => {
                if content.len() > MAX_READ_SIZE {
                    let end = content
                        .char_indices()
                        .map(|(i, _)| i)
                        .take_while(|&i| i <= MAX_READ_SIZE)
                        .last()
                        .unwrap_or(0);
                    content.truncate(end);
                    content.push_str("\n... [file truncated]");
                }
                Ok(ToolOutput {
                    content,
                    is_error: false,
                })
            }
            Err(e) => Ok(ToolOutput {
                content: format!("Failed to read file '{}': {}", path_str, e),
                is_error: true,
            }),
        }
    }
}

#[derive(Default)]
pub struct FileWriteTool;

impl FileWriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file inside the session workspace. Creates parent directories automatically."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path to write, relative to the workspace"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn declarations(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            file_access: vec![FileAccessPolicy::ReadWrite(".".into())],
            network_access: electro_core::net_policy::NetworkPolicy::Blocked,
            shell_access: electro_core::policy::ShellPolicy::Blocked,
            browser_access: electro_core::policy::BrowserPolicy::Blocked,
        }
    }

    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ElectroError> {
        let path_str = input
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ElectroError::Tool("Missing required parameter: path".into()))?;

        let content = input
            .arguments
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ElectroError::Tool("Missing required parameter: content".into()))?;

        let user_path = std::path::Path::new(path_str);
        let path = match electro_core::path_policy::resolve_safe_path(&ctx.workspace_path, user_path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Path error: {e}"),
                    is_error: true,
                });
            }
        };

        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(ToolOutput {
                    content: format!("Failed to create directories for '{}': {}", path_str, e),
                    is_error: true,
                });
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(()) => Ok(ToolOutput {
                content: format!("Written {} bytes to '{}'", content.len(), path_str),
                is_error: false,
            }),
            Err(e) => Ok(ToolOutput {
                content: format!("Failed to write file '{}': {}", path_str, e),
                is_error: true,
            }),
        }
    }
}

#[derive(Default)]
pub struct FileListTool;

impl FileListTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories at a path inside the session workspace."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list, relative to the workspace. Defaults to workspace root."
                }
            },
            "required": []
        })
    }

    fn declarations(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            file_access: vec![FileAccessPolicy::Read(".".into())],
            network_access: electro_core::net_policy::NetworkPolicy::Blocked,
            shell_access: electro_core::policy::ShellPolicy::Blocked,
            browser_access: electro_core::policy::BrowserPolicy::Blocked,
        }
    }

    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ElectroError> {
        let path_str = input
            .arguments
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");

        let user_path = std::path::Path::new(path_str);
        let path = match electro_core::path_policy::resolve_safe_path(&ctx.workspace_path, user_path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolOutput {
                    content: format!("Path error: {e}"),
                    is_error: true,
                });
            }
        };

        match tokio::fs::read_dir(&path).await {
            Ok(mut entries) => {
                let mut items = Vec::new();
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
                    if is_dir {
                        items.push(format!("{}/", name));
                    } else {
                        items.push(name);
                    }
                }
                items.sort();
                if items.is_empty() {
                    Ok(ToolOutput {
                        content: format!("Directory '{}' is empty", path_str),
                        is_error: false,
                    })
                } else {
                    Ok(ToolOutput {
                        content: items.join("\n"),
                        is_error: false,
                    })
                }
            }
            Err(e) => Ok(ToolOutput {
                content: format!("Failed to list directory '{}': {}", path_str, e),
                is_error: true,
            }),
        }
    }
}
