use spine_core::Intent;
use spine_runtime::submit_intent;

#[tokio::test]
async fn test_single_path_execution() {
    let intent = Intent::new(serde_json::json!({
        "action": "test",
        "data": "hello"
    }));

    let outcome = submit_intent(intent).await;
    
    println!("Outcome: {outcome:?}");

    assert!(outcome.success, "Expected success but got: {:?}", outcome.error);
    assert!(outcome.output.as_ref().unwrap().contains("test"));
}

#[tokio::test]
async fn test_intent_isolation() {
    // Use different actions to get different outputs
    let intent1 = Intent::new(serde_json::json!({"action": "test"}));
    let intent2 = Intent::new(serde_json::json!({"action": "analyze"}));

    let outcome1 = submit_intent(intent1).await;
    let outcome2 = submit_intent(intent2).await;
    
    println!("Outcome1: {outcome1:?}");
    println!("Outcome2: {outcome2:?}");

    assert!(outcome1.success, "Expected outcome1 success but got: {:?}", outcome1.error);
    assert!(outcome2.success, "Expected outcome2 success but got: {:?}", outcome2.error);
    // Different actions should produce different outputs
    assert_ne!(outcome1.output, outcome2.output);
}

#[tokio::test]
async fn test_file_read() {
    // Create a temp file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("spine_integration_test.txt");
    tokio::fs::write(&test_file, "hello from integration test")
        .await
        .unwrap();

    let intent = Intent::new(serde_json::json!({
        "action": "read_file",
        "path": test_file.to_str().unwrap()
    }));

    let outcome = submit_intent(intent).await;
    
    println!("File read outcome: {outcome:?}");

    assert!(outcome.success, "Expected success but got: {:?}", outcome.error);
    assert!(outcome.output.as_ref().unwrap().contains("integration test"));

    // Cleanup
    let _ = tokio::fs::remove_file(&test_file).await;
}
