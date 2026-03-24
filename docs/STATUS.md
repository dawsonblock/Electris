# Electris Status

Strict status table reflecting verified repo state.

## Capability Status

| Capability | Status | Notes |
|------------|--------|-------|
| Core runtime | ✅ Active | Event-driven execution, worker pool, dispatcher, scheduler |
| Gateway | ✅ Active | HTTP endpoints, health checks, SSE streaming, queue ingestion |
| Event streaming | ✅ Active | OutboundEvent system, subscription-based |
| Remote worker | ✅ Active | Authenticated remote execution via worker-node |
| Tool sandbox | 🟡 Partial | Policy engine complete, runner created, tool migration in progress |
| TUI | ✅ Active | Pure adapter pattern (enqueue + event subscription) |
| CLI | ✅ Active | Command-line interface with queue integration |
| Telegram | ✅ Active | Channel adapter with full integration |
| Discord | ✅ Active | Channel adapter with tests |
| Slack | ✅ Active | Channel adapter with tests |
| MCP | ✅ Active | Model Context Protocol client/server |
| Browser | ✅ Active | Browser automation (Rust 1.88+ via feature flag) |
| Hive | ⚠️ Experimental | Multi-agent swarm (feature-gated, not in default build) |
| Automation | ⚠️ Experimental | Proactive automation (feature-gated) |
| Skills | ⚠️ Experimental | Skill system (feature-gated, partial) |
| Filestore | ⚠️ Experimental | Object storage (feature-gated) |
| Codex OAuth | ⚠️ Experimental | ChatGPT Plus/Pro via OAuth PKCE (feature-gated) |

## Build Status

| Check | Status |
|-------|--------|
| `cargo check` (default) | ✅ Pass |
| `cargo check --no-default-features` | ✅ Pass |
| `cargo check --all-features` | ✅ Pass |
| `cargo test --workspace` | ✅ Pass (1500+ tests) |
| `cargo clippy` (core) | 🟡 Clean with warnings in experimental |

## Feature Flags

### Default Features
```
telegram, mcp, tui
```

### Experimental Features (require explicit opt-in)
```
hive, skills, automation, filestore, codex-oauth
```

### Build Examples
```bash
# Core only
cargo build --no-default-features --features "telegram"

# Default (operational)
cargo build

# With experimental features
cargo build --features "hive,skills,automation,filestore,codex-oauth"
```

## Execution Authority Verification

| File | Role |
|------|------|
| `src/app/server/worker.rs` | ✅ Authorized normal execution |
| `src/bin/worker-node.rs` | ✅ Authorized remote execution |
| `crates/electro-tui/src/agent_bridge.rs` | ✅ Worker task (authorized) |
| Gateway routes | ✅ Enqueue only |
| TUI adapter | ✅ Enqueue only |

## Sandbox Closure Status

| Component | Status | Notes |
|-----------|--------|-------|
| Policy engine | ✅ Complete | `CapabilityPolicy`, `PolicyEngine` |
| Sandbox runner | ✅ Complete | `run_sandboxed()`, constraints, output cap |
| Shell tool | 🟡 Migration pending | Use `run_shell_command()` |
| Git tool | 🟡 Migration pending | Use `run_git_command()` |
| Browser tool | ✅ Acceptable | Process isolation, cleanup only |
| Admin utilities | ✅ Classified | Outside request path (`daemon.rs`, `admin.rs`, `reset.rs`) |

## Output Unification Status

| Component | Output Method |
|-----------|---------------|
| Worker | ✅ Events only (`OutboundEvent`) |
| Dispatcher | ✅ Events only |
| Commands | ✅ Events only |
| Gateway | ✅ SSE stream |
| TUI | ✅ Event subscription |
| CLI | ✅ Event subscription |

## Known Limitations

1. **Tool Sandbox**: Shell and git tools still use direct `Command::new`. Migration to `runner.rs` APIs is in progress.

2. **Experimental Modules**: Hive, automation, skills, filestore, and codex-oauth are feature-gated and not part of the core stabilization set.

3. **Clippy Warnings**: Core modules pass; experimental modules have warnings.

4. **Documentation**: API docs are partial; refer to source for details.

## Operational Definition

This branch is considered operational when:

- [x] `cargo check` passes
- [x] `cargo test` passes
- [x] Service starts from one command (`cargo run --bin electro`)
- [x] Gateway is always up (independent of agent)
- [x] `/health/live` returns 200
- [x] `/health/ready` reflects actual readiness (200 ready, 503 degraded)
- [x] `POST /message` returns request_id
- [x] `GET /stream` emits `OutboundEvent`s
- [x] TUI input goes through queue → dispatcher → scheduler → worker
- [x] Only worker and worker-node execute the agent
- [ ] All live tool execution is sandboxed (partial - runner ready, migration pending)
- [x] README/status docs are truthful

## Changelog

### 2024-03-24
- Fixed TUI execution bypass (now pure adapter)
- Unified output model (events only)
- Created sandbox runner infrastructure
- Reduced default workspace (experimental feature-gated)
- Updated documentation truthfulness
- Fixed all test failures
