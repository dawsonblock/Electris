# Migration Guide: SkyClaw → ELECTRO

> **SkyClaw is now ELECTRO.** This change is permanent.
> This guide walks existing SkyClaw users through upgrading.

---

## Quick migration (most users)

If you're running SkyClaw locally with default paths, here's all you need:

### Step 1: Move your data directory

```bash
# Your data lives in ~/.skyclaw — move it to ~/.electro
mv ~/.skyclaw ~/.electro
```

This moves everything at once: your database, vault, credentials, custom tools, and skills.

### Step 2: Rename your config file

```bash
# In your project/workspace directory
mv skyclaw.toml electro.toml
```

Then edit `electro.toml` and rename the top-level section:

```toml
# Old
[skyclaw]
mode = "local"

# New
[electro]
mode = "local"
```

Everything else in the config stays the same — field names, values, env var references.

### Step 3: Rebuild the binary

```bash
cargo build --release --bin electro
```

### Step 4: Run it

```bash
# Old
skyclaw start
skyclaw chat

# New
electro start
electro chat
electro start --personality play   # NEW: PLAY mode
electro start --personality work   # NEW: WORK mode
electro start --personality pro    # NEW: PRO mode (professional, no emoticons)
```

That's it. Your conversation history, vault secrets, credentials, custom tools, and skills all carry over — the data formats haven't changed.

---

## Detailed file mapping

If you customized paths or need to verify everything moved correctly:

### Data directory (`~/.skyclaw` → `~/.electro`)

| Old path                           | New path                          | What it is                        |
|------------------------------------|-----------------------------------|-----------------------------------|
| `~/.skyclaw/memory.db`             | `~/.electro/memory.db`             | SQLite conversation history       |
| `~/.skyclaw/vault.enc`             | `~/.electro/vault.enc`             | Encrypted secrets (ChaCha20)      |
| `~/.skyclaw/vault.key`             | `~/.electro/vault.key`             | 32-byte encryption key            |
| `~/.skyclaw/credentials.toml`      | `~/.electro/credentials.toml`      | Provider API keys                 |
| `~/.skyclaw/config.toml`           | `~/.electro/config.toml`           | User config (if not using workspace toml) |
| `~/.skyclaw/agent-config.toml`     | `~/.electro/agent-config.toml`     | Agent config overrides            |
| `~/.skyclaw/allowlist.toml`        | `~/.electro/allowlist.toml`        | Admin user allowlist              |
| `~/.skyclaw/custom-tools/`         | `~/.electro/custom-tools/`         | User/agent-created script tools   |
| `~/.skyclaw/skills/`               | `~/.electro/skills/`               | Custom Markdown skills            |
| `~/.skyclaw/memory/`               | `~/.electro/memory/`               | Markdown memory files (if using Markdown backend) |
| `~/.skyclaw/workspace/`            | `~/.electro/workspace/`            | Heartbeat/scheduled task workspace |

### Files you can ignore (auto-regenerated)

- `~/.skyclaw/skyclaw.pid` → `~/.electro/electro.pid` (recreated on start)
- `~/.skyclaw/skyclaw.log` → `~/.electro/electro.log` (recreated on start)

### Critical files — do NOT lose these

| File                | Why                                                       |
|---------------------|-----------------------------------------------------------|
| `vault.key`         | Without it, your `vault.enc` secrets are unrecoverable    |
| `vault.enc`         | Your encrypted secrets — paired with `vault.key`          |
| `credentials.toml`  | Your provider API keys                                    |
| `memory.db`         | Your full conversation history                            |

---

## Docker users

### Volume mounts

```yaml
# Old
volumes:
  - skyclaw-data:/var/lib/skyclaw
  - ${HOME}/.skyclaw:/home/skyclaw/.skyclaw

# New
volumes:
  - electro-data:/var/lib/electro
  - ${HOME}/.electro:/home/electro/.electro
```

### Container paths

| Old                    | New                    |
|------------------------|------------------------|
| `/app/skyclaw`         | `/app/electro`          |
| `/var/lib/skyclaw`     | `/var/lib/electro`      |
| `/home/skyclaw/.skyclaw` | `/home/electro/.electro` |

If using named volumes, create the new volume and copy data:

```bash
docker volume create electro-data
docker run --rm \
  -v skyclaw-data:/from \
  -v electro-data:/to \
  alpine sh -c "cp -a /from/. /to/"
```

---

## Systemd users

### Service file

```bash
# Old: /etc/systemd/system/skyclaw.service
# New: /etc/systemd/system/electro.service

sudo systemctl stop skyclaw
sudo systemctl disable skyclaw

# Update paths in the service file:
#   ExecStart=/usr/local/bin/electro start
#   WorkingDirectory=/var/lib/electro
#   EnvironmentFile=/etc/electro/env
#   User=electro
#   Group=electro

sudo systemctl daemon-reload
sudo systemctl enable electro
sudo systemctl start electro
```

### System config

```bash
sudo mv /etc/skyclaw /etc/electro
sudo mv /var/lib/skyclaw /var/lib/electro
```

---

## Library/crate consumers

If you depend on skyclaw as a Rust library:

### Cargo.toml

```toml
# Old
[dependencies]
skyclaw-core = { git = "https://github.com/nagisanzenin/skyclaw" }
skyclaw-agent = { git = "https://github.com/nagisanzenin/skyclaw" }

# New
[dependencies]
electro-core = { git = "https://github.com/nagisanzenin/electro" }
electro-agent = { git = "https://github.com/nagisanzenin/electro" }
```

### Rust imports

```rust
// Old
use skyclaw_core::traits::Provider;
use skyclaw_core::types::error::SkyclawError;
use skyclaw_agent::runtime::AgentRuntime;

// New
use electro_core::traits::Provider;
use electro_core::types::error::ElectroError;
use electro_agent::runtime::AgentRuntime;
```

### Error type

`SkyclawError` → `ElectroError`. All variants remain the same.

---

## Config file reference

The config schema is unchanged except for the top-level section name. Config is searched in this order:

1. `/etc/electro/config.toml` (system)
2. `~/.electro/config.toml` (user)
3. `./config.toml` (workspace)
4. `./electro.toml` (workspace)

Environment variables still use `${VAR}` expansion syntax. No env var prefixes changed — `ANTHROPIC_API_KEY`, `TELEGRAM_BOT_TOKEN`, etc. are all the same.

---

## What's new in ELECTRO (beyond the rename)

The rebrand shipped with new features:

- **PLAY/WORK/PRO personality modes** — `electro start --personality play|work|pro`
- **`mode_switch` agent tool** — Tem can switch modes at runtime (play, work, pro)
- **Soul-injected system prompts** — Tem's character is baked into every LLM call
- **Vision browser** (v2.6.0) — screenshot + visual understanding tools

---

## Troubleshooting

**"config file not found"** — Make sure you renamed `skyclaw.toml` → `electro.toml` and the `[skyclaw]` section to `[electro]`.

**"database not found"** — Check that `~/.electro/memory.db` exists. If you forgot to move the data dir: `mv ~/.skyclaw ~/.electro`

**"vault key error"** — The `vault.key` file must be exactly 32 bytes with 0600 permissions. Verify: `ls -la ~/.electro/vault.key && wc -c ~/.electro/vault.key`

**"permission denied on vault.key"** — Fix permissions: `chmod 600 ~/.electro/vault.key`

**Custom tools missing** — Check that `~/.electro/custom-tools/` has your `.json` + script files.

---

## Is this permanent?

**Yes.** ELECTRO has its own identity — soul document, design brief, pixel art, voice guardrails, and legal distinction from existing IP. This is not a rename that will change again. SkyClaw is retired.
