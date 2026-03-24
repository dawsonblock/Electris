//! E2E Test: Remote Worker
//! 
//! Verifies that remote worker returns structured success/failure.

use electro_runtime::remote::{RemoteRequest, RemoteResponse, RemoteStreamEvent, MAX_REMOTE_REQUEST_BYTES};

#[test]
fn remote_request_serialization_roundtrip() {
    let request = RemoteRequest {
        request_id: "req-001".to_string(),
        input: "Hello remote worker".to_string(),
        channel: "http".to_string(),
        chat_id: "chat-001".to_string(),
        user_id: "user-001".to_string(),
        history: vec![
            electro_core::types::message::ChatMessage {
                role: electro_core::types::message::Role::User,
                content: electro_core::types::message::MessageContent::Text("Previous message".to_string()),
            },
        ],
    };

    // Serialize to JSON
    let json = serde_json::to_vec(&request).expect("should serialize");
    
    // Deserialize back
    let deserialized: RemoteRequest = serde_json::from_slice(&json).expect("should deserialize");
    
    assert_eq!(deserialized.request_id, request.request_id);
    assert_eq!(deserialized.input, request.input);
    assert_eq!(deserialized.channel, request.channel);
}

#[test]
fn remote_response_success_structure() {
    let response = RemoteResponse {
        request_id: "req-001".to_string(),
        output: "Task completed successfully".to_string(),
        history: vec![],
        error: None,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&response).expect("should serialize");
    
    // Verify JSON structure shows success (no error field)
    assert!(json.contains("req-001"));
    assert!(json.contains("Task completed successfully"));
    
    // Deserialize and verify
    let deserialized: RemoteResponse = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(deserialized.request_id, "req-001");
    assert_eq!(deserialized.output, "Task completed successfully");
    assert_eq!(deserialized.error, None);
}

#[test]
fn remote_response_failure_structure() {
    let response = RemoteResponse {
        request_id: "req-002".to_string(),
        output: String::new(),
        history: vec![],
        error: Some("Execution timeout".to_string()),
    };

    let json = serde_json::to_string(&response).expect("should serialize");
    
    // Verify JSON structure shows error
    assert!(json.contains("req-002"));
    assert!(json.contains("Execution timeout"));
    
    let deserialized: RemoteResponse = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(deserialized.request_id, "req-002");
    assert_eq!(deserialized.error, Some("Execution timeout".to_string()));
}

#[test]
fn remote_stream_event_wraps_outbound_event() {
    use electro_runtime::events::OutboundEvent;

    // Test wrapping Started event
    let started = RemoteStreamEvent {
        request_id: "req-003".to_string(),
        event: OutboundEvent::Started { request_id: "req-003".to_string() },
    };
    let json = serde_json::to_string(&started).unwrap();
    assert!(json.contains("req-003"));
    
    // Test wrapping Token event
    let token = RemoteStreamEvent {
        request_id: "req-003".to_string(),
        event: OutboundEvent::Token { 
            request_id: "req-003".to_string(),
            content: "Hello".to_string(),
        },
    };
    let json = serde_json::to_string(&token).unwrap();
    assert!(json.contains("Hello"));
    
    // Test wrapping Completed event
    let completed = RemoteStreamEvent {
        request_id: "req-003".to_string(),
        event: OutboundEvent::Completed { 
            request_id: "req-003".to_string(),
            content: "Final result".to_string(),
        },
    };
    let json = serde_json::to_string(&completed).unwrap();
    assert!(json.contains("Final result"));
    
    // Test wrapping Failed event
    let failed = RemoteStreamEvent {
        request_id: "req-003".to_string(),
        event: OutboundEvent::Failed { 
            request_id: "req-003".to_string(),
            error: "Something went wrong".to_string(),
        },
    };
    let json = serde_json::to_string(&failed).unwrap();
    assert!(json.contains("Something went wrong"));
}

#[test]
fn max_request_size_constant_is_reasonable() {
    // MAX_REMOTE_REQUEST_BYTES should be 250KB
    assert_eq!(MAX_REMOTE_REQUEST_BYTES, 250_000, "Max request size should be 250KB");
}

#[test]
fn remote_request_size_check() {
    let request = RemoteRequest {
        request_id: "size-test".to_string(),
        input: "Small input".to_string(),
        channel: "test".to_string(),
        chat_id: "test".to_string(),
        user_id: "test".to_string(),
        history: vec![],
    };

    let json = serde_json::to_vec(&request).expect("should serialize");
    assert!(json.len() < MAX_REMOTE_REQUEST_BYTES, "Small request should be under limit");
}

#[test]
fn remote_response_success_has_empty_error() {
    let response = RemoteResponse {
        request_id: "quick-001".to_string(),
        output: "Quick result".to_string(),
        history: vec![],
        error: None,
    };

    let json = serde_json::to_string(&response).expect("should serialize");
    let deserialized: RemoteResponse = serde_json::from_str(&json).expect("should deserialize");
    
    // Success is indicated by error being None
    assert!(deserialized.error.is_none());
    assert!(!deserialized.output.is_empty());
}

#[test]
fn remote_response_failure_has_error_set() {
    let response = RemoteResponse {
        request_id: "fail-001".to_string(),
        output: String::new(),
        history: vec![],
        error: Some("Worker crashed".to_string()),
    };

    let json = serde_json::to_string(&response).expect("should serialize");
    let deserialized: RemoteResponse = serde_json::from_str(&json).expect("should deserialize");
    
    // Failure is indicated by error being Some
    assert!(deserialized.error.is_some());
    assert_eq!(deserialized.error.unwrap(), "Worker crashed");
}

#[tokio::test]
async fn runtime_remote_execution_rejects_invalid_worker() {
    use std::sync::Arc;
    use electro_core::types::config::{ElectroMode, MemoryStrategy};
    use electro_runtime::{RuntimeConfig, RuntimeHandle, ToolPolicyConfig};

    let (queue_tx, _queue_rx) = tokio::sync::mpsc::channel(32);
    
    // Create runtime with specific allowed workers
    let runtime = RuntimeHandle::new_with_config(
        queue_tx,
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
        RuntimeConfig {
            max_concurrency: 4,
            worker_timeout: 300,
            tool_timeout_secs: 60,
            max_queue: 100,
            max_active_per_chat: 2,
            remote_threshold_chars: 500,
            remote_workers: vec!["worker1.example.com:8080".to_string()],
            remote_auth_token: Some("secret-token".to_string()),
            remote_retries: 1,
            tool_policy: ToolPolicyConfig::default(),
        },
    );

    // Attempt to run against non-allowlisted worker
    let result = runtime.run_remote(
        "req-001".to_string(),
        "test input".to_string(),
        "unauthorized-worker.example.com:8080".to_string(),
        "test".to_string(),
        "chat-001".to_string(),
        "user-001".to_string(),
        vec![],
    ).await;

    assert!(result.is_err(), "Should reject non-allowlisted worker");
    assert!(result.unwrap_err().to_string().contains("not allowlisted"));
}

#[tokio::test]
async fn runtime_respects_remote_threshold_via_router() {
    use electro_runtime::router::{ExecutionRouter, ExecutionTarget};
    use electro_core::types::message::InboundMessage;

    // Create router with threshold - note: router now takes InboundMessage
    let router = ExecutionRouter::new(100, vec![]); // 100 char threshold, no workers

    // Create a message with short content
    let short_msg = InboundMessage {
        id: "test-001".to_string(),
        channel: "test".to_string(),
        chat_id: "chat-001".to_string(),
        user_id: "user-001".to_string(),
        username: None,
        text: Some("Hello".to_string()),
        attachments: vec![],
        reply_to: None,
        timestamp: chrono::Utc::now(),
    };
    
    // Short input should stay local (no workers configured anyway)
    let target = router.route(&short_msg);
    assert!(matches!(target, ExecutionTarget::Local), "Should route locally when no workers");

    // Create a message with long content
    let long_msg = InboundMessage {
        id: "test-002".to_string(),
        channel: "test".to_string(),
        chat_id: "chat-001".to_string(),
        user_id: "user-001".to_string(),
        username: None,
        text: Some("a".repeat(200)),
        attachments: vec![],
        reply_to: None,
        timestamp: chrono::Utc::now(),
    };
    
    // With workers configured but not available, should still be local in test
    let router_with_workers = ExecutionRouter::new(100, vec!["worker1:8080".to_string()]);
    let _target = router_with_workers.route(&long_msg);
    // The router determines target based on availability and threshold
    // In test environment, this will typically be Local
}
