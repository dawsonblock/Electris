//! Execution policy enforcement.
//!
//! Policies control what tools are allowed to do.
//! They are checked before any tool execution.

/// Policy for tool execution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutionPolicy {
    /// Allow file system read operations
    pub allow_fs_read: bool,
    /// Allow file system write operations
    pub allow_fs_write: bool,
    /// Allow network access
    pub allow_network: bool,
    /// Allow shell execution
    pub allow_shell: bool,
    /// Allow git operations
    pub allow_git: bool,
    /// Maximum execution time in seconds
    pub timeout_secs: u64,
    /// Maximum memory in MB
    pub max_memory_mb: u64,
}

impl ExecutionPolicy {
    /// Create a permissive policy (for development).
    pub fn permissive() -> Self {
        Self {
            allow_fs_read: true,
            allow_fs_write: true,
            allow_network: true,
            allow_shell: true,
            allow_git: true,
            timeout_secs: 300,
            max_memory_mb: 1024,
        }
    }

    /// Create a restrictive policy (for production).
    pub fn restrictive() -> Self {
        Self {
            allow_fs_read: false,
            allow_fs_write: false,
            allow_network: false,
            allow_shell: false,
            allow_git: false,
            timeout_secs: 30,
            max_memory_mb: 128,
        }
    }

    /// Create a standard policy (reasonable defaults).
    pub fn standard() -> Self {
        Self {
            allow_fs_read: true,
            allow_fs_write: true,
            allow_network: false,
            allow_shell: true,
            allow_git: true,
            timeout_secs: 60,
            max_memory_mb: 512,
        }
    }

    /// Check if an action is allowed under this policy.
    pub fn check_action(&self, action: &str) -> Result<(), PolicyError> {
        match action {
            "read_file" | "fs_read" => {
                if self.allow_fs_read {
                    Ok(())
                } else {
                    Err(PolicyError::FsReadNotAllowed)
                }
            }
            "write_file" | "fs_write" => {
                if self.allow_fs_write {
                    Ok(())
                } else {
                    Err(PolicyError::FsWriteNotAllowed)
                }
            }
            "shell" | "execute" => {
                if self.allow_shell {
                    Ok(())
                } else {
                    Err(PolicyError::ShellNotAllowed)
                }
            }
            "git" => {
                if self.allow_git {
                    Ok(())
                } else {
                    Err(PolicyError::GitNotAllowed)
                }
            }
            "network" | "http" => {
                if self.allow_network {
                    Ok(())
                } else {
                    Err(PolicyError::NetworkNotAllowed)
                }
            }
            // Test/placeholder actions - always allowed
            "test" | "analyze" | "echo" => Ok(()),
            _ => Err(PolicyError::UnknownAction(action.to_string())),
        }
    }
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self::standard()
    }
}

/// Policy violation errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PolicyError {
    #[error("file system read operations not allowed")]
    FsReadNotAllowed,
    #[error("file system write operations not allowed")]
    FsWriteNotAllowed,
    #[error("shell execution not allowed")]
    ShellNotAllowed,
    #[error("git operations not allowed")]
    GitNotAllowed,
    #[error("network access not allowed")]
    NetworkNotAllowed,
    #[error("unknown action: {0}")]
    UnknownAction(String),
}

/// Trait for policy checking.
pub trait PolicyChecker {
    fn check(&self, action: &str) -> Result<(), PolicyError>;
}

impl PolicyChecker for ExecutionPolicy {
    fn check(&self, action: &str) -> Result<(), PolicyError> {
        self.check_action(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissive_allows_all() {
        let policy = ExecutionPolicy::permissive();
        assert!(policy.check_action("read_file").is_ok());
        assert!(policy.check_action("write_file").is_ok());
        assert!(policy.check_action("shell").is_ok());
        assert!(policy.check_action("git").is_ok());
    }

    #[test]
    fn restrictive_blocks_all() {
        let policy = ExecutionPolicy::restrictive();
        assert!(policy.check_action("read_file").is_err());
        assert!(policy.check_action("write_file").is_err());
        assert!(policy.check_action("shell").is_err());
    }

    #[test]
    fn standard_allows_fs_and_shell() {
        let policy = ExecutionPolicy::standard();
        assert!(policy.check_action("read_file").is_ok());
        assert!(policy.check_action("shell").is_ok());
    }

    #[test]
    fn standard_blocks_network() {
        let policy = ExecutionPolicy::standard();
        assert!(policy.check_action("network").is_err());
    }

    #[test]
    fn unknown_action_fails() {
        let policy = ExecutionPolicy::permissive();
        assert!(policy.check_action("unknown").is_err());
    }
}
