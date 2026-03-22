use crate::types::error::ElectroError;
use crate::types::file::FileMetadata;
use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;

/// File storage backend trait — local filesystem or cloud object storage
#[async_trait]
pub trait FileStore: Send + Sync {
    /// Store a file and return its storage key
    async fn store(
        &self,
        path: &str,
        data: Bytes,
        metadata: FileMetadata,
    ) -> Result<String, ElectroError>;

    /// Store a file from a stream (for large files)
    async fn store_stream(
        &self,
        path: &str,
        stream: BoxStream<'_, Bytes>,
        metadata: FileMetadata,
    ) -> Result<String, ElectroError>;

    /// Retrieve a file by its storage key
    async fn get(&self, key: &str) -> Result<Option<Bytes>, ElectroError>;

    /// Generate a presigned URL for direct access (for cloud backends)
    async fn presigned_url(
        &self,
        key: &str,
        expires_in_secs: u64,
    ) -> Result<Option<String>, ElectroError>;

    /// Delete a file
    async fn delete(&self, key: &str) -> Result<(), ElectroError>;

    /// List files in a path prefix
    async fn list(&self, prefix: &str) -> Result<Vec<String>, ElectroError>;

    /// Backend name (e.g., "local", "s3")
    fn backend_name(&self) -> &str;
}
