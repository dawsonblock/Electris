//! E2E Test: Gateway Roundtrip
//! 
//! Verifies that HTTP input enters queue and produces streamed output.

use std::sync::Arc;
use electro_core::types::config::{ElectroMode, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use electro_runtime::{OutboundEvent, RuntimeConfig, RuntimeHandle, ToolPolicyConfig};

#[tokio::test]
async fn gateway_http_input_enters_queue() {
    let (queue_tx, mut queue_rx) = tokio::sync::mpsc::channel(32);
    let runtime = RuntimeHandle::new_with_config(
        queue_tx.clone(),
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
        RuntimeConfig {
            max_concurrency: 4,
            worker_timeout: 300,
            tool_timeout_secs: 60,
            max_queue: 100,
            max_active_per_chat: 2,
            remote_threshold_chars: 500,
            remote_workers: vec![],
            remote_auth_token: None,
            remote_retries: 3,
            tool_policy: ToolPolicyConfig {
                allow_shell: true,
                allow_network: true,
                allow_filesystem: true,
                writable_roots: vec!["/tmp".to_string()],
            },
        },
    );

    // Simulate HTTP gateway receiving a message
    let http_msg = InboundMessage {
        id: format!("http-{}", uuid::Uuid::new_v4()),
        channel: "http".to_string(),
        chat_id: "http-session-001".to_string(),
        user_id: "http-user".to_string(),
        username: Some("api-client".to_string()),
        text: Some("Test HTTP request".to_string()),
        attachments: Vec::new(),
        reply_to: None,
        timestamp: chrono::Utc::now(),
    };

    let request_id = http_msg.id.clone();
    queue_tx.send(http_msg).await.expect("queue send should succeed");

    // Verify message enters queue
    let queued = queue_rx.recv().await.expect("message should be queued");
    assert_eq!(queued.channel, "http");
    assert_eq!(queued.chat_id, "http-session-001");
    assert_eq!(queued.text.as_deref(), Some("Test HTTP request"));

    // Setup SSE stream simulation
    let mut events = runtime.subscribe_outbound_events();

    // Simulate processing events
    runtime.emit_outbound_event(OutboundEvent::Started { request_id: request_id.clone() })
        .expect("should emit Started");
    
    runtime.emit_outbound_event(OutboundEvent::Token { 
        request_id: request_id.clone(), 
        content: "Processing".to_string() 
    }).expect("should emit Token");
    
    runtime.emit_outbound_event(OutboundEvent::Completed { 
        request_id: request_id.clone(), 
        content: "Response from gateway".to_string() 
    }).expect("should emit Completed");

    // Verify stream receives events
    let event1 = events.recv().await.expect("should receive Started");
    assert!(matches!(event1, OutboundEvent::Started { request_id: id } if id == request_id));

    let event2 = events.recv().await.expect("should receive Token");
    assert!(matches!(event2, OutboundEvent::Token { request_id: id, .. } if id == request_id));

    let event3 = events.recv().await.expect("should receive Completed");
    assert!(matches!(event3, OutboundEvent::Completed { request_id: id, .. } if id == request_id));
}

#[tokio::test]
async fn gateway_multiple_requests_handled() {
    let (queue_tx, mut queue_rx) = tokio::sync::mpsc::channel(32);
    let _runtime = RuntimeHandle::new(
        queue_tx.clone(),
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
    );

    // Send multiple HTTP requests
    for i in 0..3 {
        let msg = InboundMessage {
            id: format!("http-multi-{}", i),
            channel: "http".to_string(),
            chat_id: format!("session-{}", i),
            user_id: "api-user".to_string(),
            username: None,
            text: Some(format!("Request {}", i)),
            attachments: Vec::new(),
            reply_to: None,
            timestamp: chrono::Utc::now(),
        };
        queue_tx.send(msg).await.expect("send should succeed");
    }

    // Verify all messages enter queue
    let mut count = 0;
    while let Ok(Some(_)) = tokio::time::timeout(
        tokio::time::Duration::from_millis(100),
        queue_rx.recv()
    ).await {
        count += 1;
    }
    assert_eq!(count, 3, "all 3 messages should be queued");
}

#[tokio::test]
async fn gateway_stream_events_in_order() {
    let (queue_tx, _queue_rx) = tokio::sync::mpsc::channel(32);
    let runtime = RuntimeHandle::new(
        queue_tx,
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
    );

    let mut events = runtime.subscribe_outbound_events();
    let request_id = "ordered-001".to_string();

    // Emit events in sequence
    runtime.emit_outbound_event(OutboundEvent::Started { request_id: request_id.clone() }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Token { request_id: request_id.clone(), content: "1".to_string() }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Token { request_id: request_id.clone(), content: "2".to_string() }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Token { request_id: request_id.clone(), content: "3".to_string() }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Completed { request_id: request_id.clone(), content: "Done".to_string() }).unwrap();

    // Verify order
    let e1 = events.recv().await.unwrap();
    assert!(matches!(e1, OutboundEvent::Started { .. }));
    
    let e2 = events.recv().await.unwrap();
    assert!(matches!(e2, OutboundEvent::Token { content, .. } if content == "1"));
    
    let e3 = events.recv().await.unwrap();
    assert!(matches!(e3, OutboundEvent::Token { content, .. } if content == "2"));
    
    let e4 = events.recv().await.unwrap();
    assert!(matches!(e4, OutboundEvent::Token { content, .. } if content == "3"));
    
    let e5 = events.recv().await.unwrap();
    assert!(matches!(e5, OutboundEvent::Completed { .. }));
}
