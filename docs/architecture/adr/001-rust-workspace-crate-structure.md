# ADR-001: Rust Workspace with Multi-Crate Structure

## Status: Proposed

## Context
ELECTRO is a complex system with 12+ subsystems (channels, providers, memory, tools, etc.). We need a code organization strategy that enables:
- Independent compilation of subsystems
- Clear dependency boundaries
- Feature-flag control over optional components
- Fast incremental builds during development

## Decision
Use a Cargo workspace with 13 crates:
- `electro-core`: Trait definitions + shared types (zero external deps beyond serde/async-trait)
- `electro-gateway`: axum-based gateway server
- `electro-agent`: Agent runtime loop
- `electro-providers`: AI provider implementations
- `electro-channels`: Messaging channel implementations
- `electro-memory`: Memory backend implementations
- `electro-vault`: Secrets management
- `electro-tools`: Built-in tool implementations
- `electro-skills`: Skill loading & management
- `electro-automation`: Heartbeat & cron
- `electro-observable`: Logging, metrics, tracing
- `electro-filestore`: File storage backends
- `electro` (binary): CLI entry point

## Consequences
- Clear separation of concerns — each crate has a focused responsibility
- Feature flags can exclude entire crates (e.g., `--no-default-features` to skip browser)
- Parallel compilation of independent crates speeds up builds
- More complex Cargo.toml management
- All crates depend on `electro-core` for trait definitions
