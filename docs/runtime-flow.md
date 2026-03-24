# Runtime Flow

This is the target execution spine for the corrected Electris build.

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

## Ownership

### Inputs
- CLI: `src/app/chat/mod.rs`
- HTTP: `crates/electro-gateway`
- Channels: `crates/electro-channels`

### Runtime control
- handle: `crates/electro-runtime/src/runtime_handle.rs`
- events: `crates/electro-runtime/src/events.rs`
- executor: `crates/electro-runtime/src/executor.rs`
- router: `crates/electro-runtime/src/router.rs`
- remote protocol: `crates/electro-runtime/src/remote.rs`

### Orchestration
- dispatcher: `src/app/server/dispatcher.rs`
- scheduler: `src/app/server/scheduler.rs`
- worker: `src/app/server/worker.rs`

### Execution
- agent: `crates/electro-agent`
- tools: `crates/electro-tools`
- providers: `crates/electro-providers`
- memory: `crates/electro-memory`
- vault: `crates/electro-vault`

## Invariants

- only the worker path should call the agent execution entrypoint
- adapters enqueue messages and consume events; they do not execute requests directly
- all runtime output should go through `OutboundEvent`
- all tool execution should go through the sandbox runner
