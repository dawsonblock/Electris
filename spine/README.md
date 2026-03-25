# Electris Runtime Spine

A bounded Rust agent runtime with a single, verifiable execution path and LLM integration.

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐     ┌──────────────┐
│   Gateway   │────▶│   Runtime    │────▶│   Planner   │────▶│  Dispatcher  │
│  (HTTP API) │     │(submit_intent)│     │(Intent→Cmd) │     │(Route to Wrk)│
└─────────────┘     └──────────────┘     └─────────────┘     └──────────────┘
                                                                           │
                                                                           ▼
                              ┌─────────────────────────────────────────────────────┐
                              │                      Worker                         │
                              │  (Policy Check → Sandbox → Tool Execution → Result) │
                              └─────────────────────────────────────────────────────┘
                                                                           │
                                                                           ▼
                              ┌─────────────────────────────────────────────────────┐
                              │                      LLM Client                     │
                              │  (Plan Generation → Code Analysis → Chat Completion)│
                              └─────────────────────────────────────────────────────┘
```

## Invariants

1. **Single Entrypoint**: Only `submit_intent()` may initiate execution
2. **Event-First**: All actions produce `DomainEvent`s
3. **Transport-Only Gateway**: No business logic, no tool access
4. **Worker Boundary**: Only worker may execute tools
5. **Policy Enforcement**: All actions checked against execution policy
6. **Sandbox Limits**: Resource limits enforced on all operations

## Quick Start

### Run Locally

```bash
# Copy example config
cp spine.toml.example spine.toml

# Edit spine.toml with your API keys
# Or use environment variables

# Run the server
cargo run --release

# Or use make
make run
```

### Run with Docker

```bash
# Build and run
docker-compose up -d

# Or manually
docker build -t spine-server .
docker run -p 8080:8080 -e SPINE_LLM__API_KEY=your-key spine-server
```

### API Usage

```bash
# Health check
curl http://localhost:8080/health

# Execute a task
curl -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "payload": {
      "action": "analyze",
      "target": "src/main.rs"
    }
  }'

# Read a file
curl -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "payload": {
      "action": "read_file",
      "path": "/path/to/file.txt"
    }
  }'

# Run shell command
curl -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "payload": {
      "action": "shell",
      "command": "echo hello"
    }
  }'

# Git status
curl -X POST http://localhost:8080/execute \
  -H "Content-Type: application/json" \
  -d '{
    "payload": {
      "action": "git",
      "subcommand": "status",
      "repo": "."
    }
  }'
```

## Configuration

Configuration can be loaded from:
1. Environment variables (highest priority)
2. `spine.toml` file
3. Default values (lowest priority)

### Environment Variables

Prefix with `SPINE_` and use `__` as separator:

```bash
SPINE_SERVER__PORT=9090
SPINE_LLM__API_KEY=your-key-here
SPINE_LLM__MODEL=gpt-4
SPINE_WORKER__POLICY=permissive
SPINE_LOGGING__LEVEL=debug
```

### Configuration File

See `spine.toml.example` for a complete example:

```toml
[server]
port = 8080
host = "0.0.0.0"

[llm]
provider = "openai"
model = "gpt-4"
max_tokens = 4000
temperature = 0.7

[worker]
policy = "standard"

[logging]
level = "info"
format = "pretty"
```

## Security Features

### Policy Enforcement

Three policy modes available:
- **permissive**: All operations allowed (development only)
- **standard**: File system, shell, and git allowed; network blocked (default)
- **restrictive**: Minimal permissions (production)

### Command Blocklist

Dangerous commands are automatically blocked:
- `rm -rf /`
- Fork bombs (`:(){ :|:& };:`)
- Disk writes to system paths
- Formatting commands

### Resource Limits

Configurable limits on all operations:
- Memory (default: 512 MB)
- CPU time (default: 60 seconds)
- Output size (default: 1 MB)
- File size (default: 100 MB)

### Timeout Protection

All shell commands have automatic timeout protection (default: 60 seconds).

## LLM Integration

The spine runtime supports multiple LLM providers:

### OpenAI

```toml
[llm]
provider = "openai"
model = "gpt-4"
```

### Anthropic

```toml
[llm]
provider = "anthropic"
model = "claude-3-sonnet-20240229"
```

### Kimi (Moonshot AI)

```toml
[llm]
provider = "kimi"
model = "kimi-latest"
```

Get your API key from: https://platform.moonshot.cn/

### Using the LLM Client

```rust
use spine_llm::{create_provider, LlmClient, ChatMessage};

// Create provider
let provider = create_provider(
    "openai",
    std::env::var("OPENAI_API_KEY")?,
    "gpt-4".to_string(),
)?;

// Create client
let mut client = LlmClient::new(provider)
    .with_system_prompt("You are a helpful coding assistant");

// Single completion
let response = client.complete("Say hello").await?;

// Multi-turn conversation
let response = client.chat("What can you do?").await?;
let response = client.chat("Help me refactor this code").await?;

// Generate execution plan
let steps = client.plan("Create a new Rust project with tests").await?;

// Analyze code
let analysis = client.analyze_code(code, "rust").await?;
```

## Project Structure

```
spine/
├── Cargo.toml              # Workspace root
├── spine.toml              # Configuration file
├── src/
│   └── main.rs             # Server binary
├── crates/
│   ├── spine-core/         # Core types
│   ├── spine-runtime/      # Execution authority
│   ├── spine-gateway/      # HTTP transport
│   ├── spine-worker/       # Execution boundary
│   ├── spine-tools/        # Tool implementations
│   ├── spine-config/       # Configuration management
│   └── spine-llm/          # LLM provider integration
├── tests/
├── scripts/
└── Dockerfile
```

## Development

```bash
# Build
cargo build --release

# Test
cargo test --workspace

# Verify everything
make verify

# Format and lint
cargo fmt --all
cargo clippy --workspace -- -D warnings
```

## Verification

The `verify.sh` script checks:
- ✅ `cargo check` passes
- ✅ `cargo test` passes
- ✅ `clippy` passes with no warnings
- ✅ Single public entrypoint (`submit_intent`)

```bash
./scripts/verify.sh
```

## Design Decisions

### Why Single Entrypoint?

By forcing all execution through `submit_intent()`, we:
- Ensure consistent policy enforcement
- Maintain complete audit trails via events
- Prevent bypasses and unauthorized execution
- Enable testing and verification

### Why Event-First?

Every action produces events:
- `IntentReceived`
- `CommandStarted`
- `CommandCompleted`/`CommandFailed`

This creates an immutable history of all system activity.

### Why Transport-Only Gateway?

The gateway (HTTP API) contains zero business logic:
- Parses HTTP requests
- Creates `Intent` from JSON
- Calls `submit_intent()`
- Returns `Outcome` as JSON

This ensures the gateway can be replaced (gRPC, WebSocket, etc.) without changing execution behavior.

### Why Policy + Sandbox?

**Policy**: Controls *what* can be done (permissions)
**Sandbox**: Controls *how much* can be done (resources)

This separation allows flexible security configurations.

## License

MIT
