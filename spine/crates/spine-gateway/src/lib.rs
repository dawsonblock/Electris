//! Gateway - HTTP transport only.
//!
//! The gateway provides HTTP endpoints and routes requests to the runtime.
//! It contains NO business logic and NO direct tool access.
//! It ONLY calls spine_runtime::submit_intent().

mod routes;
mod server;

pub use routes::create_router;
pub use server::run_server;
