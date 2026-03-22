//! Shell tool — executes commands through an isolated runner when possible.
//!
//! Hardened default:
//! - `auto` backend tries `docker` and then `podman`
//! - container runner mounts only the workspace and denies network by default
//! - direct host execution is available only when the operator explicitly sets
//!   both `ELECTRO_SHELL_BACKEND=host` and `ELECTRO_ENABLE_HOST_SHELL=1`

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use electro_core::types::error::ElectroError;
use electro_core::{Tool, ToolContext, ToolInput, ToolOutput};
use electro_core::policy::CapabilityPolicy;


/// Default command timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum output size returned to the model (32 KB).
const MAX_OUTPUT_SIZE: usize = 32 * 1024;

const DEFAULT_CONTAINER_IMAGE: &str = "electro-shell-runner:local";
const DEFAULT_MEMORY_MB: u64 = 256;
const DEFAULT_PIDS_LIMIT: u64 = 128;
const DEFAULT_CPU_LIMIT: f64 = 1.0;
const DEFAULT_TMPFS_MB: u64 = 64;

const HOST_BLOCKED_PROGRAMS: &[&str] = &[
    "sh",
    "bash",
    "dash",
    "zsh",
    "fish",
    "ash",
    "busybox",
    "env",
    "sudo",
    "su",
    "doas",
];

const BLOCKED_META_SEQUENCES: &[&str] = &[
    "&&",
    "||",
    ";",
    "|",
    ">",
    "<",
    "`",
    "$(",
    "\n",
    "\r",
];

#[derive(Default)]
pub struct ShellTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellBackend {
    Auto,
    Docker,
    Podman,
    Host,
}

#[derive(Debug, Clone)]
pub struct ShellRunnerPolicy {
    pub backend: ShellBackend,
    pub container_image: String,
    pub allow_network: bool,
    pub memory_mb: u64,
    pub pids_limit: u64,
    pub cpu_limit: f64,
    pub tmpfs_mb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBackend {
    Docker,
    Podman,
    Host,
}

#[derive(Debug, Clone)]
pub struct ParsedCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl ParsedCommand {
    pub fn new(program: String, args: Vec<String>) -> Self {
        Self { program, args }
    }

    pub fn argv(&self) -> Vec<&str> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(self.program.as_str());
        argv.extend(self.args.iter().map(String::as_str));
        argv
    }
}

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

fn host_shell_enabled() -> bool {
    matches!(
        std::env::var("ELECTRO_ENABLE_HOST_SHELL")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn env_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_backend(value: &str) -> ShellBackend {
    match value.trim().to_ascii_lowercase().as_str() {
        "docker" => ShellBackend::Docker,
        "podman" => ShellBackend::Podman,
        "host" => ShellBackend::Host,
        _ => ShellBackend::Auto,
    }
}

fn parse_u64_env(name: &str, default: u64, min: u64, max: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(|v| v.clamp(min, max))
        .unwrap_or(default)
}

fn parse_f64_env(name: &str, default: f64, min: f64, max: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(|v| v.clamp(min, max))
        .unwrap_or(default)
}

pub fn load_policy() -> ShellRunnerPolicy {
    ShellRunnerPolicy {
        backend: parse_backend(
            &std::env::var("ELECTRO_SHELL_BACKEND").unwrap_or_else(|_| "auto".to_string()),
        ),
        container_image: std::env::var("ELECTRO_SHELL_CONTAINER_IMAGE")
            .unwrap_or_else(|_| DEFAULT_CONTAINER_IMAGE.to_string())
            .trim()
            .to_string(),
        allow_network: env_truthy("ELECTRO_SHELL_ALLOW_NETWORK"),
        memory_mb: parse_u64_env("ELECTRO_SHELL_MEMORY_MB", DEFAULT_MEMORY_MB, 64, 8192),
        pids_limit: parse_u64_env("ELECTRO_SHELL_PIDS_LIMIT", DEFAULT_PIDS_LIMIT, 16, 4096),
        cpu_limit: parse_f64_env("ELECTRO_SHELL_CPU_LIMIT", DEFAULT_CPU_LIMIT, 0.25, 8.0),
        tmpfs_mb: parse_u64_env("ELECTRO_SHELL_TMPFS_MB", DEFAULT_TMPFS_MB, 16, 2048),
    }
}

fn command_contains_blocked_meta(command: &str) -> Option<&'static str> {
    BLOCKED_META_SEQUENCES
        .iter()
        .copied()
        .find(|token| command.contains(token))
}

pub fn parse_command_line(command: &str) -> Result<ParsedCommand, String> {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mode {
        Normal,
        SingleQuoted,
        DoubleQuoted,
    }

    let mut args = Vec::with_capacity(8);
    let mut current = String::new();
    let mut mode = Mode::Normal;
    let mut escaped = false;

    for ch in command.chars() {
        if ch == '\0' {
            return Err("NUL bytes are not allowed in shell commands.".to_string());
        }

        match mode {
            Mode::Normal => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                    continue;
                }

                match ch {
                    '\\' => escaped = true,
                    '\'' => mode = Mode::SingleQuoted,
                    '"' => mode = Mode::DoubleQuoted,
                    c if c.is_whitespace() => {
                        if !current.is_empty() {
                            args.push(std::mem::take(&mut current));
                        }
                    }
                    _ => current.push(ch),
                }
            }
            Mode::SingleQuoted => {
                if ch == '\'' {
                    mode = Mode::Normal;
                } else {
                    current.push(ch);
                }
            }
            Mode::DoubleQuoted => {
                if escaped {
                    current.push(ch);
                    escaped = false;
                    continue;
                }

                match ch {
                    '\\' => escaped = true,
                    '"' => mode = Mode::Normal,
                    _ => current.push(ch),
                }
            }
        }
    }

    if escaped {
        return Err("Trailing escape is not allowed in shell commands.".to_string());
    }
    if mode != Mode::Normal {
        return Err("Unterminated quote in shell command.".to_string());
    }
    if !current.is_empty() {
        args.push(current);
    }
    if args.is_empty() {
        return Err("Command cannot be empty.".to_string());
    }

    let mut iter = args.into_iter();
    let program = iter
        .next()
        .ok_or_else(|| "Command cannot be empty.".to_string())?;
    Ok(ParsedCommand::new(program, iter.collect()))
}

async fn command_available(program: &str) -> bool {
    tokio::process::Command::new(program)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|status| status.success())
        .unwrap_or(false)
}

pub async fn resolve_backend(policy: &ShellRunnerPolicy) -> Result<ResolvedBackend, String> {
    match policy.backend {
        ShellBackend::Docker => {
            if command_available("docker").await {
                Ok(ResolvedBackend::Docker)
            } else {
                Err("ELECTRO_SHELL_BACKEND=docker but the docker CLI is not available on PATH.".to_string())
            }
        }
        ShellBackend::Podman => {
            if command_available("podman").await {
                Ok(ResolvedBackend::Podman)
            } else {
                Err("ELECTRO_SHELL_BACKEND=podman but the podman CLI is not available on PATH.".to_string())
            }
        }
        ShellBackend::Host => {
            if host_shell_enabled() {
                Ok(ResolvedBackend::Host)
            } else {
                Err("Direct host execution requires both ELECTRO_SHELL_BACKEND=host and ELECTRO_ENABLE_HOST_SHELL=1.".to_string())
            }
        }
        ShellBackend::Auto => {
            if command_available("docker").await {
                Ok(ResolvedBackend::Docker)
            } else if command_available("podman").await {
                Ok(ResolvedBackend::Podman)
            } else {
                Err("No isolated shell runner is available. Install docker or podman, or explicitly opt into direct host execution with ELECTRO_SHELL_BACKEND=host and ELECTRO_ENABLE_HOST_SHELL=1.".to_string())
            }
        }
    }
}

pub fn container_engine_name(backend: ResolvedBackend) -> &'static str {
    match backend {
        ResolvedBackend::Docker => "docker",
        ResolvedBackend::Podman => "podman",
        ResolvedBackend::Host => "host",
    }
}

fn canonical_workspace(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn program_basename(program: &str) -> String {
    Path::new(program)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase()
}

fn validate_host_program(parsed: &ParsedCommand) -> Result<(), ElectroError> {
    let base = program_basename(&parsed.program);
    if HOST_BLOCKED_PROGRAMS.contains(&base.as_str()) {
        return Err(ElectroError::Tool(format!(
            "Direct host execution blocks launcher '{}' by default. Use the isolated container runner instead.",
            base
        )));
    }
    Ok(())
}

fn passthrough_env_names() -> Vec<String> {
    let mut names = BTreeSet::new();
    for raw in std::env::var("ELECTRO_SHELL_PASSTHROUGH_ENV")
        .unwrap_or_default()
        .split(',')
    {
        let name = raw.trim();
        if name.is_empty() {
            continue;
        }
        if name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        {
            names.insert(name.to_string());
        }
    }
    names.into_iter().collect()
}

fn prepare_host_runtime_dirs(
    ctx: &ToolContext,
) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf), ElectroError> {
    let home = ctx.workspace_path.join(".electro-host-home");
    let cache = ctx.workspace_path.join(".electro-host-cache");
    let config = ctx.workspace_path.join(".electro-host-config");
    let state = ctx.workspace_path.join(".electro-host-state");
    let tmp = ctx.workspace_path.join(".electro-host-tmp");

    for dir in [&home, &cache, &config, &state, &tmp] {
        std::fs::create_dir_all(dir).map_err(|e| {
            ElectroError::Tool(format!(
                "Failed to prepare host shell runtime directory '{}': {}",
                dir.display(),
                e
            ))
        })?;
    }

    Ok((home, cache, config, state, tmp))
}

fn build_container_args(
    backend: ResolvedBackend,
    policy: &ShellRunnerPolicy,
    workspace_path: &Path,
    parsed: &ParsedCommand,
    is_interactive: bool,
) -> Vec<String> {
    let workspace_mount = canonical_workspace(workspace_path);
    let network_mode = if policy.allow_network { "bridge" } else { "none" };
    let cpu_limit = format!("{:.2}", policy.cpu_limit);
    let mount_arg = format!(
        "type=bind,src={},dst=/workspace,rw",
        workspace_mount.to_string_lossy()
    );
    let tmpfs_arg = format!(
        "/tmp:rw,noexec,nosuid,nodev,size={}m",
        policy.tmpfs_mb
    );

    let mut args = vec![
        "run".to_string(),
        "--rm".to_string(),
        if is_interactive { "-i".to_string() } else { "--pull".to_string() }, // replacing --pull with -i if stdin enabled for simplicity of injecting interactive. wait, docker run -i --rm --pull never works.
    ];
    if is_interactive {
        args.push("--pull".to_string());
    }
    args.push("never".to_string());
    args.push("--network".to_string());
    args.push(network_mode.to_string());
    args.push("--cap-drop".to_string());
    args.push("ALL".to_string());
    args.push("--security-opt".to_string());
    args.push("no-new-privileges".to_string());
    args.push("--pids-limit".to_string());
    args.push(policy.pids_limit.to_string());
    args.push("--memory".to_string());
    args.push(format!("{}m", policy.memory_mb));
    args.push("--cpus".to_string());
    args.push(cpu_limit);
    args.push("--read-only".to_string());
    args.push("--tmpfs".to_string());
    args.push(tmpfs_arg);
    args.push("--workdir".to_string());
    args.push("/workspace".to_string());
    args.push("--mount".to_string());
    args.push(mount_arg);
    args.push("--env".to_string());
    args.push("ELECTRO_WORKSPACE=/workspace".to_string());
    args.push("--env".to_string());
    args.push("HOME=/tmp".to_string());

    if matches!(backend, ResolvedBackend::Podman) {
        args.push("--userns".to_string());
        args.push("keep-id".to_string());
    }

    args.push(policy.container_image.clone());
    args.extend(parsed.argv().into_iter().map(String::from));
    args
}

pub async fn run_host_command(
    parsed: &ParsedCommand,
    timeout_secs: u64,
    ctx: &ToolContext,
    stdin_data: Option<Vec<u8>>,
) -> Result<ToolOutput, ElectroError> {
    validate_host_program(parsed)?;
    let (home, cache, config, state, tmp) = prepare_host_runtime_dirs(ctx)?;

    let mut command = tokio::process::Command::new(&parsed.program);
    command
        .args(&parsed.args)
        .current_dir(&ctx.workspace_path)
        .env_clear()
        .env("ELECTRO_WORKSPACE", &ctx.workspace_path)
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", &cache)
        .env("XDG_CONFIG_HOME", &config)
        .env("XDG_STATE_HOME", &state)
        .env("TMPDIR", &tmp)
        .env("CARGO_HOME", cache.join("cargo"))
        .env("RUSTUP_HOME", cache.join("rustup"))
        .env("PIP_CACHE_DIR", cache.join("pip"))
        .env("NPM_CONFIG_CACHE", cache.join("npm"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("PYTHONNOUSERSITE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }

    if let Ok(path) = std::env::var("PATH") {
        command.env("PATH", path);
    }
    if let Ok(lang) = std::env::var("LANG") {
        command.env("LANG", lang);
    }
    if let Ok(locale) = std::env::var("LC_ALL") {
        command.env("LC_ALL", locale);
    }
    if let Ok(term) = std::env::var("TERM") {
        command.env("TERM", term);
    }

    for name in passthrough_env_names() {
        if let Ok(value) = std::env::var(&name) {
            command.env(&name, value);
        }
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return Ok(ToolOutput {
            content: format!("Failed to execute command via host runner: {}", e),
            is_error: true,
        }),
    };

    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(&data).await;
        }
    }

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await;

    render_output(result, timeout_secs, "host")
}

pub async fn run_container_command(
    backend: ResolvedBackend,
    policy: &ShellRunnerPolicy,
    parsed: &ParsedCommand,
    timeout_secs: u64,
    ctx: &ToolContext,
    stdin_data: Option<Vec<u8>>,
) -> Result<ToolOutput, ElectroError> {
    let engine = container_engine_name(backend);
    let args = build_container_args(backend, policy, &ctx.workspace_path, parsed, stdin_data.is_some());

    let mut command = tokio::process::Command::new(engine);
    command
        .args(&args)
        .current_dir(&ctx.workspace_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return Ok(ToolOutput {
            content: format!("Failed to execute command via {} runner: {}", engine, e),
            is_error: true,
        }),
    };

    if let Some(data) = stdin_data {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(&data).await;
        }
    }

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        child.wait_with_output(),
    )
    .await;

    render_output(result, timeout_secs, engine)
}

fn render_output(
    result: Result<Result<std::process::Output, std::io::Error>, tokio::time::error::Elapsed>,
    timeout_secs: u64,
    runner_label: &str,
) -> Result<ToolOutput, ElectroError> {
    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let mut content = String::new();
            if !stdout.is_empty() {
                content.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str("[stderr]\n");
                content.push_str(&stderr);
            }

            if content.is_empty() {
                content = format!(
                    "Command completed with exit code {} via {} runner",
                    output.status.code().unwrap_or(-1),
                    runner_label
                );
            }

            if content.len() > MAX_OUTPUT_SIZE {
                let end = content
                    .char_indices()
                    .map(|(i, _)| i)
                    .take_while(|&i| i <= MAX_OUTPUT_SIZE)
                    .last()
                    .unwrap_or(0);
                content.truncate(end);
                content.push_str("\n... [output truncated]");
            }

            let is_error = !output.status.success();
            Ok(ToolOutput { content, is_error })
        }
        Ok(Err(e)) => Ok(ToolOutput {
            content: format!("Failed to execute command via {} runner: {}", runner_label, e),
            is_error: true,
        }),
        Err(_) => Ok(ToolOutput {
            content: format!(
                "Command timed out after {} seconds via {} runner",
                timeout_secs, runner_label
            ),
            is_error: true,
        }),
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a simple command inside the session workspace. By default ELECTRO uses an isolated docker or podman runner with a workspace-only mount and network disabled. Direct host execution requires both ELECTRO_SHELL_BACKEND=host and ELECTRO_ENABLE_HOST_SHELL=1. When host fallback is used, ELECTRO clears the inherited environment, points HOME/XDG state into the workspace, and blocks launcher programs such as sh and bash. Metacharacters, pipes, redirection, and command chaining are blocked."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "A single simple command to execute in the workspace (for example: 'git status' or 'ls -la'). Shell metacharacters are blocked."
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 300)"
                }
            },
            "required": ["command"]
        })
    }

    fn declarations(&self) -> CapabilityPolicy {
        CapabilityPolicy {
            file_access: Vec::new(),
            network_access: electro_core::net_policy::NetworkPolicy::Blocked,
            shell_access: electro_core::policy::ShellPolicy::Allowed,
browser_access: electro_core::policy::BrowserPolicy::Blocked,
        }
    }

    async fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ElectroError> {
        let command = input
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ElectroError::Tool("Missing required parameter: command".into()))?
            .trim();

        if command.is_empty() {
            return Ok(ToolOutput {
                content: "Command cannot be empty.".to_string(),
                is_error: true,
            });
        }

        if let Some(token) = command_contains_blocked_meta(command) {
            return Ok(ToolOutput {
                content: format!(
                    "Blocked shell command: metacharacter '{}' is not allowed. Use file/git tools where possible, or run a single simple command.",
                    token
                ),
                is_error: true,
            });
        }

        let parsed = match parse_command_line(command) {
            Ok(parsed) => parsed,
            Err(e) => {
                return Ok(ToolOutput {
                    content: e,
                    is_error: true,
                })
            }
        };

        let timeout_secs = input
            .arguments
            .get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(300);

        let policy = load_policy();
        let backend = match resolve_backend(&policy).await {
            Ok(backend) => backend,
            Err(e) => {
                return Ok(ToolOutput {
                    content: e,
                    is_error: true,
                })
            }
        };

        tracing::info!(
            backend = %container_engine_name(backend),
            command = %command,
            timeout = timeout_secs,
            allow_network = policy.allow_network,
            container_image = %policy.container_image,
            "Executing shell command"
        );

        match backend {
            ResolvedBackend::Host => run_host_command(&parsed, timeout_secs, ctx, None).await,
            ResolvedBackend::Docker | ResolvedBackend::Podman => {
                run_container_command(backend, &policy, &parsed, timeout_secs, ctx, None).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_line_handles_quotes() {
        let parsed = parse_command_line("git commit -m \"hello world\" 'file name.txt'")
            .expect("command should parse");
        assert_eq!(parsed.program, "git");
        assert_eq!(
            parsed.args,
            vec!["commit", "-m", "hello world", "file name.txt"]
        );
    }

    #[test]
    fn parse_command_line_rejects_unterminated_quote() {
        let err = parse_command_line("echo \"oops").unwrap_err();
        assert!(err.contains("Unterminated quote"));
    }

    #[test]
    fn build_container_args_disables_network_by_default() {
        let policy = ShellRunnerPolicy {
            backend: ShellBackend::Docker,
            container_image: DEFAULT_CONTAINER_IMAGE.to_string(),
            allow_network: false,
            memory_mb: 256,
            pids_limit: 128,
            cpu_limit: 1.0,
            tmpfs_mb: 64,
        };
        let parsed = ParsedCommand::new("ls".to_string(), vec!["-la".to_string()]);
        let args = build_container_args(
            ResolvedBackend::Docker,
            &policy,
            Path::new("/tmp/workspace"),
            &parsed,
            false,
        );

        assert!(args
            .windows(2)
            .any(|w| w.iter().map(String::as_str).eq(["--network", "none"])));
        assert!(args
            .windows(2)
            .any(|w| w.iter().map(String::as_str).eq(["--read-only", "--tmpfs"])));
        assert_eq!(args.last().map(String::as_str), Some("-la"));
    }

    #[test]
    fn auto_backend_name_is_stable() {
        assert_eq!(parse_backend("auto"), ShellBackend::Auto);
        assert_eq!(parse_backend("docker"), ShellBackend::Docker);
        assert_eq!(parse_backend("podman"), ShellBackend::Podman);
        assert_eq!(parse_backend("host"), ShellBackend::Host);
    }

    #[test]
    fn host_program_validation_blocks_shell_launchers() {
        let parsed = ParsedCommand::new("bash".to_string(), vec!["-lc".to_string(), "echo hi".to_string()]);
        let err = validate_host_program(&parsed).unwrap_err();
        assert!(err.to_string().contains("blocks launcher"));
    }
}

