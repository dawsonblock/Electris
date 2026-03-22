use std::path::{Path, PathBuf};
use crate::types::error::ElectroError;

/// Resolve a user-provided path relative to a root, ensuring it does not escape.
///
/// This handles:
/// - Canonicalization of both the root and the final path.
/// - Rejection of absolute paths that are outside the root.
/// - Rejection of `..` or symbolic link escapes.
pub fn resolve_safe_path(root: &Path, user_path: &Path) -> Result<PathBuf, ElectroError> {
    // 1. Canonicalize the root to handle symlinks in the base path
    let root = root.canonicalize().map_err(|e| {
        ElectroError::Policy(format!("Failed to resolve workspace root: {e}"))
    })?;

    // 2. Resolve the absolute path
    let absolute_path = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        root.join(user_path)
    };

    // 3. Canonicalize the final path (handles all .. and symlinks)
    // Note: The file or directory must exist for canonicalize() to work on some OSs.
    // If it doesn't exist, we can't fully trust canonicalize.
    // However, for Hardening Phase 6, we require existing paths or we handle 
    // the "next-to-be-created" case by checking the parent.
    
    let resolved = match absolute_path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Path doesn't exist. Check its parent's canonical path.
            if let Some(parent) = absolute_path.parent() {
                let canon_parent = parent.canonicalize().map_err(|e| {
                    ElectroError::Policy(format!("Invalid path (parent escape or missing): {e}"))
                })?;
                canon_parent.join(absolute_path.file_name().unwrap_or_default())
            } else {
                return Err(ElectroError::Policy("Cannot resolve path with no parent".into()));
            }
        }
    };

    // 4. Prefix check: must start with root
    if !resolved.starts_with(&root) {
        return Err(ElectroError::Policy(format!(
            "Path escape detected: {} is outside of {}",
            resolved.display(),
            root.display()
        )));
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_safe_path_basic() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        
        // Simple relative
        let p = resolve_safe_path(&root, Path::new("test.txt")).unwrap();
        assert!(p.starts_with(&root));
        assert!(p.ends_with("test.txt"));
    }

    #[test]
    fn test_resolve_safe_path_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        
        // Parent escape
        let res = resolve_safe_path(&root, Path::new("../outside.txt"));
        assert!(res.is_err());
    }

    #[test]
    fn test_resolve_safe_path_absolute_outside() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        
        let res = resolve_safe_path(&root, Path::new("/etc/passwd"));
        assert!(res.is_err());
    }

    #[test]
    fn test_resolve_safe_path_symlink_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let outside = tempdir().unwrap();
        let outside_path = outside.path().canonicalize().unwrap();
        
        // Create a symlink pointing outside
        let link_target = outside_path.join("secret.txt");
        fs::write(&link_target, "secret").unwrap();
        
        let link_path = root.join("malleable_link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&link_target, &link_path).unwrap();
        
        // Resolving the link should fail prefix check
        let res = resolve_safe_path(&root, Path::new("malleable_link"));
        assert!(res.is_err());
    }
}
