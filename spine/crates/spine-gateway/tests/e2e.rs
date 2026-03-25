//! E2E tests for the gateway HTTP API.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use spine_gateway::create_router;

#[tokio::test]
async fn health_endpoint_returns_ok() {
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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn execute_endpoint_returns_success() {
    let app = create_router();

    let request_body = json!({
        "payload": {
            "action": "test",
            "data": "hello"
        }
    });

    let response = app
        .oneshot(
            Request::builder()
                .uri("/execute")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(request_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(json["success"], true);
    assert!(json["output"].as_str().unwrap().contains("test"));
}
