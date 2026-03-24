# ADR-006: Technology Stack Decisions

## Status: Proposed

## Decision

| Component | Crate | Version | Rationale |
|-----------|-------|---------|-----------|
| Async runtime | tokio | 1.x | Industry standard, required by axum/sqlx/teloxide |
| HTTP/WS server | axum | 0.8.x | Best ergonomics, Tower middleware, WebSocket native |
| Serialization | serde + toml + serde_yaml + serde_json | latest | TOML native + YAML compat + JSON for APIs |
| Database | sqlx | 0.8.x | Async, compile-time checked queries, SQLite + PG |
| Telegram | teloxide | 0.17.x | Most mature Rust Telegram framework |
| Discord | serenity + poise | 0.12.x | Standard Rust Discord library + command framework |
| Slack | Custom HTTP (reqwest) | — | No mature Rust Slack SDK; raw API is straightforward |
| WhatsApp | Custom HTTP (reqwest) | — | WhatsApp Business API via HTTP |
| Browser | chromiumoxide | latest | Async Chrome DevTools Protocol, headed + headless |
| Crypto | chacha20poly1305 + ed25519-dalek | latest | AEAD encryption + signature verification |
| CLI | clap | 4.x | Derive-based CLI parsing |
| Logging | tracing + tracing-subscriber | latest | Structured, async-aware logging |
| Error types | thiserror (libs) + anyhow (binary) | latest | Standard Rust error handling |
| HTTP client | reqwest | 0.12.x | For provider APIs, Slack, WhatsApp |
| TLS | rustls | latest | Pure Rust TLS, no OpenSSL dependency |
| Object storage | aws-sdk-s3 | latest | S3-compatible (works with R2, GCS, MinIO) |
| Config | config-rs | 0.14.x | Multi-source config with TOML backend |
| OpenTelemetry | opentelemetry + tracing-opentelemetry | latest | Cloud-native observability |
| Vector search | sqlite-vss or custom HNSW | — | Embedded vector search for memory |

## Consequences
- Pure Rust stack — no C/C++ dependencies except Chrome binary for browser automation
- Static binary via musl target achievable
- All async — no blocking calls in the hot path
- Chrome/Chromium binary must be present for browser tool (feature-gated)
