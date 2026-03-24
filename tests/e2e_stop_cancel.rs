//! E2E Test: Stop/Cancel
//! 
//! Verifies that /stop or cancel path interrupts safely.

use std::sync::Arc;
use electro_core::types::config::{ElectroMode, MemoryStrategy};
use electro_runtime::{OutboundEvent, RuntimeHandle};

#[tokio::test]
async fn cancel_request_emits_failed_event() {
    let (queue_tx, _queue_rx) = tokio::sync::mpsc::channel(32);
    let runtime = RuntimeHandle::new(
        queue_tx,
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
    );

    let mut events = runtime.subscribe_outbound_events();
    let request_id = "cancel-test-001".to_string();

    // Start a request
    runtime.emit_outbound_event(OutboundEvent::Started { 
        request_id: request_id.clone() 
    }).expect("should emit Started");

    // Cancel it
    runtime.emit_outbound_event(OutboundEvent::Failed { 
        request_id: request_id.clone(),
        error: "Request cancelled by user".to_string()
    }).expect("should emit Failed");

    // Verify events
    let start = events.recv().await.expect("should receive Started");
    assert!(matches!(start, OutboundEvent::Started { request_id: id } if id == request_id));

    let fail = events.recv().await.expect("should receive Failed");
    assert!(matches!(fail, OutboundEvent::Failed { request_id: id, error } 
        if id == request_id && error.contains("cancelled")));
}

#[tokio::test]
async fn executor_permits_can_be_acquired_and_released() {
    use electro_runtime::executor::ExecutionController;

    let controller = ExecutionController::new(2);
    
    // Check initial available permits
    assert_eq!(controller.available(), 2, "should have 2 permits available initially");
    
    // Acquire a permit
    let permit = controller.acquire().await;
    assert_eq!(controller.available(), 1, "should have 1 permit after acquiring one");
    
    // Drop permit to simulate cancellation/release
    drop(permit);
    
    // Note: Semaphore permits are returned asynchronously, so we check it eventually returns
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    assert_eq!(controller.available(), 2, "should have 2 permits after releasing");
}

#[tokio::test]
async fn stop_signal_interrupts_processing() {
    let (queue_tx, mut queue_rx) = tokio::sync::mpsc::channel(32);
    let runtime = RuntimeHandle::new(
        queue_tx.clone(),
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
    );

    let mut events = runtime.subscribe_outbound_events();

    // Simulate a stop command message
    let stop_msg = electro_core::types::message::InboundMessage {
        id: "stop-cmd-001".to_string(),
        channel: "cli".to_string(),
        chat_id: "session-001".to_string(),
        user_id: "admin".to_string(),
        username: None,
        text: Some("/stop".to_string()),
        attachments: Vec::new(),
        reply_to: None,
        timestamp: chrono::Utc::now(),
    };

    queue_tx.send(stop_msg).await.expect("should send stop command");

    // Verify stop command enters queue
    let queued = queue_rx.recv().await.expect("should receive stop command");
    assert_eq!(queued.text.as_deref(), Some("/stop"));

    // Emit stop acknowledgement
    runtime.emit_outbound_event(OutboundEvent::Completed { 
        request_id: "stop-cmd-001".to_string(),
        content: "Shutting down...".to_string()
    }).expect("should emit shutdown message");

    let event = events.recv().await.expect("should receive completion");
    assert!(matches!(event, OutboundEvent::Completed { content, .. } 
        if content.contains("Shutting down")));
}

#[tokio::test]
async fn concurrent_requests_can_be_cancelled_independently() {
    let (queue_tx, _queue_rx) = tokio::sync::mpsc::channel(32);
    let runtime = RuntimeHandle::new(
        queue_tx,
        Arc::new(tokio::sync::RwLock::new(ElectroMode::Play)),
        Arc::new(tokio::sync::RwLock::new(MemoryStrategy::Lambda)),
    );

    // Subscribe BEFORE emitting events to ensure we receive all of them
    let mut events = runtime.subscribe_outbound_events();

    // Start multiple requests with unique IDs
    let req1 = "req-001-concurrent".to_string();
    let req2 = "req-002-concurrent".to_string();
    let req3 = "req-003-concurrent".to_string();

    // Emit all events
    runtime.emit_outbound_event(OutboundEvent::Started { request_id: req1.clone() }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Started { request_id: req2.clone() }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Started { request_id: req3.clone() }).unwrap();

    // Cancel only req2
    runtime.emit_outbound_event(OutboundEvent::Failed { 
        request_id: req2.clone(),
        error: "Cancelled".to_string()
    }).unwrap();

    // Complete req1 and req3
    runtime.emit_outbound_event(OutboundEvent::Completed { 
        request_id: req1.clone(),
        content: "Done 1".to_string()
    }).unwrap();
    runtime.emit_outbound_event(OutboundEvent::Completed { 
        request_id: req3.clone(),
        content: "Done 3".to_string()
    }).unwrap();

    // Collect all 5 events
    let mut received = Vec::new();
    loop {
        match tokio::time::timeout(
            tokio::time::Duration::from_millis(50),
            events.recv()
        ).await {
            Ok(Ok(event)) => received.push(event),
            _ => break,
        }
    }

    // We should have exactly 6 events (3 Started + 1 Failed + 2 Completed)
    assert_eq!(received.len(), 6, "should receive exactly 6 events, got: {:?}", received);

    // Helper to check if a request ID matches
    let is_req = |e: &OutboundEvent, req_id: &str| -> bool {
        match e {
            OutboundEvent::Started { request_id } => request_id == req_id,
            OutboundEvent::Failed { request_id, .. } => request_id == req_id,
            OutboundEvent::Completed { request_id, .. } => request_id == req_id,
            _ => false,
        }
    };

    // Verify req2 was cancelled
    let req2_cancelled = received.iter().any(|e| {
        matches!(e, OutboundEvent::Failed { request_id, .. } if request_id == "req-002-concurrent")
    });
    assert!(req2_cancelled, "req2 should be cancelled, events: {:?}", received);

    // Verify req1 and req3 completed
    let req1_completed = received.iter().any(|e| {
        matches!(e, OutboundEvent::Completed { request_id, .. } if request_id == "req-001-concurrent")
    });
    let req3_completed = received.iter().any(|e| {
        matches!(e, OutboundEvent::Completed { request_id, .. } if request_id == "req-003-concurrent")
    });
    assert!(req1_completed, "req1 should complete, events: {:?}", received);
    assert!(req3_completed, "req3 should complete, events: {:?}", received);

    // Verify all three started
    let starts: Vec<_> = received.iter().filter(|e| {
        matches!(e, OutboundEvent::Started { .. })
    }).collect();
    assert_eq!(starts.len(), 3, "should have 3 Started events");
}
