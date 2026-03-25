//! Core types for the runtime spine.

pub mod command;
pub mod event;
pub mod intent;
pub mod outcome;

pub use command::Command;
pub use event::DomainEvent;
pub use intent::Intent;
pub use outcome::Outcome;
