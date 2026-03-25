//! Shell execution tools.
//!
//! All functions are `pub(crate)` - only accessible within this crate
//! and to the spine-worker crate.

use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

/// Maximum output size (1 MB)
const MAX_OUTPUT_SIZE: usize = 1024 * 1024;

/// Default timeout (60 seconds)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Execute a shell command with safety checks.
///
/// # Arguments
/// * `command` - The shell command to execute
///
/// # Returns
/// * `Ok(String)` - Command stdout
/// * `Err(String)` - Error message (stderr or timeout)
///
/// # Safety
/// Commands are validated against a blocklist before execution.
/// Output is capped at 1 MB.
/// Execution is timeout-protected.
pub async fn execute(command: &str) -> Result<String, String> {
    tracing::debug!(command = %command, "shell: executing command");

    // Validate command
    validate_command(command)?;

    // Execute with timeout
    let result = timeout(DEFAULT_TIMEOUT, execute_inner(command)).await;

    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(format!("Command timed out after {} seconds", DEFAULT_TIMEOUT.as_secs())),
    }
}

/// Inner execution without timeout.
async fn execute_inner(command: &str) -> Result<String, String> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {e}"))?;

    if output.status.success() {
        let stdout = output.stdout;
        
        // Cap output size
        if stdout.len() > MAX_OUTPUT_SIZE {
            return Err(format!(
                "Output exceeded maximum size of {MAX_OUTPUT_SIZE} bytes"
            ));
        }

        String::from_utf8(stdout)
            .map_err(|e| format!("Invalid UTF-8 in output: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Command failed: {stderr}"))
    }
}

/// Validate a command against the blocklist.
fn validate_command(command: &str) -> Result<(), String> {
    let command_lower = command.to_lowercase();

    // Block dangerous patterns
    let blocked_patterns = [
        "rm -rf /",
        "rm -rf /*",
        ":(){ :|:& };:", // Fork bomb
        "> /dev/sda",
        "dd if=/dev/zero of=/dev/sda",
        "mkfs.ext4 /dev/sda",
        "mv / /dev/null",
        "chmod -R 777 /",
    ];

    for pattern in &blocked_patterns {
        if command_lower.contains(pattern) {
            return Err(format!("Blocked dangerous command pattern: {pattern}"));
        }
    }

    Ok(())
}

/// Execute a command in a specific directory.
pub async fn execute_in_dir(
    command: &str,
    dir: impl AsRef<std::path::Path>,
) -> Result<String, String> {
    tracing::debug!(
        command = %command,
        dir = %dir.as_ref().display(),
        "shell: executing command in directory"
    );

    validate_command(command)?;

    let result = timeout(DEFAULT_TIMEOUT, async {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| format!("Failed to execute: {e}"))?;

        if output.status.success() {
            String::from_utf8(output.stdout)
                .map_err(|e| format!("Invalid UTF-8: {e}"))
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Command failed: {stderr}"))
        }
    })
    .await;

    match result {
        Ok(r) => r,
        Err(_) => Err(format!("Command timed out after {} seconds", DEFAULT_TIMEOUT.as_secs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn execute_echo() {
        let result = execute("echo hello world").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello world"));
    }

    #[tokio::test]
    async fn execute_fails_on_invalid() {
        let result = execute("not_a_real_command_12345").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn validate_blocks_dangerous() {
        assert!(validate_command("rm -rf /").is_err());
        assert!(validate_command(":(){ :|:& };:").is_err());
    }

    #[tokio::test]
    async fn validate_allows_safe() {
        assert!(validate_command("echo hello").is_ok());
        assert!(validate_command("ls -la").is_ok());
    }

    #[tokio::test]
    async fn execute_in_dir_works() {
        let result = execute_in_dir("pwd", "/tmp").await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("/tmp"));
    }
}
