
<p align="center">
  <a href="https://github.com/dawsonblock/Electro/stargazers"><img src="https://img.shields.io/github/stars/dawsonblock/Electro?style=for-the-badge&color=F5A623&logo=github&logoColor=white" alt="Stars"></a>&nbsp;
  <a href="https://discord.gg/3ux2c5xz"><img src="https://img.shields.io/badge/Discord-Community-5865F2?style=for-the-badge&logo=discord&logoColor=white" alt="Discord"></a>&nbsp;
  <img src="https://img.shields.io/badge/License-MIT-A3E635?style=for-the-badge" alt="MIT">&nbsp;
  <img src="https://img.shields.io/badge/v3.2.0-Stable-06B6D4?style=for-the-badge" alt="Version">&nbsp;
  <img src="https://img.shields.io/badge/Rust-1.83.0-E34F26?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
</p>

<h1 align="center">Electro</h1>

<p align="center">
  Cloud-native AI agent runtime, written in Rust.<br>
  <sub>Deploy once. Runs forever. Thinks on a budget.</sub>
</p>

---

## What is Electro

Electro is a cloud-native AI agent runtime written in Rust. It connects to messaging channels, routes messages through an agent loop that calls AI providers, executes tools, and persists conversation history to memory backends—all from a single binary that runs headless on minimal hardware.

## Capabilities

- **Multi-channel deployment**: Telegram, Discord, Slack, CLI, or interactive TUI [IMPLEMENTED]
- **AI providers**: Anthropic, OpenAI, Google Gemini, xAI Grok, OpenRouter, and ChatGPT via OAuth [IMPLEMENTED]
- **Built-in tools**: Shell, vision browser, file ops, web fetch, git, MCP client [IMPLEMENTED]
- **λ-Memory**: Exponential decay memory with hash-based recall [IMPLEMENTED]
- **Blueprints**: Structured, replayable recipes that capture learned procedures [IMPLEMENTED]
- **Many Tems**: Stigmergic swarm intelligence for parallel task execution [EXPERIMENTAL]

## Quick Start

```bash
# Clone and build
git clone https://github.com/dawsonblock/Electro.git && cd Electro
cargo build --release
```

**Mode 1 — Interactive TUI**

```bash
./target/release/electro tui
```

**Mode 2 — Server with Telegram bot**

```bash
export TELEGRAM_BOT_TOKEN="your-token"
./target/release/electro start
```

See [docs/setup/getting-started.md](docs/setup/getting-started.md) for full setup instructions.

## Documentation

| Guide | Description |
|-------|-------------|
| [docs/setup/getting-started.md](docs/setup/getting-started.md) | Initial setup and configuration |
| [docs/setup/docker-oauth.md](docs/setup/docker-oauth.md) | Docker deployment with OAuth |
| [docs/dev/getting-started.md](docs/dev/getting-started.md) | Development environment setup |
| [docs/dev/architecture.md](docs/dev/architecture.md) | Architecture deep dive |
| [docs/dev/contributor-protocol.md](docs/dev/contributor-protocol.md) | Contributing guidelines |
| [docs/ops/configuration.md](docs/ops/configuration.md) | Configuration reference |
| [docs/ops/deployment.md](docs/ops/deployment.md) | Deployment guides |
| [docs/ops/monitoring.md](docs/ops/monitoring.md) | Monitoring and observability |
| [docs/ops/upgrade.md](docs/ops/upgrade.md) | Upgrade procedures |
| [docs/channels/](docs/channels/) | Channel setup guides (Telegram, Discord, Slack) |
| [docs/architecture/vision.md](docs/architecture/vision.md) | Project vision and roadmap |

## Workspace Layout

```
electro (binary)
├─ electro-core        — Traits, types, config, errors
├─ electro-agent       — Agentic core, λ-Memory, blueprints
├─ electro-hive        — Swarm intelligence, Many Tems
├─ electro-providers   — Anthropic, OpenAI, Gemini, Grok, OpenRouter
├─ electro-codex-oauth — ChatGPT Plus/Pro via OAuth PKCE
├─ electro-tui         — Interactive terminal UI
├─ electro-channels    — Telegram, Discord, Slack, CLI
├─ electro-memory      — SQLite, Markdown, λ-Memory
├─ electro-vault        — ChaCha20-Poly1305 secrets
├─ electro-tools       — Shell, browser, file ops, web fetch, git
├─ electro-mcp         — MCP client and runtime server manager
├─ electro-gateway     — HTTP server, health, dashboard
└─ electro-observable  — OpenTelemetry tracing
```

## Development

```bash
cargo check --workspace                              # Quick compile check
cargo test --workspace                               # Run tests
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Lint
cargo fmt --all                                      # Format
cargo build --release                                # Release binary
```

Requires **Rust 1.83.0** and Chrome/Chromium (for the browser tool).

---

<p align="center">MIT License</p>
