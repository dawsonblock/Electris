use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// The unified capability policy schema that defines a tool's permissions.
/// Every dangerous action must map to one of these categories.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CapabilityPolicy {
    /// Permitted file access modes (read, write) and their restricted scopes
    pub file_access: Vec<FileAccessPolicy>,
    /// Permitted network egress class
    pub network_access: crate::net_policy::NetworkPolicy,
    /// Permitted shell execution limits
    pub shell_access: ShellPolicy,
    /// Automated browser interaction permissions
    pub browser_access: BrowserPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FileAccessPolicy {
    /// Can read within the specified directory
    Read(String),
    /// Can write within the specified directory
    Write(String),
    /// Can read and write within the specified directory
    ReadWrite(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ShellPolicy {
    #[default]
    Blocked,
    /// Unrestricted shell execution within the workspace
    Allowed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum BrowserPolicy {
    #[default]
    Blocked,
    /// Can automate an isolated browser
    Allowed {
        /// If true, the tool may evaluate arbitrary JavaScript against the page content
        eval_js: bool,
        /// If true, the tool may persist and restore browser session cookies
        session_persistence: bool,
    },
}

/// Evaluation context when enforcing a policy
pub struct PolicyContext {
    /// The workspace root that all file paths are clamped to
    pub workspace: PathBuf,
}

/// The result of an authoritative policy decision
#[derive(Debug, Clone)]
pub enum PolicyDecision {
    Allow,
    Deny(DenialReason),
}

/// Taxonomy of reasons for denying a tool execution request
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum DenialReason {
    #[error("File path escapes the permitted workspace / sandbox root")]
    PathEscape,
    #[error("File operation type (read/write) was not declared in policy")]
    UndeclaredFileOp,
    #[error("Network domain '{0}' was not explicitly permitted")]
    UndeclaredNetworkTarget(String),
    #[error("Network target resolves to a private/internal IP address or loopback")]
    InternalNetworkBlocked,
    #[error("Tool attempted to gain shell execution without declaring it")]
    UndeclaredShellExec,
    #[error("Tool attempted to use the browser without declaring it")]
    UndeclaredBrowser,
    #[error("Tool execution command contains a blocked pattern or metacharacter")]
    DangerousShellPattern,
}

impl CapabilityPolicy {
    /// Create a fully blocked / empty capability policy
    pub fn none() -> Self {
        Self::default()
    }
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn evaluate_shell(policy: &CapabilityPolicy) -> PolicyDecision {
        match policy.shell_access {
            ShellPolicy::Allowed => PolicyDecision::Allow,
            ShellPolicy::Blocked => PolicyDecision::Deny(DenialReason::UndeclaredShellExec),
        }
    }

    pub fn evaluate_browser(policy: &CapabilityPolicy) -> PolicyDecision {
        match policy.browser_access {
            BrowserPolicy::Allowed { .. } => PolicyDecision::Allow,
            BrowserPolicy::Blocked => PolicyDecision::Deny(DenialReason::UndeclaredBrowser),
        }
    }
}
