//! Integration test: start the gateway server and hit the /health endpoint.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use tower::ServiceExt;

use electro_agent::AgentRuntime;
use electro_core::types::config::{ElectroMode, GatewayConfig, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use electro_gateway::health::{health_handler, readiness_handler};
use electro_gateway::server::AppState;
use electro_gateway::session::SessionManager;
use electro_runtime::RuntimeHandle;
use electro_test_utils::{MockMemory, MockProvider};
use tokio::sync::{mpsc, RwLock};

fn make_test_state() -> Arc<AppState> {
    let provider = Arc::new(MockProvider::with_text("test"));
    let memory = Arc::new(MockMemory::new());
    let agent = Arc::new(AgentRuntime::new(
        provider,
        memory,
        vec![],
        "test-model".to_string(),
        None,
    ));

    Arc::new(AppState {
        channels: vec![],
        agent: Some(agent),
        runtime: None,
        config: GatewayConfig::default(),
        sessions: SessionManager::new(),
        identity: None,
    })
}

async fn make_runtime_state(agent_ready: bool) -> Arc<AppState> {
    let (queue_tx, _queue_rx) = mpsc::channel::<InboundMessage>(1);
    let runtime = RuntimeHandle::new(
        queue_tx,
        Arc::new(RwLock::new(ElectroMode::Play)),
        Arc::new(RwLock::new(MemoryStrategy::Lambda)),
    );

    if agent_ready {
        let provider = Arc::new(MockProvider::with_text("test"));
        let memory = Arc::new(MockMemory::new());
        let agent = Arc::new(AgentRuntime::new(
            provider,
            memory,
            vec![],
            "test-model".to_string(),
            None,
        ));
        runtime.set_agent_arc(agent).await;
    }

    Arc::new(AppState {
        channels: vec![],
        agent: None,
        runtime: Some(runtime),
        config: GatewayConfig::default(),
        sessions: SessionManager::new(),
        identity: None,
    })
}

#[tokio::test]
async fn health_endpoint_returns_200_json() {
    let state = make_test_state();

    let app = Router::new()
        .route("/health", get(health_handler))
        .with_state(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string());
    assert!(json["uptime_seconds"].is_number());
}

#[tokio::test]
async fn nonexistent_route_returns_404() {
    let state = make_test_state();

    let app = Router::new()
        .route("/health", get(health_handler))
        .with_state(state);

    let req = Request::builder()
        .uri("/nonexistent")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn readiness_endpoint_reports_degraded_when_agent_missing() {
    let state = make_runtime_state(false).await;

    let app = Router::new()
        .route("/health/ready", get(readiness_handler))
        .with_state(state);

    let req = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Degraded state returns 503 Service Unavailable but with valid JSON body
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "degraded");
    assert_eq!(json["agent"], false);
}

#[tokio::test]
async fn readiness_endpoint_reports_ready_when_agent_available() {
    let state = make_runtime_state(true).await;

    let app = Router::new()
        .route("/health/ready", get(readiness_handler))
        .with_state(state);

    let req = Request::builder()
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ready");
    assert_eq!(json["agent"], true);
}
