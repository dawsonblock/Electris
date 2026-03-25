//! Worker - the execution boundary.
//!
//! This crate is the **ONLY** place where tools may be executed.
//! It provides isolation, policy enforcement, and sandboxing for tool execution.
//!
//! # Invariant
//! No other crate may execute tools directly.
//! All tool execution must go through the worker.

mod executor;
mod policy;
mod sandbox;

pub use executor::execute_command;
pub use policy::{ExecutionPolicy, PolicyChecker};
pub use sandbox::{ResourceLimits, Sandbox};
