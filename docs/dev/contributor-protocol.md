# Contributor Protocol

The single source of truth for building ELECTRO. Every contributor — human or AI — follows this protocol.

---

## Principles

### Traits in Core, Implementations in Crates

All shared abstractions live in `electro-core/src/traits/`. Implementations live in their respective crates. A channel implementation never imports a provider. A tool never imports a memory backend. If two crates need the same type, it belongs in `electro-core/src/types/`.

### No Cross-Implementation Dependencies

Leaf crates (providers, channels, tools, memory backends) must never depend on each other. The dependency graph is a star: core at the center, everything else at the edges. Violations create coupling that makes the system impossible to extend.

### Feature Flags for Optional Dependencies

Platform-specific SDKs (teloxide, serenity, chromiumoxide) are behind Cargo feature flags. Never `use teloxide::*` unconditionally. The binary must compile with zero optional features enabled.

### Factory Dispatch by Name String

Each crate exposes a `create_*()` function that takes a config and returns `Box<dyn Trait>`. The gateway and main.rs never construct implementations directly — they call the factory with a name string from config.

### Every Error is a ElectroError

No `unwrap()` in production code. No `Box<dyn Error>`. Every fallible operation returns `Result<T, ElectroError>` using the appropriate variant. The caller always knows what domain the error came from.

### Security is Structural, Not Optional

Empty allowlists deny everyone. Numeric IDs only — never usernames. Path traversal protection on every file operation. API keys redacted in Debug output. Vault key files are 0600. These rules are not suggestions — they are enforced by the code.

---

## Architecture

ELECTRO is a Cargo workspace with 18 crates plus a root binary.

```
electro (binary)                    src/main.rs — CLI, onboarding, agent init
├── electro-core         (traits)   13 trait definitions, types, errors, config
├── electro-gateway      (http)     axum server, health, identity, OAuth
├── electro-agent        (brain)    TEM'S MIND — autonomy modules
├── electro-providers    (llm)      Anthropic, OpenAI-compat (6 providers)
├── electro-channels     (io)       Telegram, Discord, Slack, CLI
├── electro-memory       (storage)  SQLite + Markdown with failover
├── electro-vault        (crypto)   ChaCha20-Poly1305 encrypted secrets
├── electro-tools        (actions)  Shell, browser, file ops, web fetch, git
├── electro-skills       (extend)   Skill registry
├── electro-automation   (cron)     Heartbeat, scheduled tasks
├── electro-observable   (telemetry) OpenTelemetry, metrics
├── electro-filestore    (files)    Local + S3/R2 storage
└── electro-test-utils   (testing)  Shared test helpers
```

### Message Flow

```
Channel.start() → inbound message via mpsc::channel
  → Gateway router
    → Agent runtime loop
      → Provider.complete() or Provider.stream()
      ← CompletionResponse (may contain tool_use)
      → Tool.execute() if tool_use
      ← ToolOutput fed back to provider
    ← Final response
  → Channel.send_message(OutboundMessage)
```

---

## Compilation Gate

Every change MUST pass all four checks before proceeding:

```bash
cargo check --workspace                                    # Must compile
cargo clippy --workspace --all-targets --all-features -- -D warnings  # Zero warnings
cargo fmt --all -- --check                                 # Formatting clean
cargo test --workspace                                     # All tests pass
```

If any check fails, fix the issue and re-run. Do NOT present work as done if any check fails.

---

## Unit Test Requirements

### Structure

Every new module, function, or feature MUST have unit tests in a `#[cfg(test)] mod tests` block:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn happy_path() {
        // Test the normal case
    }

    #[tokio::test]
    async fn error_case() {
        // Test error handling
    }
}
```

### Coverage Requirements

- Happy path for every public function
- Error cases and edge cases
- At least one integration-style test if the module interacts with other crates

### Running Crate-Specific Tests

```bash
cargo test -p electro-<crate> -- --nocapture
```

For filtered tests:
```bash
cargo test -p electro-tools -- browser
```

---

## Self-Testing

After compilation and unit tests pass, test the feature end-to-end.

### Build Release

```bash
cargo build --release --bin electro
```

### Run the Binary

```bash
./target/release/electro start 2>&1 | tee /tmp/electro.log &
```

Or for interactive CLI chat (once implemented):
```bash
./target/release/electro chat
```

### What to Verify

- Agent responds correctly to messages
- New tools work as expected
- Error handling works for edge-case inputs
- Log output shows no warnings or errors
- Memory persistence works across restarts

---

## Debugging Protocol

When something fails:

1. **Read the error output carefully** — Rust compiler errors are precise
2. **Tail the logs** — `tail -f /tmp/electro.log | grep --line-buffered -E "ERROR|WARN|panic"`
3. **Add tracing** — Use `tracing::debug!` with structured fields to trace data flow
4. **Isolate the failure** — Run specific crate tests: `cargo test -p electro-<crate> <test_name>`
5. **Fix and re-verify** — After fixing, re-run the full compilation gate

Never skip to the next step without understanding and fixing the current failure.

---

## Non-Negotiable Rules

1. **Never skip tests.** "It should work" is not acceptable — prove it.
2. **Never present stub code as done.** If a function has `todo!()` or `unimplemented!()`, it is not done.
3. **Always run clippy.** Zero warnings is the CI gate — match it locally.
4. **Always check logs.** Start the service, read the output, verify behavior.
5. **Fix failures immediately.** Do not accumulate technical debt across steps.
6. **The user should never find a bug that tests could have caught.**
