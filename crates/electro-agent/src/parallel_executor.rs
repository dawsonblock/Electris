//! ParallelExecutor — runs independent tool calls concurrently using tokio::task::JoinSet.
//!
//! This replaces the sequential tool-calling loop when a plan allows for
//! parallelism (e.g. searching multiple domains or downloading multiple files).

use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use electro_core::types::error::ElectroError;
use electro_core::{Tool, ToolContext, ToolInput, ToolOutput};

/// Execute a batch of tool calls in parallel.
///
/// Each tool call runs in its own task. Results are returned in a Rayon-style
/// unordered Vec, or sorted by their original indices if order is required.
pub async fn execute_parallel(
    tools: &[Arc<dyn Tool>],
    calls: Vec<(String, ToolInput)>,
    ctx: &ToolContext,
) -> Vec<Result<ToolOutput, ElectroError>> {
    let mut set = JoinSet::new();
    let ctx_owned = ctx.clone();
    let ctx_arc = Arc::new(ctx_owned);
    let calls_count = calls.len();

    info!(count = calls_count, "Starting parallel execution of tool calls");

    for (idx, (name, input)) in calls.into_iter().enumerate() {
        let tool = tools.iter().find(|t| t.name() == name).cloned();
        let ctx = ctx_arc.clone();
        let name_for_err = name.clone();

        set.spawn(async move {
            let result = if let Some(t) = tool {
                debug!(tool = %name, index = idx, "Executing tool in parallel");
                t.execute(input, &ctx).await
            } else {
                Err(ElectroError::Tool(format!("Tool not found: {name_for_err}")))
            };
            (idx, result)
        });
    }

    let mut results: Vec<Option<Result<ToolOutput, ElectroError>>> = (0..calls_count).map(|_| None).collect();

    while let Some(res) = set.join_next().await {
        match res {
            Ok((idx, output)) => {
                results[idx] = Some(output);
            }
            Err(e) => {
                warn!(error = %e, "Parallel tool task panicked or was cancelled");
            }
        }
    }

    debug!(count = calls_count, "Parallel execution completed");
    results.into_iter().flatten().collect()
}
