# Hardened build notes

This archive adds a fail-closed hardening pass over the highest-risk surfaces.

## ✅ Completed Hardening (Implemented)

- **Isolated Shell Runner**: Shell commands use an isolated container runner by default. `ELECTRO_SHELL_BACKEND=auto` tries `docker` and then `podman`.
- **Sandbox Security**: The shell runner mounts only the workspace at `/workspace`, uses a read-only root filesystem, drops Linux capabilities, applies resource limits, and denies network by default.
- **Host Fallback Safety**: Direct host fallback clears the inherited environment, redirects HOME/XDG state into workspace-local `.electro-host-*` directories, and blocks launcher programs (`sh`, `bash`, `sudo`). Requires **both** `ELECTRO_SHELL_BACKEND=host` and `ELECTRO_ENABLE_HOST_SHELL=1`.
- **Path Validation**: File tools accept only workspace-relative paths.
- **Web Safety**: Web fetch blocks private, loopback, local, and internal targets, including hostname resolutions that map to private IPs.
- **Browser Isolation**: Launches with a clean profile by default (`ELECTRO_INHERIT_BROWSER_SESSION=0`). Prefers remote isolated Chrome via CDP.
- **Custom Tool Safety**: Disabled unless `ELECTRO_ENABLE_CUSTOM_TOOLS=1`.
- **Fail-Closed Design**: The orchestrator factory fails closed instead of constructing placeholder backends.

## 🟢 Verified Results (2026-03-23)

- **Toolchain Alignment**: Verified 100% synchronization on **Rust 1.83.0** across `rust-toolchain.toml`, Cargo manifests, GitHub workflows, and Dockerfiles.
- **Build Integrity**: Full Rust test suite (`cargo test --workspace`) and clippy checks pass with zero warnings on the 1.83.0 baseline.
- **Shell Runner**: `scripts/smoke_shell_runner.sh` confirms the hardened baseline.
- **Browser Security**: DNS resolution checks successfully prevent SSRF and private network escapes during navigation.

## 🛠 Planned Hardening (Upcoming)

- **Network Egress**: Move browser and web-fetch traffic behind a dedicated proxy or namespace-enforced egress policy instead of process-local URL checks.
- **Hardware Vaults**: Expand `electro-vault` support for hardware-backed keys (TPM/HSM).
- **Audit Trails**: Implement full cryptographic signatures for all tool execution logs.
