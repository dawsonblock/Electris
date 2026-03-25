//! Git operations.
//!
//! All functions are `pub(crate)` - only accessible within this crate
//! and to the spine-worker crate.

use std::path::Path;
use tokio::process::Command;

/// Run a git command in the given directory.
///
/// # Arguments
/// * `dir` - Directory containing the git repository
/// * `args` - Git command arguments
///
/// # Returns
/// * `Ok(String)` - Command stdout
/// * `Err(String)` - Error message
pub async fn run(dir: impl AsRef<Path>, args: &[&str]) -> Result<String, String> {
    let dir = dir.as_ref();

    tracing::debug!(
        dir = %dir.display(),
        args = ?args,
        "git: running command"
    );

    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .map_err(|e| format!("Failed to run git: {e}"))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8: {e}"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Git failed: {stderr}"))
    }
}

/// Get the current branch name.
pub async fn current_branch(dir: impl AsRef<Path>) -> Result<String, String> {
    let output = run(dir, &["branch", "--show-current"]).await?;
    Ok(output.trim().to_string())
}

/// Get the current commit hash (full).
pub async fn current_commit(dir: impl AsRef<Path>) -> Result<String, String> {
    let output = run(dir, &["rev-parse", "HEAD"]).await?;
    Ok(output.trim().to_string())
}

/// Get the short commit hash.
pub async fn current_commit_short(dir: impl AsRef<Path>) -> Result<String, String> {
    let output = run(dir, &["rev-parse", "--short", "HEAD"]).await?;
    Ok(output.trim().to_string())
}

/// Check if there are uncommitted changes.
pub async fn has_changes(dir: impl AsRef<Path>) -> Result<bool, String> {
    let output = run(dir, &["status", "--porcelain"]).await?;
    Ok(!output.trim().is_empty())
}

/// Get repository status as a formatted string.
pub async fn status(dir: impl AsRef<Path>) -> Result<String, String> {
    run(dir, &["status", "-sb"]).await
}

/// Add files to staging area.
pub async fn add(dir: impl AsRef<Path>, paths: &[&str]) -> Result<(), String> {
    let mut args = vec!["add"];
    args.extend_from_slice(paths);
    run(dir, &args).await?;
    Ok(())
}

/// Commit staged changes.
pub async fn commit(dir: impl AsRef<Path>, message: &str) -> Result<String, String> {
    run(dir, &["commit", "-m", message]).await
}

/// Push to remote.
pub async fn push(dir: impl AsRef<Path>, remote: &str, branch: &str) -> Result<String, String> {
    run(dir, &["push", remote, branch]).await
}

/// Pull from remote.
pub async fn pull(dir: impl AsRef<Path>) -> Result<String, String> {
    run(dir, &["pull"]).await
}

/// Get the repository root directory.
pub async fn repo_root(dir: impl AsRef<Path>) -> Result<String, String> {
    let output = run(dir, &["rev-parse", "--show-toplevel"]).await?;
    Ok(output.trim().to_string())
}

/// List recent commits.
pub async fn log(
    dir: impl AsRef<Path>,
    count: usize,
    format: &str,
) -> Result<String, String> {
    run(dir, &[
        "log",
        &format!("-n{count}"),
        &format!("--pretty=format:{format}"),
    ])
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn git_status_in_current_dir() {
        // This test runs in the current git repo
        let result = run(".", &["status"]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn current_branch_works() {
        let branch = current_branch(".").await;
        // May fail if not in a git repo
        if let Ok(name) = branch {
            assert!(!name.is_empty());
        }
    }

    #[tokio::test]
    async fn current_commit_works() {
        let commit = current_commit(".").await;
        if let Ok(hash) = commit {
            assert_eq!(hash.len(), 40); // Full SHA-1 hash
        }
    }

    #[tokio::test]
    async fn repo_root_finds_git_root() {
        let root = repo_root(".").await;
        if let Ok(path) = root {
            assert!(!path.is_empty());
            // Path should exist
            assert!(Path::new(&path).exists());
        }
    }
}
