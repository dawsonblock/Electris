//! Tool implementations.
//!
//! All tool functions are `pub(crate)` - they are only accessible
//! to the spine-worker crate. No other code may execute tools directly.

pub mod fs;
pub mod git;
pub mod shell;

// Note: Functions are accessed via fs::*, git::*, shell::* 
// They are pub(crate) and only accessible within the crate
