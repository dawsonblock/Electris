# Getting Started with ELECTRO

This guide walks you through setting up ELECTRO, a cloud-native Rust AI agent runtime. Whether you're a new user or an experienced developer, you'll find what you need here.

## Prerequisites

Before you begin, ensure you have the following:

- **Rust 1.83+** — Install via [rustup](https://rustup.rs):
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  source $HOME/.cargo/env
  ```

- **Chrome/Chromium** (optional) — Required only if you want to use the browser tool. Install via your system's package manager or from [google.com/chrome](https://www.google.com/chrome).

- **Telegram bot token** — Create one by messaging [@BotFather](https://t.me/BotFather) on Telegram:
  1. Send `/newbot` to BotFather
  2. Choose a name and username (must end in `bot`)
  3. Copy the token BotFather provides

  > Keep your token secure — anyone with it can control your bot.

## Build

Clone the repository and build the binary:

```bash
git clone https://github.com/dawsonblock/Electro.git
cd electro
cargo build --release
```

The first build takes ~2-4 minutes. Subsequent builds are much faster. The binary is located at `./target/release/electro`.

### Feature Flags

Enable optional features at build time:

| Feature | Description |
|---------|-------------|
| `tui` | Interactive terminal UI |
| `telegram` | Telegram channel support (enabled by default) |
| `discord` | Discord channel support |
| `slack` | Slack channel support |
| `browser` | Browser automation tool |

Example:
```bash
cargo build --release --features tui
```

## Authentication

ELECTRO supports two authentication methods. Choose one.

### Codex OAuth (ChatGPT Plus/Pro)

If you have a ChatGPT Plus or Pro subscription, use the built-in OAuth flow:

```bash
./target/release/electro auth login
```

A browser window opens for you to log into your ChatGPT account. Tokens last ~10 days and are stored at `~/.electro/oauth.json`. They're auto-detected at startup.

For headless environments:
```bash
./target/release/electro auth login --headless
```

This prints a URL you can open on any device, then paste the redirect URL back.

Export tokens for container deployments:
```bash
./target/release/electro auth login --output ./oauth.json
```

Check token status:
```bash
./target/release/electro auth status
```

### API Key

Alternatively, provide an API key directly. ELECTRO auto-detects the provider based on the key prefix:

| Prefix | Provider |
|--------|----------|
| `sk-ant-` | Anthropic (Claude) |
| `sk-` | OpenAI |
| `AIzaSy` | Google Gemini |
| `xai-` | xAI Grok |
| `sk-or-` | OpenRouter |

You can set the API key via the Telegram bot after starting, or configure it in your config file. ELECTRO also supports a secure OTK setup link with AES-256-GCM encryption.

### Auto-Detection

At startup, ELECTRO checks for credentials in this order:
1. Codex OAuth token (`~/.electro/oauth.json`)
2. API keys from environment variables
3. Keys configured in `electro.toml`

## First Run

1. **Set your Telegram token:**
   ```bash
   export TELEGRAM_BOT_TOKEN="your-bot-token-here"
   ```

2. **Start the bot:**
   ```bash
   ./target/release/electro start
   ```

   For background operation:
   ```bash
   ./target/release/electro start -d
   ```
   Logs are written to `~/.electro/electro.log`.

3. **If using an API key:** Configure your key using ELECTRO's secure setup flow (for example, an encrypted one-time setup link or local configuration file). Avoid pasting API keys directly into Telegram chats, since chat history and logs can retain secrets.

4. **Test it out:** Send a message to your bot. Try:
   - `Hello!` — basic chat
   - `What files are in my home directory?` — shell tool
   - `/model` — see available models

### Stopping the Bot

```bash
./target/release/electro stop
```

## Configuration

By default, ELECTRO looks for config in:
1. `./electro.toml` (project root)
2. `~/.electro/electro.toml`

### Basic Configuration

```toml
[provider]
name = "anthropic"
api_key = "${ANTHROPIC_API_KEY}"
model = "claude-sonnet-4-6"

[agent]
max_spend_usd = 5.0

[channel.telegram]
enabled = true
token = "${TELEGRAM_BOT_TOKEN}"
allowlist = []

[memory]
backend = "sqlite"

[security]
sandbox = "mandatory"
```

Environment variables expand via `${VAR}` syntax. For the full configuration schema, see [Configuration Reference](../ops/configuration.md).

### Key Paths

| Path | Purpose |
|------|---------|
| `~/.electro/` | Home directory for all state |
| `~/.electro/credentials.toml` | Provider API keys |
| `~/.electro/oauth.json` | Codex OAuth tokens |
| `~/.electro/memory.db` | SQLite memory |
| `~/.electro/electro.log` | Daemon logs |

## Docker

For containerized deployments, see the detailed [Docker OAuth Setup](docker-oauth.md) guide. It covers:

- Codex OAuth token export for containers
- docker-compose configuration
- Environment variable handling

## Next Steps

Now that ELECTRO is running, explore these guides:

- **[Discord Channel](../channels/discord.md)** — Add Discord support
- **[Slack Channel](../channels/slack.md)** — Add Slack support
- **[CLI Channel](../channels/cli.md)** — Run in terminal mode
- **[Deployment Guide](../ops/deployment.md)** — VPS deployment with systemd
- **[Configuration Reference](../ops/configuration.md)** — Full config schema

### Switching Providers

Use `/model` to switch between providers mid-conversation:
```
/model                 # list available models
/model gpt-5.4        # switch to GPT-5.4
/model claude-sonnet-4-6  # switch to Claude
```

Or just say "Switch to GPT-5.2" — natural language works too.

### MCP Servers

Configure or connect MCP servers at runtime:
```
/mcp add fetch npx -y @modelcontextprotocol/server-fetch
/mcp add github npx -y @modelcontextprotocol/server-github
/mcp    # list connected servers
```

Configured MCP servers can extend ELECTRO at runtime. This archive does not ship an autonomous built-in MCP server registry.

## Updating

```bash
./target/release/electro update
```

This pulls the latest code and rebuilds. Or manually:
```bash
git pull && cargo build --release
```

## Compilation Gates

Before deploying, ensure all checks pass:

```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo test --workspace
```
