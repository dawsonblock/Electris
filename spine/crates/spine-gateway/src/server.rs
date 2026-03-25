//! HTTP server for the gateway.

use std::net::SocketAddr;

use axum::Router;

use crate::routes::create_router;

/// Run the HTTP server.
///
/// # Example
/// ```no_run
/// use spine_gateway::run_server;
///
/// # async fn example() {
/// run_server(8080).await;
/// # }
/// ```
pub async fn run_server(port: u16) {
    let app = create_router();

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("gateway: listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}

/// Create the router (for testing/mounting).
#[allow(dead_code)]
pub fn create_app() -> Router {
    create_router()
}
