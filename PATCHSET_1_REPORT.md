# Patchset 1 report

Applied the truth/defaults/toolchain remediation pass.

## Files changed
- `Cargo.toml`
- `README.md`
- `docs/setup/getting-started.md`
- `docs/setup/docker-oauth.md`
- `src/onboarding.rs`
- `crates/electro-mcp/src/mcp_manage.rs`
- `crates/electro-mcp/src/config.rs`
- `crates/electro-mcp/src/manager.rs`
- `crates/electro-tools/src/shell.rs`
- `docs/SHELL_RUNNER.md`
- `docs/HARDENING_NOTES.md`
- `crates/electro-channels/src/lib.rs`
- `docs/dev/getting-started.md`
- `docs/api/config.md`
- `docs/api/traits.md`
- `docs/dev/adding-channel.md`
- `docs/dev/architecture.md`
- `docs/ops/configuration.md`
- `Dockerfile`
- `CLAUDE.md`
- `config/default.toml`
- `rust-toolchain.toml`
- `docs/STATUS.md`

## Notes
- CI workflow files already targeted Rust 1.83, so they were left unchanged.
- MCP onboarding was downgraded from autonomous discovery/install claims to configured-server management only.
- WhatsApp references were removed from active docs and replaced with an explicit runtime error in channel creation.
- Shell default image now points at `electro-shell-runner:local`.
- Build/test execution was not run here because Cargo is unavailable in this environment.
