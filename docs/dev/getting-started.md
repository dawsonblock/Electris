# Developer Guide: Getting Started

This guide walks through setting up a development environment for ELECTRO, building the project, and running the test suite.

## Prerequisites

| Requirement | Version | Purpose |
|-------------|---------|---------|
| Rust | 1.83+ | Language toolchain |
| Cargo | (bundled with Rust) | Build system and package manager |
| Git | 2.x+ | Version control |
| Docker | 24.x+ | Container builds (optional) |
| Chrome/Chromium | Latest | Browser automation tool (optional) |
| SQLite | 3.x | Default memory backend (usually pre-installed) |

### Installing Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# Verify
rustc --version   # Should be 1.83+
cargo --version
```

### Recommended Rust Targets

For cross-compilation and release builds:

```bash
rustup target add x86_64-unknown-linux-musl
rustup target add aarch64-unknown-linux-musl
```

### Development Tools

```bash
# Linting
rustup component add clippy

# Formatting
rustup component add rustfmt

# Faster linking (optional, significantly speeds up debug builds)
cargo install cargo-watch

# Code coverage (optional)
cargo install cargo-tarpaulin
```

## Cloning the Repository

```bash
git clone https://github.com/electro/electro.git
cd electro
```

## Building

### Debug Build

Fast compilation for development:

```bash
cargo build
```

The binary is at `target/debug/electro`.

### Release Build

Optimized with LTO, single codegen unit, and symbol stripping:

```bash
cargo build --release
```

The binary is at `target/release/electro`. Expected size: under 10 MB.

### Static Binary (musl)

For deployment as a single static binary:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

### Selective Feature Compilation

Exclude optional subsystems to speed up builds or reduce binary size:

```bash
# Minimal build: no channels except CLI, no browser, no PostgreSQL
cargo build --no-default-features

# Only Telegram support
cargo build --no-default-features --features telegram

# Everything except browser
cargo build --features "telegram,discord,slack,postgres"
```

Available feature flags:

| Feature | What it includes |
|---------|-----------------|
| `telegram` | Telegram channel (teloxide) |
| `discord` | Discord channel (serenity + poise) |
| `slack` | Slack channel (custom HTTP) |
| `browser` | Browser automation (chromiumoxide) |
| `postgres` | PostgreSQL memory backend (sqlx) |

All features are enabled by default.

## Running

### Start the Gateway

```bash
# Using cargo
cargo run -- start

# Using the built binary
./target/debug/electro start

# With a custom config
cargo run -- --config path/to/config.toml start

# In local mode with GUI
cargo run -- --mode local start --gui
```

### Interactive Chat

```bash
cargo run -- chat
```

### Check Status

```bash
cargo run -- status
```

### Validate Configuration

```bash
cargo run -- config validate
```

## Configuration for Development

Copy the default config and adjust for local development:

```bash
mkdir -p ~/.electro
cp config/default.toml ~/.electro/config.toml
```

Minimal config for local development:

```toml
[electro]
mode = "local"

[gateway]
host = "127.0.0.1"
port = 8080

[provider]
name = "anthropic"
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-6"

[memory]
backend = "sqlite"

[observability]
log_level = "debug"
```

Set your provider API key:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

## Running Tests

### Full Test Suite

```bash
cargo test --workspace
```

### Single Crate

```bash
cargo test -p electro-core
cargo test -p electro-channels
cargo test -p electro-memory
```

### With Logging Output

```bash
RUST_LOG=debug cargo test --workspace -- --nocapture
```

### Specific Test

```bash
cargo test -p electro-core -- serde_roundtrip
```

### Integration Tests

Integration tests that require external services (PostgreSQL, Chrome) are gated behind feature flags and environment variables:

```bash
# PostgreSQL integration tests
DATABASE_URL="postgres://user:pass@localhost/electro_test" cargo test -p electro-memory --features postgres

# Browser tests (requires Chrome/Chromium)
cargo test -p electro-tools --features browser
```

## Linting and Formatting

```bash
# Lint all crates
cargo clippy --workspace -- -D warnings

# Format all code
cargo fmt --all

# Check formatting without modifying
cargo fmt --all -- --check
```

## Watching for Changes

Use `cargo-watch` for automatic rebuild on file changes:

```bash
# Rebuild on change
cargo watch -x build

# Test on change
cargo watch -x "test --workspace"

# Run the gateway on change
cargo watch -x "run -- start"
```

## Docker Development

Build the Docker image locally:

```bash
docker build -t electro:dev .
```

Run with local configuration:

```bash
docker run -it --rm \
  -p 8080:8080 \
  -v ~/.electro:/var/lib/electro \
  -e ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY" \
  electro:dev
```

## Project Structure

See the [Architecture Guide](architecture.md) for a detailed explanation of the crate dependency graph and data flow.

```
electro/
  Cargo.toml                  Workspace root + binary package
  src/main.rs                 CLI entry point
  config/default.toml         Default configuration file
  crates/
    electro-core/              Traits, types, config, errors (zero business logic)
    electro-gateway/           HTTP/WS server (axum)
    electro-agent/             Agent runtime loop
    electro-providers/         AI provider implementations
    electro-channels/          Messaging channel implementations
    electro-memory/            Memory backend implementations
    electro-vault/             Secrets management
    electro-tools/             Built-in tool implementations
    electro-skills/            Skill loading and management
    electro-automation/        Heartbeat and cron
    electro-observable/        Logging, metrics, telemetry
    electro-filestore/         File storage backends
    electro-test-utils/        Shared test utilities
  docs/                        Documentation
  infrastructure/
    terraform/                 AWS Terraform configs + Fly.io config
```
