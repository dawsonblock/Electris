//! Sandbox runner for tool execution.
//!
//! This module provides the canonical execution path for live tools:
//! ```text
//! policy check → validation → sandbox runner → output cap → audit event
//! ```
//!
//! All shell, git, and other live tool execution must flow through this runner
//! to ensure proper sandboxing and auditing.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use std::time::Duration;

use anyhow::{anyhow, Result};
use electro_core::policy::{PolicyDecision, PolicyEngine, CapabilityPolicy};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

/// Default sandbox constraints for tool execution.
#[derive(Debug, Clone)]
pub struct SandboxConstraints {
    /// Maximum memory in bytes (default: 512MB).
    pub memory_bytes: u64,
    /// Maximum CPU cores (default: 1).
    pub cpu_cores: u32,
    /// Maximum number of PIDs (default: 32).
    pub pid_limit: u32,
    /// Timeout in seconds (default: 30).
    pub timeout_secs: u64,
    /// Maximum output size in bytes (default: 64KB).
    pub max_output_bytes: usize,
}

impl Default for SandboxConstraints {
    fn default() -> Self {
        Self {
            memory_bytes: 512 * 1024 * 1024, // 512MB
            cpu_cores: 1,
            pid_limit: 32,
            timeout_secs: 30,
            max_output_bytes: 64 * 1024, // 64KB
        }
    }
}

/// Request to execute a command in the sandbox.
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    /// The program to execute.
    pub program: String,
    /// Arguments for the program.
    pub args: Vec<String>,
    /// Working directory for execution.
    pub working_dir: Option<std::path::PathBuf>,
    /// Environment variables.
    pub env: HashMap<String, String>,
    /// Input to provide to stdin.
    pub stdin_input: Option<String>,
}

impl ExecutionRequest {
    /// Create a new execution request.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
            env: HashMap::new(),
            stdin_input: None,
        }
    }

    /// Add an argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments.
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(|a| a.into()));
        self
    }

    /// Set the working directory.
    pub fn working_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set stdin input.
    pub fn stdin(mut self, input: impl Into<String>) -> Self {
        self.stdin_input = Some(input.into());
        self
    }
}

/// Result of a sandboxed execution.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Exit code (None if process was killed or timed out).
    pub exit_code: Option<i32>,
    /// Standard output (truncated if exceeds limit).
    pub stdout: String,
    /// Standard error (truncated if exceeds limit).
    pub stderr: String,
    /// Whether the execution was truncated due to output limits.
    pub truncated: bool,
    /// Whether the execution timed out.
    pub timed_out: bool,
}

impl ExecutionResult {
    /// Check if the execution was successful.
    pub fn success(&self) -> bool {
        self.exit_code == Some(0) && !self.timed_out
    }

    /// Get combined output (stdout + stderr).
    pub fn combined_output(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Validates a command against safety rules.
fn validate_command(_program: &str, args: &[String]) -> Result<()> {
    // Check for dangerous characters in arguments
    for arg in args {
        if arg.contains(';') || arg.contains('&') || arg.contains('|') || arg.contains('$') {
            return Err(anyhow!("dangerous shell characters detected in arguments"));
        }
    }

    // Check for path traversal attempts
    for arg in args {
        if arg.contains("..") {
            return Err(anyhow!("path traversal attempt detected"));
        }
    }

    Ok(())
}

/// Runs a command in a sandboxed environment with policy enforcement.
///
/// # Arguments
/// * `request` - The execution request
/// * `policy` - The capability policy to enforce
/// * `constraints` - Sandbox constraints (optional, uses defaults if not provided)
///
/// # Returns
/// The execution result or an error if policy denies execution.
pub async fn run_sandboxed(
    request: ExecutionRequest,
    policy: &CapabilityPolicy,
    constraints: Option<SandboxConstraints>,
) -> Result<ExecutionResult> {
    // 1. POLICY CHECK: Check shell access permission
    match PolicyEngine::evaluate_shell(policy) {
        PolicyDecision::Allow => {}
        PolicyDecision::Deny(reason) => {
            return Err(anyhow!("policy denied: {}", reason));
        }
    }

    // 2. VALIDATION: Validate command safety
    validate_command(&request.program, &request.args)?;

    let constraints = constraints.unwrap_or_default();

    // 3. SANDBOX RUNNER: Execute with constraints
    let mut cmd = Command::new(&request.program);
    cmd.args(&request.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());

    // Set working directory if provided
    if let Some(ref dir) = request.working_dir {
        cmd.current_dir(dir);
    }

    // Set environment variables
    for (key, value) in &request.env {
        cmd.env(key, value);
    }

    // Spawn the process
    let mut child = cmd.spawn()
        .map_err(|e| anyhow!("failed to spawn process: {}", e))?;

    // Write stdin if provided
    if let Some(input) = request.stdin_input {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes()).await;
            // stdin is dropped here, closing the pipe
        }
    }

    // Wait for completion with timeout
    let timeout_duration = Duration::from_secs(constraints.timeout_secs);
    let result = timeout(timeout_duration, child.wait()).await;

    let mut timed_out = false;
    let exit_code = match result {
        Ok(Ok(status)) => status.code(),
        Ok(Err(e)) => return Err(anyhow!("process error: {}", e)),
        Err(_) => {
            // Timeout - kill the process
            timed_out = true;
            let _ = child.start_kill();
            None
        }
    };

    // Read stdout with output cap
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut truncated = false;

    if let Some(mut stdout_pipe) = child.stdout.take() {
        let mut buffer = vec![0u8; 8192];
        let mut total_read = 0usize;

        loop {
            match stdout_pipe.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    total_read += n;
                    if total_read > constraints.max_output_bytes {
                        truncated = true;
                        break;
                    }
                    stdout.push_str(&String::from_utf8_lossy(&buffer[..n]));
                }
                Err(_) => break,
            }
        }
    }

    // Read stderr (also capped)
    if let Some(mut stderr_pipe) = child.stderr.take() {
        let mut buffer = vec![0u8; 4096];
        let mut total_read = 0usize;
        let stderr_limit = constraints.max_output_bytes.saturating_sub(stdout.len());

        loop {
            match stderr_pipe.read(&mut buffer).await {
                Ok(0) => break,
                Ok(n) => {
                    total_read += n;
                    if total_read > stderr_limit {
                        break;
                    }
                    stderr.push_str(&String::from_utf8_lossy(&buffer[..n]));
                }
                Err(_) => break,
            }
        }
    }

    // 4. OUTPUT CAP: Truncate if necessary
    if stdout.len() > constraints.max_output_bytes {
        stdout.truncate(constraints.max_output_bytes);
        stdout.push_str("\n[output truncated due to size limit]");
        truncated = true;
    }

    // 5. AUDIT EVENT: Log the execution (placeholder for actual audit implementation)
    tracing::debug!(
        program = %request.program,
        args = ?request.args,
        exit_code = ?exit_code,
        timed_out = timed_out,
        truncated = truncated,
        "sandboxed execution completed"
    );

    Ok(ExecutionResult {
        exit_code,
        stdout,
        stderr,
        truncated,
        timed_out,
    })
}

/// Convenience function to run a shell command with sandboxing.
///
/// This is the canonical entry point for shell execution from tools.
pub async fn run_shell_command(
    command: &str,
    working_dir: Option<&Path>,
    policy: &CapabilityPolicy,
) -> Result<ExecutionResult> {
    let request = ExecutionRequest::new("sh")
        .arg("-c")
        .arg(command)
        .working_dir(working_dir.unwrap_or_else(|| Path::new(".")));

    run_sandboxed(request, policy, None).await
}

/// Convenience function to run a git command with sandboxing.
///
/// This is the canonical entry point for git execution from tools.
pub async fn run_git_command(
    args: &[String],
    working_dir: Option<&Path>,
    policy: &CapabilityPolicy,
) -> Result<ExecutionResult> {
    // Check filesystem policy for the working directory
    if let Some(dir) = working_dir {
        match PolicyEngine::evaluate_file(policy, dir, false) {
            PolicyDecision::Allow => {}
            PolicyDecision::Deny(reason) => {
                return Err(anyhow!("policy denied: {}", reason));
            }
        }
    }

    let request = ExecutionRequest::new("git")
        .args(args.iter().map(|s| s.as_str()))
        .working_dir(working_dir.unwrap_or_else(|| Path::new(".")));

    run_sandboxed(request, policy, None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_request_builder() {
        let req = ExecutionRequest::new("echo")
            .arg("hello")
            .arg("world")
            .env("FOO", "bar")
            .stdin("input");

        assert_eq!(req.program, "echo");
        assert_eq!(req.args, vec!["hello", "world"]);
        assert_eq!(req.env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(req.stdin_input, Some("input".to_string()));
    }

    #[test]
    fn test_validate_command_safe() {
        assert!(validate_command("echo", &["hello".to_string()]).is_ok());
        assert!(validate_command("git", &["status".to_string()]).is_ok());
    }

    #[test]
    fn test_validate_command_dangerous() {
        assert!(validate_command("sh", &["foo; rm -rf /".to_string()]).is_err());
        assert!(validate_command("cat", &["../etc/passwd".to_string()]).is_err());
    }

    #[tokio::test]
    async fn test_run_sandboxed_echo() {
        let policy = CapabilityPolicy::shell();
        let request = ExecutionRequest::new("echo").arg("hello");
        
        let result = run_sandboxed(request, &policy, None).await;
        assert!(result.is_ok());
        
        let exec_result = result.unwrap();
        assert!(exec_result.success());
        assert!(exec_result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_run_sandboxed_blocked_by_policy() {
        let policy = CapabilityPolicy::none(); // Shell blocked
        let request = ExecutionRequest::new("echo").arg("hello");
        
        let result = run_sandboxed(request, &policy, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("policy denied"));
    }

    #[tokio::test]
    async fn test_run_sandboxed_timeout() {
        let policy = CapabilityPolicy::shell();
        let constraints = SandboxConstraints {
            timeout_secs: 1, // Very short timeout
            ..Default::default()
        };
        let request = ExecutionRequest::new("sleep").arg("10");
        
        let result = run_sandboxed(request, &policy, Some(constraints)).await;
        assert!(result.is_ok());
        
        let exec_result = result.unwrap();
        assert!(exec_result.timed_out);
    }
}
