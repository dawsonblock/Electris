# API Reference: Core Traits

ELECTRO defines 12 core traits in `electro-core` (plus the `FileTransfer` sub-trait on `Channel`). Every subsystem is a trait implementation. Trait objects (`Box<dyn Trait>`) provide runtime polymorphism, with configuration determining which implementation is used.

All traits require `Send + Sync` and use `#[async_trait]` for async method support.

Source: `crates/electro-core/src/traits/`

---

## 1. Provider

**File**: `traits/provider.rs`

AI model provider. Implement for each AI backend (Anthropic, OpenAI, Google, Mistral, Groq).

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider name (e.g., "anthropic", "openai-compatible")
    fn name(&self) -> &str;

    /// Send a completion request and get a full response
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, ElectroError>;

    /// Send a completion request and get a streaming response
    async fn stream(&self, request: CompletionRequest) -> Result<BoxStream<'_, Result<StreamChunk, ElectroError>>, ElectroError>;

    /// Check if the provider is healthy and reachable
    async fn health_check(&self) -> Result<bool, ElectroError>;

    /// List available models for this provider
    async fn list_models(&self) -> Result<Vec<String>, ElectroError>;
}
```

**Implementations**: `electro-providers` crate -- `anthropic.rs`, `openai_compat.rs`, `google.rs`, `mistral.rs`, `groq.rs`

---

## 2. Channel

**File**: `traits/channel.rs`

Messaging channel. Implement for each platform (Telegram, Discord, Slack, CLI).

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    /// Channel name (e.g., "telegram", "discord", "cli")
    fn name(&self) -> &str;

    /// Start the channel listener (connect to platform API)
    async fn start(&mut self) -> Result<(), ElectroError>;

    /// Stop the channel listener gracefully
    async fn stop(&mut self) -> Result<(), ElectroError>;

    /// Send a text message to a specific chat
    async fn send_message(&self, msg: OutboundMessage) -> Result<(), ElectroError>;

    /// Get the file transfer capability for this channel (None if not supported)
    fn file_transfer(&self) -> Option<&dyn FileTransfer>;

    /// Check if a user is allowed to use this channel
    fn is_allowed(&self, user_id: &str) -> bool;
}
```

**Implementations**: `electro-channels` crate -- `telegram.rs`, `discord.rs`, `slack.rs`, `cli.rs`

---

## 3. FileTransfer (sub-trait of Channel)

**File**: `traits/channel.rs`

Bi-directional file transfer. Every messaging channel should implement this alongside the `Channel` trait.

```rust
#[async_trait]
pub trait FileTransfer: Send + Sync {
    /// Receive files attached to an inbound message
    async fn receive_file(&self, msg: &InboundMessage) -> Result<Vec<ReceivedFile>, ElectroError>;

    /// Send a file to a user via the messaging platform
    async fn send_file(&self, chat_id: &str, file: OutboundFile) -> Result<(), ElectroError>;

    /// Stream a large file with progress
    async fn send_file_stream(
        &self,
        chat_id: &str,
        stream: BoxStream<'_, Bytes>,
        metadata: FileMetadata,
    ) -> Result<(), ElectroError>;

    /// Maximum file size this channel supports (in bytes)
    fn max_file_size(&self) -> usize;
}
```

**Implementations**: `electro-channels/src/file_transfer.rs` -- per-channel implementations

---

## 4. Tool

**File**: `traits/tool.rs`

Agent capabilities such as shell execution, file operations, browser automation, Git, and HTTP requests.

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (e.g., "shell", "browser", "file_read")
    fn name(&self) -> &str;

    /// Human-readable description for the AI model
    fn description(&self) -> &str;

    /// JSON Schema for tool parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// What resources this tool needs (for sandboxing enforcement)
    fn declarations(&self) -> ToolDeclarations;

    /// Execute the tool with given input
    async fn execute(&self, input: ToolInput, ctx: &ToolContext) -> Result<ToolOutput, ElectroError>;
}
```

**Associated types**:

- `ToolDeclarations` -- file access paths, network domains, shell access flag
- `PathAccess` -- `Read(String)`, `Write(String)`, `ReadWrite(String)`
- `ToolInput` -- name + JSON arguments
- `ToolOutput` -- content string + is_error flag
- `ToolContext` -- workspace path + session ID

**Implementations**: `electro-tools` crate -- `shell.rs`, `file_ops.rs`, `browser.rs`, `git.rs`, `http.rs`, `screenshot.rs`

---

## 5. Memory

**File**: `traits/memory.rs`

Persistence for conversations, long-term memory, and skills. Supports hybrid search (vector similarity + keyword matching).

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    /// Store a memory entry
    async fn store(&self, entry: MemoryEntry) -> Result<(), ElectroError>;

    /// Hybrid search: vector similarity + keyword matching
    async fn search(&self, query: &str, opts: SearchOpts) -> Result<Vec<MemoryEntry>, ElectroError>;

    /// Get a specific memory entry by ID
    async fn get(&self, id: &str) -> Result<Option<MemoryEntry>, ElectroError>;

    /// Delete a memory entry
    async fn delete(&self, id: &str) -> Result<(), ElectroError>;

    /// List all sessions
    async fn list_sessions(&self) -> Result<Vec<String>, ElectroError>;

    /// Get conversation history for a session
    async fn get_session_history(&self, session_id: &str, limit: usize) -> Result<Vec<MemoryEntry>, ElectroError>;

    /// Backend name (e.g., "sqlite", "postgres", "markdown")
    fn backend_name(&self) -> &str;
}
```

**Associated types**:

- `MemoryEntry` -- id, content, metadata (JSON), timestamp, optional session_id, entry_type
- `MemoryEntryType` -- `Conversation`, `LongTerm`, `DailyLog`, `Skill`
- `SearchOpts` -- limit, vector_weight (default 0.7), keyword_weight (default 0.3), optional session/type filters

**Implementations**: `electro-memory` crate -- `sqlite.rs`, `postgres.rs`, `markdown.rs`

---

## 6. Vault

**File**: `traits/vault.rs`

Encrypted secrets management. Stores API keys and credentials encrypted at rest with ChaCha20-Poly1305.

```rust
#[async_trait]
pub trait Vault: Send + Sync {
    /// Store a secret (encrypts before storage)
    async fn store_secret(&self, key: &str, plaintext: &[u8]) -> Result<(), ElectroError>;

    /// Retrieve a secret (decrypts on read)
    async fn get_secret(&self, key: &str) -> Result<Option<Vec<u8>>, ElectroError>;

    /// Delete a secret
    async fn delete_secret(&self, key: &str) -> Result<(), ElectroError>;

    /// List secret keys (names only, not values)
    async fn list_keys(&self) -> Result<Vec<String>, ElectroError>;

    /// Check if a key exists
    async fn has_key(&self, key: &str) -> Result<bool, ElectroError>;

    /// Resolve a vault:// URI to its plaintext value
    async fn resolve_uri(&self, uri: &str) -> Result<Option<Vec<u8>>, ElectroError>;

    /// Vault backend name (e.g., "local-chacha20", "aws-kms")
    fn backend_name(&self) -> &str;
}
```

**Implementations**: `electro-vault` crate -- `local.rs` (ChaCha20-Poly1305), `resolver.rs` (vault:// URI resolution), `detector.rs` (API key pattern detection)

---

## 7. FileStore

**File**: `traits/filestore.rs`

File storage backends for local filesystem or cloud object storage (S3, R2, GCS).

```rust
#[async_trait]
pub trait FileStore: Send + Sync {
    /// Store a file and return its storage key
    async fn store(&self, path: &str, data: Bytes, metadata: FileMetadata) -> Result<String, ElectroError>;

    /// Store a file from a stream (for large files)
    async fn store_stream(
        &self,
        path: &str,
        stream: BoxStream<'_, Bytes>,
        metadata: FileMetadata,
    ) -> Result<String, ElectroError>;

    /// Retrieve a file by its storage key
    async fn get(&self, key: &str) -> Result<Option<Bytes>, ElectroError>;

    /// Generate a presigned URL for direct access (for cloud backends)
    async fn presigned_url(&self, key: &str, expires_in_secs: u64) -> Result<Option<String>, ElectroError>;

    /// Delete a file
    async fn delete(&self, key: &str) -> Result<(), ElectroError>;

    /// List files in a path prefix
    async fn list(&self, prefix: &str) -> Result<Vec<String>, ElectroError>;

    /// Backend name (e.g., "local", "s3")
    fn backend_name(&self) -> &str;
}
```

**Implementations**: `electro-filestore` crate -- `local.rs`, `s3.rs`

---

## 8. Observable

**File**: `traits/observable.rs`

Monitoring, logging, and metrics collection. Used by all subsystems to report health and performance.

```rust
#[async_trait]
pub trait Observable: Send + Sync {
    /// Record a metric
    async fn record_metric(&self, name: &str, value: f64, labels: &[(&str, &str)]) -> Result<(), ElectroError>;

    /// Record a counter increment
    async fn increment_counter(&self, name: &str, labels: &[(&str, &str)]) -> Result<(), ElectroError>;

    /// Record a histogram observation
    async fn observe_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]) -> Result<(), ElectroError>;

    /// Report health status
    async fn health_status(&self) -> Result<HealthStatus, ElectroError>;
}
```

**Associated types**:

- `HealthStatus` -- overall status + list of component health records
- `HealthState` -- `Healthy`, `Degraded`, `Unhealthy`
- `ComponentHealth` -- name, status, optional message

**Implementations**: `electro-observable` crate -- `logging.rs`, `metrics.rs`, `otel.rs`

---

## 9. Identity

**File**: `traits/identity.rs`

Authentication and authorization for channel users.

```rust
#[async_trait]
pub trait Identity: Send + Sync {
    /// Authenticate a user from a channel message
    async fn authenticate(&self, channel: &str, user_id: &str) -> Result<AuthResult, ElectroError>;

    /// Check if a user has a specific permission
    async fn has_permission(&self, user_id: &str, permission: &str) -> Result<bool, ElectroError>;

    /// Register a new user (from chat-based onboarding)
    async fn register_user(&self, user_id: &str, channel: &str) -> Result<(), ElectroError>;
}
```

**Associated types**:

- `AuthResult` -- `Allowed`, `Denied { reason }`, `NeedsSetup`

---

## 10. Tunnel

**File**: `traits/tunnel.rs`

Secure external access via tunnel providers (Cloudflare Tunnel, ngrok, Tailscale, etc.).

```rust
#[async_trait]
pub trait Tunnel: Send + Sync {
    /// Start the tunnel and return the public URL
    async fn start(&mut self, local_port: u16) -> Result<String, ElectroError>;

    /// Stop the tunnel
    async fn stop(&mut self) -> Result<(), ElectroError>;

    /// Get the current public URL (None if not running)
    fn public_url(&self) -> Option<&str>;

    /// Tunnel provider name (e.g., "cloudflare", "ngrok")
    fn provider_name(&self) -> &str;
}
```

---

## 11. Orchestrator (stub)

**File**: `traits/orchestrator.rs`

Container/VM lifecycle management. Stub for v0.1; designed for future multi-instance orchestration.

```rust
#[async_trait]
pub trait Orchestrator: Send + Sync {
    async fn provision(&self, spec: AgentSpec) -> Result<AgentInstance, ElectroError>;
    async fn scale(&self, instance: &AgentInstance, replicas: u32) -> Result<(), ElectroError>;
    async fn destroy(&self, instance: &AgentInstance) -> Result<(), ElectroError>;
    async fn health(&self, instance: &AgentInstance) -> Result<bool, ElectroError>;
    fn backend_name(&self) -> &str;
}
```

**Associated types**:

- `AgentSpec` -- name, image, env vars, resource limits (memory_mb, cpu_millicores)
- `AgentInstance` -- id, name, status, optional URL
- `ResourceLimits` -- memory_mb, cpu_millicores

---

## 12. Tenant (stub)

**File**: `traits/tenant.rs`

Multi-tenancy isolation. Stub for v0.1; single-tenant only, but the trait is designed for future multi-tenant deployments.

```rust
#[async_trait]
pub trait Tenant: Send + Sync {
    /// Get tenant ID from a channel user
    async fn resolve_tenant(&self, channel: &str, user_id: &str) -> Result<TenantId, ElectroError>;

    /// Get workspace path for a tenant
    fn workspace_path(&self, tenant_id: &TenantId) -> std::path::PathBuf;

    /// Check rate limits for a tenant
    async fn check_rate_limit(&self, tenant_id: &TenantId) -> Result<bool, ElectroError>;
}
```

**Associated types**:

- `TenantId(String)` -- wraps a string identifier; `TenantId::default_tenant()` returns `"default"`

---

## 13. Peripheral (stub)

**File**: `traits/peripheral.rs`

Hardware integration for sensors and GPIO. Stub for v0.1; out of scope.

```rust
#[async_trait]
pub trait Peripheral: Send + Sync {
    fn name(&self) -> &str;
    async fn read(&self) -> Result<serde_json::Value, ElectroError>;
    async fn write(&self, data: serde_json::Value) -> Result<(), ElectroError>;
}
```
