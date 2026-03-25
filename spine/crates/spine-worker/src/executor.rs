//! Command executor - the only place where tools are called.
//!
//! This module is the execution boundary. It is the only code that
//! may invoke tool implementations from spine-tools.

use spine_core::{Command, Outcome};

use crate::policy::{ExecutionPolicy, PolicyChecker};
use crate::sandbox::{ResourceLimits, Sandbox};

/// Execute a command.
///
/// This is the **only** public function for command execution.
/// It applies policy checks, sandbox limits, and then invokes tools.
///
/// # Example
/// ```
/// use spine_core::Command;
/// use spine_worker::execute_command;
///
/// # async fn example() {
/// let cmd = Command::new("intent-1", "test", serde_json::json!({}));
/// let outcome = execute_command(cmd).await;
/// # }
/// ```
pub async fn execute_command(command: Command) -> Outcome {
    tracing::info!(
        intent_id = %command.intent_id,
        action = %command.action,
        "worker: executing command"
    );

    // Apply policy check
    let policy = ExecutionPolicy::standard();
    if let Err(e) = policy.check(&command.action) {
        tracing::warn!(
            intent_id = %command.intent_id,
            error = %e,
            "worker: policy check failed"
        );
        return Outcome::failure(e.to_string());
    }

    // Apply sandbox limits
    let sandbox = Sandbox::new(ResourceLimits::standard());
    if let Err(e) = sandbox.check_operation(100, 60) {
        tracing::warn!(
            intent_id = %command.intent_id,
            error = %e,
            "worker: sandbox check failed"
        );
        return Outcome::failure(e.to_string());
    }

    // Execute the tool
    let result = execute_tool(&command).await;

    tracing::info!(
        intent_id = %command.intent_id,
        success = result.success,
        "worker: command completed"
    );

    result
}

/// Execute a tool based on the command.
///
/// This is the internal routing logic that calls specific tools.
/// It dispatches to spine-tools for actual implementation.
async fn execute_tool(command: &Command) -> Outcome {
    let action = command.action.as_str();
    let args = &command.args;
    
    tracing::debug!(action = %action, "worker: executing tool");

    match action {
        // File system operations
        "read_file" | "fs_read" => {
            let path = args.get("path").and_then(|v| v.as_str());
            match path {
                Some(p) => match spine_tools::fs::read_file(p).await {
                    Ok(content) => Outcome::success(content),
                    Err(e) => Outcome::failure(e),
                },
                None => Outcome::failure("Missing 'path' argument"),
            }
        }
        "write_file" | "fs_write" => {
            let path = args.get("path").and_then(|v| v.as_str());
            let content = args.get("content").and_then(|v| v.as_str());
            match (path, content) {
                (Some(p), Some(c)) => match spine_tools::fs::write_file(p, c).await {
                    Ok(_) => Outcome::success("File written successfully"),
                    Err(e) => Outcome::failure(e),
                },
                _ => Outcome::failure("Missing 'path' or 'content' argument"),
            }
        }

        // Shell operations
        "shell" => {
            let cmd = args.get("command").and_then(|v| v.as_str());
            match cmd {
                Some(c) => match spine_tools::shell::execute(c).await {
                    Ok(output) => Outcome::success(output),
                    Err(e) => Outcome::failure(e),
                },
                None => Outcome::failure("Missing 'command' argument"),
            }
        }

        // Git operations
        "git" => {
            let subcommand = args.get("subcommand").and_then(|v| v.as_str());
            let repo_path = args.get("repo").and_then(|v| v.as_str()).unwrap_or(".");
            match subcommand {
                Some("status") => match spine_tools::git::status(repo_path).await {
                    Ok(output) => Outcome::success(output),
                    Err(e) => Outcome::failure(e),
                },
                Some(sc) => Outcome::failure(format!("Unknown git subcommand: {sc}")),
                None => Outcome::failure("Missing 'subcommand' argument"),
            }
        }

        // Placeholder/test actions
        "test" => {
            Outcome::success("Executed: test".to_string())
        }
        "analyze" => {
            Outcome::success("Executed: analyze".to_string())
        }

        // Unknown action
        _ => {
            tracing::warn!(action = %action, "worker: unknown action");
            Outcome::failure(format!("Unknown action: {action}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_test_command() {
        let cmd = Command::new("id", "test", serde_json::json!({}));
        let outcome = execute_command(cmd).await;
        assert!(outcome.success, "Got error: {:?}", outcome.error);
    }

    #[tokio::test]
    async fn execute_unknown_action_fails() {
        let cmd = Command::new("id", "unknown_action", serde_json::json!({}));
        let outcome = execute_command(cmd).await;
        assert!(!outcome.success);
        assert!(outcome.error.unwrap().to_lowercase().contains("unknown"));
    }

    #[tokio::test]
    async fn execute_read_file_requires_path() {
        let cmd = Command::new("id", "read_file", serde_json::json!({}));
        let outcome = execute_command(cmd).await;
        assert!(!outcome.success);
        assert!(outcome.error.unwrap().contains("Missing"));
    }

    #[tokio::test]
    async fn policy_blocks_restricted_action() {
        // Network is blocked by standard policy
        let cmd = Command::new("id", "network", serde_json::json!({}));
        let outcome = execute_command(cmd).await;
        assert!(!outcome.success);
        assert!(outcome.error.unwrap().contains("not allowed"));
    }
}
