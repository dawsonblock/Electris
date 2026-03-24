# Electris Architecture Ownership

**Single Source of Truth for Component Ownership**

## Execution Authority

**Owner:** `src/app/server/worker.rs` and `src/bin/worker-node.rs`

These are the ONLY files that may call `agent.process_message()` in the live runtime.

- `worker.rs` - Local worker pool execution
- `worker-node.rs` - Remote worker node execution

**NOT authorized to execute directly:**
- `electro-gateway` - Must only enqueue
- `electro-tui` - Currently bypasses (tech debt)
- Any channel adapters - Must use queue

## HTTP Surface

**Owner:** `crates/electro-gateway`

The gateway crate owns all HTTP endpoints:
- `/health/live` - Liveness probe
- `/health/ready` - Readiness probe  
- `/message` - Request enqueue
- `/stream` - Event stream (SSE)

**NOT authorized:**
- `src/app/server/gateway.rs` - Deleted (was duplicate)
- Any other server modules

## Output Contract

**Owner:** `electro-runtime::OutboundEvent`

All output goes through the event system:
- `Started` - Request started
- `ToolCall` - Tool invoked
- `ToolResult` - Tool completed
- `Completed` - Request completed
- `Failed` - Request failed

Adapters subscribe to events and render platform-specific output.

**NOT authorized:**
- Direct `sender.send_message()` calls from worker (removed)
- Direct `println!` in core (removed)

## Tool Policy

**Owner:** `crates/electro-tools`

Tool execution policy enforced by:
- `policy.rs` - Policy definitions
- `shell.rs` - Shell runner with backend selection (host/docker/podman)
- `tool.rs` - Tool trait implementations

**Backends:**
- Host - With validation, env isolation (development)
- Docker/Podman - Full sandbox (production recommended)

## Runtime State

**Owner:** `electro-runtime`

Runtime state managed by:
- `RuntimeHandle` - Shared runtime reference
- `ExecutionRouter` - Local vs remote routing
- `OutboundEvent` bus - Event broadcasting

## Crate Classification

### Core Stabilization (Required)
- `electro-core` - Types, policies
- `electro-runtime` - Execution controller
- `electro-agent` - Agent runtime
- `electro-gateway` - HTTP surface
- `electro-tools` - Tool registry
- `electro-providers` - LLM providers
- `electro-memory` - Persistence
- `electro-vault` - Secrets
- `electro-channels` - Channel adapters
- `electro-mcp` - MCP support
- `electro-tui` - CLI (despite bypass issue)
- `electro-observable` - Metrics

### Optional/Experimental (Feature-gated)
- `electro-skills` - Skill system
- `electro-automation` - Proactive automation
- `electro-filestore` - Object storage
- `electro-codex-oauth` - OAuth
- `electro-hive` - Multi-agent swarm

## Change Control

When modifying architecture:
1. Update this doc if ownership changes
2. Update `docs/status.md` if capability status changes
3. Run `scripts/check_core_paths.sh` to verify
