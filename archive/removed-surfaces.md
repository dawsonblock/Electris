# Removed / Parked / Deferred Surfaces

This archive does not aggressively delete large subsystems because the build was not compile-verified in this environment.

## Intended live core
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

## Intended parked systems
- electro-hive
- delegation
- proactive
- prompt patching
- unfinished orchestration variants

## Intended cuts after compile verification
- duplicate gateway surface if `crates/electro-gateway` is confirmed canonical
- duplicate startup paths
- direct request output from worker core
- direct `Command::new(...)` paths outside the sandbox layer
- dead admin or onboarding code not reachable from the live runtime path
