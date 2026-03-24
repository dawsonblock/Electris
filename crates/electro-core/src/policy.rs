use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

/// Policy for tool execution, controlling what capabilities are allowed.
#[derive(Debug, Clone)]
pub struct ToolPolicy {
    pub allow_shell: bool,
    pub allow_network: bool,
    pub allow_filesystem: bool,
    pub writable_roots: Vec<PathBuf>,
}

static POLICY_OVERRIDE: OnceLock<RwLock<Option<ToolPolicy>>> = OnceLock::new();

/// Set a global runtime policy override.
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
    /// Create a default policy for a workspace.
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

/// Enforce policy against a tool name.
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

/// Validate that a path is within allowed roots.
/// 
/// This function normalizes the path to prevent directory traversal attacks.
/// Paths containing parent directory components ("..") are rejected.
pub fn validate_path(path: &Path, roots: &[PathBuf]) -> Result<()> {
    if roots.is_empty() {
        return Err(anyhow!("no writable roots configured"));
    }

    // Check for path traversal attempts - reject paths with ".." components
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(anyhow!("path traversal not allowed"));
    }

    if roots.iter().any(|root| path.starts_with(root)) {
        Ok(())
    } else {
        Err(anyhow!("path not allowed"))
    }
}

// ============================================================================
// Extended Policy Types for Tool Sandboxing
// ============================================================================

/// File access permission for a specific path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileAccessPolicy {
    /// Read-only access to the path.
    Read(PathBuf),
    /// Write-only access to the path.
    Write(PathBuf),
    /// Read and write access to the path.
    ReadWrite(PathBuf),
}

impl FileAccessPolicy {
    /// Get the path from the policy.
    pub fn path(&self) -> &Path {
        match self {
            FileAccessPolicy::Read(p) => p,
            FileAccessPolicy::Write(p) => p,
            FileAccessPolicy::ReadWrite(p) => p,
        }
    }

    /// Check if read access is allowed.
    pub fn allows_read(&self) -> bool {
        matches!(self, FileAccessPolicy::Read(_) | FileAccessPolicy::ReadWrite(_))
    }

    /// Check if write access is allowed.
    pub fn allows_write(&self) -> bool {
        matches!(self, FileAccessPolicy::Write(_) | FileAccessPolicy::ReadWrite(_))
    }
}

/// Shell access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShellPolicy {
    Blocked,
    Allowed,
}

/// Browser access policy with detailed controls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BrowserPolicy {
    Blocked,
    Allowed {
        /// Whether to run in headless mode.
        headless: bool,
        /// Whether JavaScript evaluation is allowed.
        eval_js: bool,
        /// Whether session persistence is allowed.
        session_persistence: bool,
    },
}

/// Network access policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkAccessPolicy {
    Blocked,
    Allowed,
    Restricted { allowlist: Vec<String> },
}

/// Comprehensive capability policy for tool declarations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityPolicy {
    pub file_access: Vec<FileAccessPolicy>,
    pub network_access: crate::net_policy::NetworkPolicy,
    pub shell_access: ShellPolicy,
    pub browser_access: BrowserPolicy,
}

impl CapabilityPolicy {
    /// Create a policy with all capabilities blocked.
    pub fn none() -> Self {
        Self {
            file_access: vec![],
            network_access: crate::net_policy::NetworkPolicy::Blocked,
            shell_access: ShellPolicy::Blocked,
            browser_access: BrowserPolicy::Blocked,
        }
    }

    /// Create a policy with only shell access.
    pub fn shell() -> Self {
        Self {
            file_access: vec![],
            network_access: crate::net_policy::NetworkPolicy::Blocked,
            shell_access: ShellPolicy::Allowed,
            browser_access: BrowserPolicy::Blocked,
        }
    }

    /// Create a policy with only network access.
    pub fn network() -> Self {
        Self {
            file_access: vec![],
            network_access: crate::net_policy::NetworkPolicy::Unrestricted,
            shell_access: ShellPolicy::Blocked,
            browser_access: BrowserPolicy::Blocked,
        }
    }

    /// Create a policy with only filesystem access.
    pub fn filesystem(roots: Vec<PathBuf>) -> Self {
        Self {
            file_access: roots.into_iter().map(FileAccessPolicy::ReadWrite).collect(),
            network_access: crate::net_policy::NetworkPolicy::Blocked,
            shell_access: ShellPolicy::Blocked,
            browser_access: BrowserPolicy::Blocked,
        }
    }

    /// Create a policy with all capabilities allowed.
    pub fn all() -> Self {
        Self {
            file_access: vec![FileAccessPolicy::ReadWrite(PathBuf::from("/"))],
            network_access: crate::net_policy::NetworkPolicy::Unrestricted,
            shell_access: ShellPolicy::Allowed,
            browser_access: BrowserPolicy::Allowed { 
                headless: true,
                eval_js: true,
                session_persistence: true,
            },
        }
    }
}

// ============================================================================
// Policy Engine and Decision Types
// ============================================================================

/// Decision outcome from policy evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Deny(String),
}

/// Policy engine for evaluating tool capability requests.
pub struct PolicyEngine;

impl PolicyEngine {
    /// Evaluate whether browser access is allowed.
    pub fn evaluate_browser(policy: &CapabilityPolicy) -> PolicyDecision {
        match policy.browser_access {
            BrowserPolicy::Allowed { .. } => PolicyDecision::Allow,
            BrowserPolicy::Blocked => PolicyDecision::Deny("browser access blocked by policy".into()),
        }
    }

    /// Evaluate whether shell access is allowed.
    pub fn evaluate_shell(policy: &CapabilityPolicy) -> PolicyDecision {
        match policy.shell_access {
            ShellPolicy::Allowed => PolicyDecision::Allow,
            ShellPolicy::Blocked => PolicyDecision::Deny("shell access blocked by policy".into()),
        }
    }

    /// Evaluate whether network access is allowed.
    pub fn evaluate_network(policy: &CapabilityPolicy) -> PolicyDecision {
        match policy.network_access {
            crate::net_policy::NetworkPolicy::Unrestricted => PolicyDecision::Allow,
            crate::net_policy::NetworkPolicy::PublicWeb { .. } => PolicyDecision::Allow,
            crate::net_policy::NetworkPolicy::Blocked => PolicyDecision::Deny("network access blocked by policy".into()),
        }
    }

    /// Evaluate whether file access is allowed for the given path.
    pub fn evaluate_file(policy: &CapabilityPolicy, path: &Path, write: bool) -> PolicyDecision {
        if policy.file_access.is_empty() {
            return PolicyDecision::Deny("no file access granted".into());
        }

        for access in &policy.file_access {
            if path.starts_with(access.path()) {
                if write && !access.allows_write() {
                    return PolicyDecision::Deny("write access not granted for path".into());
                }
                if !write && !access.allows_read() {
                    return PolicyDecision::Deny("read access not granted for path".into());
                }
                return PolicyDecision::Allow;
            }
        }

        PolicyDecision::Deny("path not in allowed file access list".into())
    }
}

/// Reason for denying a capability request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DenialReason {
    PolicyViolation,
    QuotaExceeded,
    RateLimited,
    UnsafeArguments,
    InternalError,
    PathEscape,
    UndeclaredFileOp,
}

impl std::fmt::Display for DenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DenialReason::PolicyViolation => write!(f, "policy violation"),
            DenialReason::QuotaExceeded => write!(f, "quota exceeded"),
            DenialReason::RateLimited => write!(f, "rate limited"),
            DenialReason::UnsafeArguments => write!(f, "unsafe arguments"),
            DenialReason::InternalError => write!(f, "internal error"),
            DenialReason::PathEscape => write!(f, "path escape attempt"),
            DenialReason::UndeclaredFileOp => write!(f, "undeclared file operation"),
        }
    }
}
