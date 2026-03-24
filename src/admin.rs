use electro_core::paths;

/// Check if a user ID is in the admin allowlist.
#[allow(dead_code)]
pub fn is_admin_user(user_id: &str) -> bool {
    let path = paths::allowlist_file();
    let path = path.with_extension("toml");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Parse just the admin field — keep it minimal to avoid coupling with channel types
    #[derive(serde::Deserialize)]
    struct AllowlistCheck {
        admin: String,
    }
    match toml::from_str::<AllowlistCheck>(&content) {
        Ok(al) => al.admin == user_id,
        Err(_) => false,
    }
}

/// Get the path to the PID file: `~/.electro/electro.pid`
#[allow(dead_code)]
pub fn pid_file_path() -> Option<std::path::PathBuf> {
    Some(paths::pid_file())
}

/// Write the current process PID to the PID file.
#[allow(dead_code)]
pub fn write_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = std::fs::write(&path, std::process::id().to_string());
    }
}

/// Remove the PID file.
#[allow(dead_code)]
pub fn remove_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// Read the PID from the PID file.
#[allow(dead_code)]
pub fn read_pid_file() -> Option<u32> {
    let path = pid_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    content.trim().parse().ok()
}

/// Check if a process with the given PID is still running.
#[allow(dead_code)]
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
