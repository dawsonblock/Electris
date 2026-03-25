use spine_core::{Intent, Outcome};
use spine_worker::execute_command;
use crate::planner::plan;

pub(crate) async fn dispatch(intent: Intent) -> Outcome {
    let command = plan(intent).await;
    execute_command(command).await
}
