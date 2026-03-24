# Electris Ownership

This document explicitly assigns ownership of critical runtime concerns.

## Execution Authority

### Normal Runtime
- **Owner**: `src/app/server/worker.rs`
- **Invariant**: Only this file executes the agent for normal runtime requests
- **Entry Point**: `create_chat_worker()` spawns the worker task
- **Responsibilities**:
  - Acquire execution permit
  - Emit `Started` event
  - Select local vs remote execution
  - Run with timeout
  - Emit `ToolCall`, `ToolResult`, `Completed`/`Failed` events
  - Persist history
  - Release permit

### Remote Execution
- **Owner**: `src/bin/worker-node.rs`
- **Invariant**: Only this binary executes the agent for remote requests
- **Entry Point**: `/execute` HTTP handler
- **Responsibilities**:
  - Authenticate requests
  - Execute agent
  - Return structured success/failure response

### Prohibited
The following must NOT execute the agent directly:
- Gateway routes (must enqueue only)
- TUI/CLI adapters (must enqueue only)
- Channel adapters (must enqueue only)
- Command handlers (must emit events only)

## HTTP Surface

- **Owner**: `crates/electro-gateway`
- **Invariant**: Only this crate owns the HTTP service surface
- **Routes**:
  - `GET /health/live` - Liveness probe
  - `GET /health/ready` - Readiness probe (returns 503 if degraded)
  - `POST /message` - Enqueue message (returns request_id)
  - `GET /stream` - SSE stream of `OutboundEvent`
  - `POST /execute` - Remote worker execution endpoint

### Prohibited
No other crate may define HTTP routes that:
- Duplicate gateway functionality
- Execute the agent directly
- Bypass the queue/dispatcher/worker path

## Output Contract

- **Owner**: `crates/electro-runtime::events::OutboundEvent`
- **Invariant**: This is the exclusive runtime output contract
- **Events**:
  - `Started { request_id }`
  - `Token { request_id, content }`
  - `ToolCall { request_id, tool }`
  - `ToolResult { request_id, tool, success, content }`
  - `Completed { request_id, content }`
  - `Failed { request_id, error }`

### Responsibilities by Component

**Worker** (emits):
- All execution events
- No direct output formatting

**Adapters** (consume and render):
- Gateway: SSE stream
- CLI: Print to stdout
- TUI: Render to UI
- Channels: Send platform messages

### Prohibited
- Worker must not format platform-specific responses
- Dispatcher must not own final output delivery
- Command handlers must not bypass event flow

## Sandbox Boundary

- **Owner**: `crates/electro-tools`
- **Policy**: `crates/electro-tools/src/policy.rs`
- **Runner**: `crates/electro-tools/src/runner.rs`
- **Invariant**: All live tool execution must follow: `policy → validation → sandbox runner → output cap → audit event`

### Canonical Chain

1. **Policy Check**: `CapabilityPolicy` evaluation
2. **Validation**: Command safety, path traversal checks
3. **Sandbox Runner**: `run_sandboxed()` with constraints
4. **Output Cap**: Truncation at 64KB default
5. **Audit Event**: Execution logging

### Tool Classes

**Shell**:
- Route through: `run_shell_command()`
- Sandbox-only (no direct host execution)
- Strict timeout
- No network by default

**Git**:
- Route through: `run_git_command()`
- Restricted to approved workspace roots
- No arbitrary path traversal

**Browser**:
- Isolated process model
- Explicit policy for no-sandbox overrides

### Prohibited
- Direct `Command::new()` in request-handling paths
- Execution outside the canonical chain

## Runtime State

- **Owner**: `crates/electro-runtime`
- **Key Types**:
  - `RuntimeHandle` - Runtime interaction
  - `OutboundEvent` - Output contract
  - `ExecutionController` - Concurrency control
  - `ExecutionRouter` - Local vs remote routing

## Memory & Persistence

- **Owner**: `crates/electro-memory`
- **Backends**: SQLite (default), PostgreSQL (optional)
- **Responsibilities**:
  - Conversation history
  - Lambda memory
  - Configuration storage

## Providers

- **Owner**: `crates/electro-providers`
- **Supported**: Anthropic, OpenAI, Gemini, Grok, OpenRouter
- **Responsibilities**:
  - API client implementation
  - Authentication
  - Rate limiting
  - Circuit breaking

## Verification

To verify ownership invariants:

```bash
# Execution authority
grep -r 'process_message(' src crates --include='*.rs' | grep -v test | grep -v 'runtime.rs'
# Should only show: worker.rs, worker-node.rs, agent_bridge.rs (worker task)

# HTTP surface
grep -r 'health/live\|health/ready\|/message\|/stream' crates/electro-gateway/src --include='*.rs'
# Should only show in: electro-gateway

# Command::new in live paths
grep -r 'Command::new' crates/electro-tools/src/shell.rs crates/electro-tools/src/git.rs
# Should be routed through runner.rs
```
