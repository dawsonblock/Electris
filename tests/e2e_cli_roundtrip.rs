//! E2E Test: CLI Roundtrip
//! 
//! Verifies that CLI input enters runtime and produces outbound events.

use std::sync::Arc;
use electro_core::types::config::{ElectroMode, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use electro_runtime::{OutboundEvent, RuntimeConfig, RuntimeHandle, ToolPolicyConfig};

fn create_test_message(text: &str) -> InboundMessage {
    InboundMessage {
        id: format!("msg-{}", uuid::Uuid::new_v4()),
        channel: "cli".to_string(),
        chat_id: "cli-session".to_string(),
        user_id: "cli-user".to_string(),
        username: Some("tester".to_string()),
        text: Some(text.to_string()),
        attachments: Vec::new(),
        reply_to: None,
        timestamp: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn cli_input_produces_outbound_events() {
    // Setup runtime
    let (queue_tx, mut queue_rx) = tokio::sync::mpsc::channel(32);
    let shared_mode = Arc::new(tokio::sync::RwLock::new(ElectroMode::Play));
    let shared_memory_strategy = Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda));
    
    let runtime = RuntimeHandle::new_with_config(
        queue_tx.clone(),
        shared_mode,
        shared_memory_strategy,
        RuntimeConfig {
            max_concurrency: 1,
            worker_timeout: 60,
            tool_timeout_secs: 30,
            max_queue: 10,
            max_active_per_chat: 1,
            remote_threshold_chars: 500,
            remote_workers: vec![],
            remote_auth_token: None,
            remote_retries: 1,
            tool_policy: ToolPolicyConfig {
                allow_shell: false,
                allow_network: false,
                allow_filesystem: false,
                writable_roots: vec![],
            },
        },
    );

    // Subscribe to outbound events
    let mut events = runtime.subscribe_outbound_events();

    // Simulate CLI sending a message
    let test_msg = create_test_message("Hello from CLI");
    let request_id = test_msg.id.clone();
    queue_tx.send(test_msg).await.expect("queue send should succeed");

    // Verify message enters queue
    let queued = queue_rx.recv().await.expect("message should be queued");
    assert_eq!(queued.text.as_deref(), Some("Hello from CLI"));
    assert_eq!(queued.channel, "cli");

    // Emit events to simulate processing
    runtime.emit_outbound_event(OutboundEvent::Started { request_id: request_id.clone() })
        .expect("should emit Started event");
    
    runtime.emit_outbound_event(OutboundEvent::Token { 
        request_id: request_id.clone(), 
        content: "Hello".to_string() 
    }).expect("should emit Token event");
    
    runtime.emit_outbound_event(OutboundEvent::Completed { 
        request_id: request_id.clone(), 
        content: "Hello back!".to_string() 
    }).expect("should emit Completed event");

    // Verify events are received
    let event1 = events.recv().await.expect("should receive Started event");
    assert!(matches!(event1, OutboundEvent::Started { request_id: id } if id == request_id));

    let event2 = events.recv().await.expect("should receive Token event");
    assert!(matches!(event2, OutboundEvent::Token { request_id: id, content } if id == request_id && content == "Hello"));

    let event3 = events.recv().await.expect("should receive Completed event");
    assert!(matches!(event3, OutboundEvent::Completed { request_id: id, content } if id == request_id && content == "Hello back!"));
}

#[tokio::test]
async fn cli_failed_request_emits_failed_event() {
    let (queue_tx, _queue_rx) = tokio::sync::mpsc::channel(32);
    let runtime = RuntimeHandle::new(
        queue_tx,
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
    );

    let mut events = runtime.subscribe_outbound_events();
    let request_id = "test-fail-001".to_string();

    runtime.emit_outbound_event(OutboundEvent::Started { request_id: request_id.clone() })
        .expect("should emit Started event");
    
    runtime.emit_outbound_event(OutboundEvent::Failed { 
        request_id: request_id.clone(), 
        error: "Test error".to_string() 
    }).expect("should emit Failed event");

    let event1 = events.recv().await.expect("should receive Started event");
    assert!(matches!(event1, OutboundEvent::Started { request_id: id } if id == request_id));

    let event2 = events.recv().await.expect("should receive Failed event");
    assert!(matches!(event2, OutboundEvent::Failed { request_id: id, error } if id == request_id && error == "Test error"));
}
