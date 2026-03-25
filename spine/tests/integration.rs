use spine_core::Intent;
use spine_runtime::submit_intent;

#[tokio::test]
async fn test_single_path_execution() {
    let intent = Intent::new(serde_json::json!({
        "action": "test",
        "data": "hello"
    }));

    let outcome = submit_intent(intent).await;

    assert!(outcome.success);
    assert!(outcome.output.as_ref().unwrap().contains("test"));
}

#[tokio::test]
async fn test_intent_isolation() {
    let intent1 = Intent::new(serde_json::json!({"action": "test"}));
    let intent2 = Intent::new(serde_json::json!({"action": "analyze"}));

    let outcome1 = submit_intent(intent1).await;
    let outcome2 = submit_intent(intent2).await;

    assert!(outcome1.success);
    assert!(outcome2.success);
    // Different actions should produce different outputs
    assert_ne!(outcome1.output, outcome2.output);
}
