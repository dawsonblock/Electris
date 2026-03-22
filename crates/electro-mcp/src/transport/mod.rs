//! MCP transport layer — abstracts stdio and HTTP communication.

pub mod http;
pub mod stdio;

use crate::jsonrpc::{JsonRpcNotification, JsonRpcResponse};
use async_trait::async_trait;
use electro_core::types::error::ElectroError;

/// Transport trait — sends JSON-RPC messages to an MCP server.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Send a JSON-RPC request and wait for the matching response.
    async fn send(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, ElectroError>;

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<(), ElectroError>;

    /// Send a raw JSON-RPC notification object.
    async fn notify_raw(&self, notification: JsonRpcNotification) -> Result<(), ElectroError> {
        self.notify(&notification.method, notification.params).await
    }

    /// Check if the transport is still alive.
    fn is_alive(&self) -> bool;

    /// Close the transport and clean up resources.
    async fn close(&self) -> Result<(), ElectroError>;
}

/// Null transport — always returns errors. Used as placeholder for disconnected servers.
pub(crate) struct NullTransport;

#[async_trait]
impl Transport for NullTransport {
    async fn send(
        &self,
        _method: &str,
        _params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse, ElectroError> {
        Err(ElectroError::Tool("MCP server is not connected".to_string()))
    }

    async fn notify(
        &self,
        _method: &str,
        _params: Option<serde_json::Value>,
    ) -> Result<(), ElectroError> {
        Err(ElectroError::Tool("MCP server is not connected".to_string()))
    }

    fn is_alive(&self) -> bool {
        false
    }

    async fn close(&self) -> Result<(), ElectroError> {
        Ok(())
    }
}
