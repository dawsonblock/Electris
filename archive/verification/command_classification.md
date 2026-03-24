# Command::new Classification

Generated during final verification pass.

## Bucket 1: Live Tool Execution (SANDBOXED)

| File | Line | Context | Classification |
|------|------|---------|----------------|
| `crates/electro-tools/src/runner.rs:182` | `Command::new(&request.program)` | `run_sandboxed()` - Canonical sandbox runner | ✅ **PRIMARY SANDBOX ENTRY** |
| `crates/electro-tools/src/shell.rs:556` | `Command::new(engine)` | Container engine (docker/podman) | ✅ **ISOLATED** (containers provide isolation) |
| `crates/electro-tools/src/shell.rs:243` | `Command::new(program)` | `command_available()` - version check only | ✅ **CHECK ONLY** (no execution) |
| `crates/electro-tools/src/git.rs` | via `run_host_command` | Git commands routed through sandbox | ✅ **SANDBOXED** |

**Status**: All live tool execution flows through `runner.rs`.

## Bucket 2: Admin/Dev Utilities (NOT IN REQUEST PATH)

| File | Line | Context | Classification |
|------|------|---------|----------------|
| `src/daemon.rs:33,50,63` | `Command::new("kill")` etc. | Daemon process management | ✅ **ADMIN UTILITY** |
| `src/daemon.rs:177` | `Command::new(exe)` | Spawn daemon process | ✅ **ADMIN UTILITY** |
| `src/admin.rs:57` | `Command::new("kill")` | Admin process kill | ✅ **ADMIN UTILITY** |
| `src/reset.rs:105` | `Command::new("kill")` | Reset utility | ✅ **ADMIN UTILITY** |

**Status**: These are operational utilities, not part of request-handling runtime.

## Bucket 3: Transport/Process Helpers

| File | Line | Context | Classification |
|------|------|---------|----------------|
| `crates/electro-codex-oauth/src/lib.rs:288,295,302` | `Command::new("open")` etc. | Open browser for OAuth | ✅ **USER INTERACTION** (not automated) |
| `crates/electro-mcp/src/transport/stdio.rs:60` | `Command::new(command)` | MCP server transport | ✅ **TRANSPORT LAYER** |
| `crates/electro-tui/src/lib.rs:76` | `Command::new("stty")` | Terminal setup for TUI | ✅ **SETUP ONLY** (not request handling) |

## Bucket 4: Browser Administrative

| File | Line | Context | Classification |
|------|------|---------|----------------|
| `crates/electro-tools/src/browser.rs:1648,1659,1679` | `Command::new("pgrep")` etc. | Browser process cleanup | ✅ **CLEANUP ONLY** (administrative) |

## Summary

- **Live request path**: All tool execution goes through `runner.rs`
- **Container path**: Uses docker/podman (provides isolation)
- **Admin utilities**: Separated from runtime
- **No direct host execution** in live request path

**VERDICT**: ✅ SANDBOX BOUNDARY ENFORCED
