//! File system tools.
//!
//! All functions are `pub(crate)` - only accessible within this crate
//! and to the spine-worker crate.

use std::path::Path;

/// Read a file's contents.
///
/// # Arguments
/// * `path` - Path to the file
///
/// # Returns
/// * `Ok(String)` - File contents
/// * `Err(String)` - Error message
///
/// # Safety
/// This function should be called within the worker's sandbox context.
pub async fn read_file(path: impl AsRef<Path>) -> Result<String, String> {
    let path = path.as_ref();

    tracing::debug!(path = %path.display(), "fs: reading file");

    // Validate path is not a directory
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Cannot access {}: {}", path.display(), e))?;

    if metadata.is_dir() {
        return Err(format!("{} is a directory, not a file", path.display()));
    }

    // Check file size (100 MB limit)
    const MAX_SIZE: u64 = 100 * 1024 * 1024;
    if metadata.len() > MAX_SIZE {
        return Err(format!(
            "File {} too large: {} bytes (max {})",
            path.display(),
            metadata.len(),
            MAX_SIZE
        ));
    }

    tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))
}

/// Write content to a file.
///
/// # Arguments
/// * `path` - Path to the file
/// * `content` - Content to write
///
/// # Returns
/// * `Ok(())` - Write successful
/// * `Err(String)` - Error message
pub async fn write_file(
    path: impl AsRef<Path>,
    content: impl AsRef<[u8]>,
) -> Result<(), String> {
    let path = path.as_ref();

    tracing::debug!(path = %path.display(), "fs: writing file");

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    tokio::fs::write(path, content)
        .await
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Check if a path exists.
pub async fn exists(path: impl AsRef<Path>) -> bool {
    tokio::fs::metadata(path).await.is_ok()
}

/// List directory contents.
pub async fn list_dir(path: impl AsRef<Path>) -> Result<Vec<String>, String> {
    let path = path.as_ref();
    let mut entries = Vec::new();

    let mut dir = tokio::fs::read_dir(path)
        .await
        .map_err(|e| format!("Failed to read directory {}: {}", path.display(), e))?;

    while let Some(entry) = dir
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read entry: {e}"))?
    {
        entries.push(entry.file_name().to_string_lossy().to_string());
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_write_roundtrip() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("spine_test.txt");

        // Write
        write_file(&test_file, "hello world")
            .await
            .unwrap();

        // Read
        let content = read_file(&test_file).await.unwrap();
        assert_eq!(content, "hello world");

        // Cleanup
        let _ = tokio::fs::remove_file(&test_file).await;
    }

    #[tokio::test]
    async fn read_nonexistent_fails() {
        let result = read_file("/tmp/nonexistent_file_12345.txt").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_creates_parent_dirs() {
        let temp_dir = std::env::temp_dir();
        let nested = temp_dir.join("spine_test").join("nested").join("file.txt");

        write_file(&nested, "test content").await.unwrap();

        let content = read_file(&nested).await.unwrap();
        assert_eq!(content, "test content");

        // Cleanup
        let _ = tokio::fs::remove_dir_all(temp_dir.join("spine_test")).await;
    }

    #[tokio::test]
    async fn exists_works() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("spine_exists_test.txt");

        assert!(!exists(&test_file).await);

        write_file(&test_file, "test").await.unwrap();
        assert!(exists(&test_file).await);

        let _ = tokio::fs::remove_file(&test_file).await;
    }
}
