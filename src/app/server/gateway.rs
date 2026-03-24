//! Gateway HTTP service surface — health endpoints and SSE streaming

use axum::extract::Extension;
use axum::routing::get;
use axum::{Json, Router};
use axum::response::Sse;
use axum::response::sse::Event;
use electro_runtime::{OutboundEvent, RuntimeHandle};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use futures::stream::Stream;

/// Liveness probe — always returns ok if service is running
async fn live() -> &'static str {
    "ok"
}

/// Readiness probe — returns runtime state (ready vs degraded)
async fn ready(
    Extension(runtime): Extension<RuntimeHandle>,
) -> Json<Value> {
    let agent_ready = runtime.agent().await.is_some();

    Json(json!({
        "status": if agent_ready { "ready" } else { "degraded" },
        "agent": agent_ready
    }))
}

/// SSE stream endpoint for outbound events
async fn stream(
    Extension(runtime): Extension<RuntimeHandle>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = runtime.subscribe_outbound_events();

    let stream = BroadcastStream::new(rx).filter_map(|msg| async {
        match msg {
            Ok(ev) => {
                let data = serde_json::to_string(&ev).ok()?;
                Some(Ok(Event::default().data(data)))
            }
            _ => None,
        }
    });

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("ping"))
}

/// Register health and stream routes on the given router
pub fn register_health_routes(router: Router, runtime: RuntimeHandle) -> Router {
    router
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/stream", get(stream))
        .layer(Extension(runtime))
}
