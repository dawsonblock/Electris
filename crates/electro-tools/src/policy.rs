//! Tool policy re-exported from electro-core for backward compatibility.
//! 
//! Use `electro_core::policy` for new code.

pub use electro_core::policy::{ToolPolicy, enforce, set_runtime_policy, validate_path};

use std::path::PathBuf;

/// Create a default policy for a workspace.
/// 
/// Convenience wrapper that calls `ToolPolicy::for_workspace`.
pub fn policy_for_workspace(workspace: PathBuf) -> ToolPolicy {
    ToolPolicy::for_workspace(workspace)
}
