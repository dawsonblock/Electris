//! ELECTRO Memory crate
//!
//! Provides persistent memory backends for conversations, long-term knowledge,
//! and skills. Three backends ship out of the box:
//!
//! - [`SqliteMemory`] — SQLite-backed (via sqlx), suitable for production.
//! - [`MarkdownMemory`] — Flat Markdown files, compatible with OpenClaw.
//! - [`ResilientMemory`] — Decorator that wraps any backend with automatic
//!   failover to an in-memory cache and repair logic.

pub mod failover;
pub mod markdown;
pub mod search;
pub mod sqlite;
pub mod sqlite_usage;

pub use failover::{FailoverConfig, MemoryHealthStatus, ResilientMemory};
pub use markdown::MarkdownMemory;
pub use sqlite::SqliteMemory;
pub use sqlite_usage::SqliteUsageStore;

use electro_core::error::ElectroError;
use electro_core::Memory;

/// Factory function: create a memory backend by name.
///
/// Supported backends:
/// - `"sqlite"` — requires `url` (e.g. `"sqlite:memory.db"` or `"sqlite::memory:"`).
/// - `"markdown"` — requires `url` to be a directory path.
pub async fn create_memory_backend(
    backend: &str,
    url: &str,
) -> Result<Box<dyn Memory>, ElectroError> {
    match backend {
        "sqlite" => {
            let mem = SqliteMemory::new(url).await?;
            Ok(Box::new(mem))
        }
        "markdown" => {
            let mem = MarkdownMemory::new(url).await?;
            Ok(Box::new(mem))
        }
        other => Err(ElectroError::Config(format!(
            "Unknown memory backend: {other}"
        ))),
    }
}
