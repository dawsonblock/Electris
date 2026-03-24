use electro_core::paths;
use std::path::PathBuf;

/// Get the path to the PID file: `~/.electro/electro.pid`
pub fn pid_file_path() -> Option<PathBuf> {
    Some(paths::pid_file())
}

/// Write the current process PID to the PID file.
pub fn write_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = std::fs::write(&path, std::process::id().to_string());
    }
}

/// Remove the PID file.
pub fn remove_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Read the PID from the PID file.
pub fn read_pid_file() -> Option<u32> {
    let path = pid_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    content.trim().parse().ok()
}

/// Check if a process with the given PID is still running.
pub fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
        || std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// Stop a running daemon by PID.
/// Returns Ok(true) if stopped, Ok(false) if not running, Err if failed to stop.
pub fn stop_daemon() -> Result<bool, String> {
    match read_pid_file() {
        Some(pid) if is_process_alive(pid) => {
            // Send SIGTERM on Unix, taskkill on Windows
            #[cfg(unix)]
            {
                let status = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status();
                match status {
                    Ok(s) if s.success() => {
                        remove_pid_file();
                        Ok(true)
                    }
                    _ => Err(format!("Failed to stop ELECTRO daemon (PID {}).", pid)),
                }
            }
            #[cfg(windows)]
            {
                let status = std::process::Command::new("taskkill")
                    .args(["/PID", &pid.to_string(), "/F"])
                    .status();
                match status {
                    Ok(s) if s.success() => {
                        remove_pid_file();
                        Ok(true)
                    }
                    _ => Err(format!("Failed to stop ELECTRO daemon (PID {}).", pid)),
                }
            }
            #[cfg(not(any(unix, windows)))]
            {
                Err(format!(
                    "Process termination not implemented for this platform"
                ))
            }
        }
        Some(_pid) => {
            // Stale PID file — clean up
            remove_pid_file();
            Ok(false)
        }
        None => Ok(false),
    }
}

/// Helper for CLI to stop the daemon and print status
pub fn stop_daemon_cli() -> anyhow::Result<()> {
    match stop_daemon() {
        Ok(true) => {
            println!("ELECTRO daemon stopped.");
            Ok(())
        }
        Ok(false) => {
            eprintln!("No ELECTRO daemon running.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

/// Start the server as a background daemon.
/// Forks/detaches the process, sets up log redirection, and writes PID file.
/// Returns the child PID on success.
pub fn start_daemon(log_path: Option<String>) -> Result<u32, String> {
    let electro_dir = paths::electro_home();
    let _ = std::fs::create_dir_all(&electro_dir);

    // Check for saved credentials — daemon requires prior setup
    let creds_path = electro_dir.join("credentials.toml");
    if !creds_path.exists() {
        return Err(format!(
            "Error: No saved credentials found at {}\n\n\
             First-time setup requires foreground mode to complete onboarding.\n\
             Run `electro start` (without -d) first, then use -d for subsequent runs.",
            creds_path.display()
        ));
    }

    // Check if already running
    if let Some(pid) = read_pid_file() {
        if is_process_alive(pid) {
            return Err(format!(
                "ELECTRO daemon is already running (PID {}). Use `electro stop` first.",
                pid
            ));
        }
        // Stale PID file — clean up
        remove_pid_file();
    }

    // Resolve log path
    let log_path = log_path
        .map(PathBuf::from)
        .unwrap_or_else(|| electro_dir.join("electro.log"));

    // Re-exec ourselves as a detached child
    let exe = std::env::current_exe().expect("cannot resolve own executable path");
    let mut args: Vec<String> = std::env::args().collect();

    // Remove --daemon / -d flag so the child runs in foreground
    args.retain(|a| a != "--daemon" && a != "-d");

    // Remove --log and its value too
    let mut skip_next = false;
    args.retain(|a| {
        if skip_next {
            skip_next = false;
            return false;
        }
        if a == "--log" {
            skip_next = true;
            return false;
        }
        if a.starts_with("--log=") {
            return false;
        }
        true
    });

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Cannot open log file {}: {}", log_path.display(), e))?;

    let log_err = log_file
        .try_clone()
        .map_err(|e| format!("Cannot clone log file handle: {}", e))?;

    let child = std::process::Command::new(exe)
        .args(&args[1..]) // skip argv[0]
        .stdout(log_file)
        .stderr(log_err)
        .stdin(std::process::Stdio::null())
        .spawn();

    match child {
        Ok(c) => {
            // Write child PID
            let child_pid = c.id();
            if let Some(path) = pid_file_path() {
                let _ = std::fs::write(&path, child_pid.to_string());
            }
            Ok(child_pid)
        }
        Err(e) => Err(format!("Failed to start daemon: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_file_path() {
        let path = pid_file_path();
        assert!(path.is_some());
    }

    #[test]
    fn test_read_write_pid_file() {
        // This test uses the actual PID file, so be careful
        // In a real test environment, we'd use a temp file
        write_pid_file();
        let pid = read_pid_file();
        assert!(pid.is_some());
        assert_eq!(pid.unwrap(), std::process::id());
        remove_pid_file();
        assert!(read_pid_file().is_none());
    }
}
