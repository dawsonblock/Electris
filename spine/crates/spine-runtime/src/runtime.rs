use spine_core::{Intent, Outcome};
use crate::dispatcher::dispatch;

pub async fn submit_intent(intent: Intent) -> Outcome {
    tracing::info!(intent_id = %intent.id, "runtime: received intent");
    let outcome = dispatch(intent).await;
    tracing::info!(success = outcome.success, "runtime: completed");
    outcome
}
