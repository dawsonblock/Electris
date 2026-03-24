use anyhow::Result;
use electro_core::paths;
use std::io::Write;

/// Factory reset — wipe all local state and start fresh.
///
/// - Wipes all local state (~/.electro/)
/// - Creates backups before reset
/// - Handles confirmation prompts
pub async fn factory_reset(confirm: bool) -> Result<()> {
    let data_dir = paths::electro_home();

    if !data_dir.exists() {
        println!("Nothing to reset — {} does not exist.", data_dir.display());
        return Ok(());
    }

    // Check if daemon is running
    if let Some(pid) = read_pid_file() {
        if is_process_alive(pid) {
            eprintln!(
                "ELECTRO daemon is running (PID {}). Stop it first with `electro stop`.",
                pid
            );
            std::process::exit(1);
        }
    }

    // Confirmation gate
    if !confirm {
        println!("This will DELETE all ELECTRO local state:");
        println!("  {}/", data_dir.display());
        println!();
        println!("  - credentials.toml    (saved API keys)");
        println!("  - memory.db           (conversation history)");
        println!("  - allowlist.toml      (user access control)");
        println!("  - mcp.toml            (MCP server configs)");
        println!("  - config.toml         (local config overrides)");
        println!("  - oauth.json          (Codex OAuth tokens)");
        println!("  - custom-tools/       (user-authored tools)");
        println!("  - workspace/          (workspace files)");
        println!();
        println!("A backup will be saved before deletion.");
        println!();
        print!("Type 'reset' to confirm: ");
        std::io::stdout().flush().ok();

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        if input.trim() != "reset" {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Backup before wipe
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_dir = paths::backup_dir(&timestamp.to_string());

    // Copy directory tree for backup
    match copy_dir_recursive(&data_dir, &backup_dir) {
        Ok(()) => {
            println!("Backup saved to {}", backup_dir.display());
        }
        Err(e) => {
            eprintln!("Failed to create backup: {}", e);
            eprintln!("Aborting reset — your data is untouched.");
            std::process::exit(1);
        }
    }

    // Nuke everything
    match std::fs::remove_dir_all(&data_dir) {
        Ok(()) => {
            // Re-create the empty directory so future commands don't fail
            let _ = std::fs::create_dir_all(&data_dir);
            println!("Factory reset complete.");
            println!("Run `electro start` for fresh onboarding.");
        }
        Err(e) => {
            eprintln!("Failed to remove {}: {}", data_dir.display(), e);
            eprintln!("Backup is at {}", backup_dir.display());
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Get the path to the PID file: `~/.electro/electro.pid`
fn pid_file_path() -> Option<std::path::PathBuf> {
    Some(paths::pid_file())
}

/// Read the PID from the PID file.
fn read_pid_file() -> Option<u32> {
    let path = pid_file_path()?;
    let content = std::fs::read_to_string(&path).ok()?;
    content.trim().parse().ok()
}

/// Check if a process with the given PID is still running.
fn is_process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
        || std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
