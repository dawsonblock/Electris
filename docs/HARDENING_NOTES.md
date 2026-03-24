# Hardened build notes

This archive adds a fail-closed hardening pass over the highest-risk surfaces.

## Default behavior changes

- Shell commands now use an isolated container runner by default. `ELECTRO_SHELL_BACKEND=auto` tries `docker` and then `podman`.
- The shell runner mounts only the workspace at `/workspace`, uses a read-only root filesystem, drops Linux capabilities, applies resource limits, and denies network by default.
- Direct host fallback now clears the inherited environment, redirects HOME/XDG state into workspace-local `.electro-host-*` directories, and blocks launcher programs such as `sh`, `bash`, and `sudo`.
- Direct host execution now requires **both** `ELECTRO_SHELL_BACKEND=host` and `ELECTRO_ENABLE_HOST_SHELL=1`.
- Shell commands are parsed into argv and executed directly. This archive no longer relies on `sh -c` for either the isolated runner or the host fallback path.
- File tools now accept only workspace-relative paths.
- Web fetch blocks private, loopback, local, and internal targets, now including hostname resolutions that map to private IPs and redirects to blocked targets.
- Browser launches with a clean profile by default. Inheriting the live Chrome session now requires `ELECTRO_INHERIT_BROWSER_SESSION=1`.
- Custom tool loading, creation, and execution are disabled unless `ELECTRO_ENABLE_CUSTOM_TOOLS=1`.
- Telegram, Slack, and Discord no longer auto-promote the first user to admin unless `ELECTRO_ALLOW_FIRST_USER_BOOTSTRAP=1`.
- OTLP exporter health is reported as degraded because transport is not implemented.
- The orchestrator factory now fails closed instead of constructing placeholder backends that only error later.

## Shell runner configuration

Environment variables:

- `ELECTRO_SHELL_BACKEND=auto|docker|podman|host`
- `ELECTRO_SHELL_CONTAINER_IMAGE=electro-shell-runner:local`
- `ELECTRO_SHELL_ALLOW_NETWORK=0|1`
- `ELECTRO_SHELL_MEMORY_MB=256`
- `ELECTRO_SHELL_PIDS_LIMIT=128`
- `ELECTRO_SHELL_CPU_LIMIT=1.0`
- `ELECTRO_SHELL_TMPFS_MB=64`
- `ELECTRO_SHELL_PASSTHROUGH_ENV=OPENAI_API_KEY,ANTHROPIC_API_KEY` for explicit host-fallback env allowlisting after the environment is cleared
- `ELECTRO_ENABLE_HOST_SHELL=1` only when combined with `ELECTRO_SHELL_BACKEND=host`

## What is still not solved

- The isolated shell runner depends on an installed `docker` or `podman` CLI. I added the runner path, but I could not verify it in this environment.
- The default isolated shell image is `electro-shell-runner:local`. The repo includes `docker/shell-runner.Dockerfile` plus helper scripts to build and smoke-test that trusted image with `git`, `python3`, `node`, `jq`, and common dev tools.
- Browser automation can still reach public sites; it is not isolated in a separate network namespace from the rest of the process.
- Browser and web_fetch now share an optional domain allowlist via `ELECTRO_PUBLIC_WEB_ALLOWLIST`, and browser navigation now performs DNS resolution checks before and after navigation.
- Browser JavaScript evaluation is disabled by default and now requires `ELECTRO_BROWSER_ALLOW_EVAL=1`.
- Browser supports an operator-supplied proxy via `ELECTRO_BROWSER_PROXY_SERVER` and `ELECTRO_BROWSER_PROXY_BYPASS`.
- I could not run `cargo check` or the Rust test suite in this environment because `cargo` and `rustc` were not installed.

## Recommended next step

The next hardening step is to move browser and web-fetch traffic behind a real per-task network boundary, ideally a dedicated proxy or namespace-enforced egress policy instead of process-local URL checks and optional allowlists.

## Upgrade additions in this archive

- Added `scripts/build_shell_runner.sh` to build a trusted runner image.
- Added `scripts/smoke_shell_runner.sh` to verify the runner baseline without enabling network.
- Expanded the starter runner image with common developer tooling so operators do not need to fall back to host execution for basic repo work.

- Browser runtime now prefers a remote isolated Chrome instance over CDP (`ELECTRO_BROWSER_ISOLATION_MODE=remote`, `ELECTRO_BROWSER_REMOTE_URL=http://127.0.0.1:9223`).
- Added `docker-compose.browser-sandbox.yml` with a browser container on an internal Docker network and a dedicated proxy sidecar for egress.
- Local browser fallback is now explicit and fails closed unless a proxy is configured or the operator deliberately weakens the policy.
- Added `docs/BROWSER_SANDBOX.md` plus build/run/smoke scripts for the browser sandbox stack.

## Canonical Shipped Mode (P2.12)

The "ELECTRO Shipped Mode" is the high-assurance product surface optimized for production and security.

**Core Runtime (Shipped):**

- `electro-core` / `electro-agent` / `electro-providers` — The cognitive engine.
- `electro-vault` — Hardware-backed secret management.
- `electro-tools` — Verified tool stack (Shell, Browser, Git).
- `electro-channels` — Messaging integrations.
- `electro-memory` / `electro-filestore` — State and object storage.
- `electro-gateway` — SkyGate HTTP/WS surface.
- `electro-automation` — Background tasks and heartbeats.
- `electro-observable` — Telemetry and audit trails.

**Experimental / Support:**

- `electro-hive` — Stigmergic swarm intelligence (Experimental).
- `electro-tui` — Terminal-based research interface (Optional).
- `electro-mcp` — External tool bridge (Active/Evolving).

**Deprecated:**

- `electro-distill` — Eigen-Tune remnants. Removed from workspace.
