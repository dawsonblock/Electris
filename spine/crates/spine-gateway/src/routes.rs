//! HTTP routes for the gateway.
//!
//! All routes delegate to spine_runtime::submit_intent().
//! No route contains business logic.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use spine_core::Intent;
use spine_runtime::submit_intent;
use std::sync::Arc;

/// Application state (shared across handlers).
#[derive(Clone)]
pub struct AppState;

/// Request to execute an intent.
#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    /// The intent payload
    pub payload: serde_json::Value,
}

/// Response from execution.
#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    pub success: bool,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// Create the router with all routes.
pub fn create_router() -> Router {
    let state = Arc::new(AppState);

    Router::new()
        .route("/execute", post(handle_execute))
        .route("/health", axum::routing::get(handle_health))
        .with_state(state)
}

/// POST /execute - Submit an intent for execution.
///
/// This is the main entrypoint for external requests.
/// It ONLY calls submit_intent() and returns the result.
async fn handle_execute(
    State(_state): State<Arc<AppState>>,
    Json(req): Json<ExecuteRequest>,
) -> impl IntoResponse {
    tracing::info!("gateway: received execute request");

    // Create intent from request
    let intent = Intent::new(req.payload);

    // Submit to runtime - this is the ONLY call we make
    let outcome = submit_intent(intent).await;

    // Return outcome as response
    let response = ExecuteResponse {
        success: outcome.success,
        output: outcome.output,
        error: outcome.error,
    };

    let status = if response.success {
        StatusCode::OK
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };

    (status, Json(response))
}

/// GET /health - Health check endpoint.
async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_check_returns_ok() {
        let app = create_router();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn execute_endpoint_accepts_request() {
        let app = create_router();

        let body = serde_json::json!({
            "payload": {
                "action": "test"
            }
        });

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/execute")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
