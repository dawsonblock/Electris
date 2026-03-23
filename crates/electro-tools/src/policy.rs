use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub allow_shell: bool,
    pub allow_network: bool,
    pub allow_filesystem: bool,
    pub writable_roots: Vec<PathBuf>,
}

static POLICY_OVERRIDE: OnceLock<RwLock<Option<ToolPolicy>>> = OnceLock::new();

pub fn set_runtime_policy(policy: ToolPolicy) {
    let slot = POLICY_OVERRIDE.get_or_init(|| RwLock::new(None));
    if let Ok(mut guard) = slot.write() {
        *guard = Some(policy);
    }
}

fn get_runtime_policy_override() -> Option<ToolPolicy> {
    let slot = POLICY_OVERRIDE.get_or_init(|| RwLock::new(None));
    slot.read().ok().and_then(|guard| guard.clone())
}

impl ToolPolicy {
    pub fn for_workspace(workspace: PathBuf) -> Self {
        if let Some(mut override_policy) = get_runtime_policy_override() {
            if override_policy.writable_roots.is_empty() {
                override_policy.writable_roots.push(workspace);
            }
            return override_policy;
        }

        Self {
            allow_shell: true,
            allow_network: true,
            allow_filesystem: true,
            writable_roots: vec![workspace],
        }
    }
}

pub fn enforce(policy: &ToolPolicy, tool: &str) -> Result<()> {
    match tool {
        "shell" if !policy.allow_shell => Err(anyhow!("shell disabled")),
        "fetch" | "web_fetch" if !policy.allow_network => Err(anyhow!("network disabled")),
        "file_read" | "file_write" | "file_list" | "git" if !policy.allow_filesystem => {
            Err(anyhow!("filesystem disabled"))
        }
        _ => Ok(()),
    }
}

pub fn validate_path(path: &Path, roots: &[PathBuf]) -> Result<()> {
    if roots.is_empty() {
        return Err(anyhow!("no writable roots configured"));
    }

    if roots.iter().any(|root| path.starts_with(root)) {
        Ok(())
    } else {
        Err(anyhow!("path not allowed"))
    }
}
