# Electris Complete Fix Plan

This archive is a corrected salvage snapshot, not a verified release.

## Target runtime spine

```text
CLI / HTTP / Slack / Discord / Telegram
→ RuntimeMessage
→ dispatcher
→ scheduler
→ worker
→ electro-agent
→ tools / providers / memory
→ OutboundEvent
→ adapter render / SSE / channel reply
```

## Non-negotiable rules

- only `src/app/server/worker.rs` should call the agent execution path
- only `crates/electro-gateway` should own the HTTP service surface
- all runtime output should flow through `OutboundEvent`
- all tool execution should flow through the sandbox runner
- advanced systems stay out of the live runtime until the core loop is proven

## Highest-priority work

1. run `cargo check` on a machine with Rust installed and fix compile errors
2. confirm there are no remaining raw merge-conflict markers in source files
3. collapse to one startup path and one gateway
4. audit direct `process_message(...)` calls outside the worker path
5. audit direct `Command::new(...)` tool execution outside the sandbox layer
6. burn down warnings in the live core only
7. add or repair the small end-to-end test set:
   - gateway roundtrip
   - CLI roundtrip
   - stop/cancel
   - sandbox enforcement
   - remote worker execution

## Keep / park / cut

### Keep live
- electro-agent
- electro-runtime
- electro-gateway
- electro-tools
- electro-providers
- electro-memory
- electro-vault
- electro-channels
- electro-mcp
- electro-tui
- src/app/server/{mod,dispatcher,scheduler,worker}.rs
- src/app/chat/mod.rs
- src/app/cli.rs
- src/bin/worker-node.rs

### Park until core is stable
- electro-hive
- delegation
- proactive
- prompt patching
- incomplete orchestration variants

### Cut from active runtime
- duplicate gateway implementations
- alternate startup paths
- direct output from worker core
- direct tool execution outside sandbox
- dead admin/onboarding branches that are not on the live path

## Acceptance gates

- `cargo check` succeeds
- `cargo test` passes for the core runtime
- `cargo clippy --all-targets --all-features` is reduced to a low, explainable warning count in the live core
- output is event-driven
- gateway starts unconditionally and exposes readiness separately
- host shell execution only exists in the sandbox layer
