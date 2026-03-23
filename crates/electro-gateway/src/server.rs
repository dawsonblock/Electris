//! SkyGate server — axum-based HTTP server with health/status routes,
//! WebSocket upgrade support, and shared application state.

use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use electro_agent::AgentRuntime;
use electro_core::types::config::GatewayConfig;
use electro_core::types::error::ElectroError;
use electro_core::Channel;
use electro_runtime::RuntimeHandle;
use tokio::net::TcpListener;
use tracing::info;

use crate::dashboard::{dashboard_config, dashboard_health, dashboard_page, dashboard_tasks};
use crate::health::{health_handler, readiness_handler, status_handler};
use crate::identity::{oauth_callback_handler, OAuthIdentityManager};
use crate::session::SessionManager;

/// Shared application state accessible from all handlers.
pub struct AppState {
    pub channels: Vec<Arc<dyn Channel>>,
    pub agent: Option<Arc<AgentRuntime>>,
    pub runtime: Option<RuntimeHandle>,
    pub config: GatewayConfig,
    pub sessions: SessionManager,
    pub identity: Option<Arc<OAuthIdentityManager>>,
}

impl AppState {
    pub async fn agent(&self) -> Option<Arc<AgentRuntime>> {
        if let Some(runtime) = &self.runtime {
            runtime.agent().await
        } else {
            self.agent.clone()
        }
    }

    pub async fn active_provider_name(&self) -> Option<String> {
        if let Some(runtime) = &self.runtime {
            let provider = runtime.active_provider().await;
            if provider.is_some() {
                return provider;
            }
        }

        self.agent
            .as_ref()
            .map(|agent| agent.provider().name().to_string())
    }
}

/// The main SkyGate server.
pub struct SkyGate {
    state: Arc<AppState>,
}

impl SkyGate {
    /// Create a new SkyGate server.
    pub fn new(
        channels: Vec<Arc<dyn Channel>>,
        runtime: RuntimeHandle,
        config: GatewayConfig,
    ) -> Self {
        let state = Arc::new(AppState {
            channels,
            agent: None,
            runtime: Some(runtime),
            config,
            sessions: SessionManager::new(),
            identity: None,
        });
        Self { state }
    }

    /// Create a new SkyGate server with an OAuth identity manager.
    pub fn with_identity(
        channels: Vec<Arc<dyn Channel>>,
        runtime: RuntimeHandle,
        config: GatewayConfig,
        identity: OAuthIdentityManager,
    ) -> Self {
        let state = Arc::new(AppState {
            channels,
            agent: None,
            runtime: Some(runtime),
            config,
            sessions: SessionManager::new(),
            identity: Some(Arc::new(identity)),
        });
        Self { state }
    }

    /// Build the axum Router with all routes.
    fn build_router(&self) -> Router {
        let mut router = Router::new()
            .route("/health", get(health_handler))
            .route("/health/ready", get(readiness_handler))
            .route("/status", get(status_handler))
            .route("/dashboard", get(dashboard_page))
            .route("/dashboard/api/health", get(dashboard_health))
            .route("/dashboard/api/tasks", get(dashboard_tasks))
            .route("/dashboard/api/config", get(dashboard_config))
            .with_state(self.state.clone());

        // Mount OAuth callback when identity is configured
        if let Some(ref identity) = self.state.identity {
            let auth_router = Router::new()
                .route("/auth/callback", get(oauth_callback_handler))
                .with_state(identity.clone());
            router = router.merge(auth_router);
        }

        router
    }

    /// Bind to the configured host and port, returning the listener.
    /// This allows the caller to fail fast if the port is already in use.
    pub async fn bind(&self) -> Result<TcpListener, ElectroError> {
        let addr = format!("{}:{}", self.state.config.host, self.state.config.port);
        TcpListener::bind(&addr)
            .await
            .map_err(|e| ElectroError::Internal(format!("Failed to bind to {}: {}", addr, e)))
    }

    /// Start the server using the provided listener.
    pub async fn serve(&self, listener: TcpListener) -> Result<(), ElectroError> {
        let addr = listener
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        info!(addr = %addr, "Starting SkyGate server");

        let router = self.build_router();

        axum::serve(listener, router)
            .await
            .map_err(|e| ElectroError::Internal(format!("Server error: {}", e)))?;

        Ok(())
    }

    /// Get a reference to the shared application state.
    pub fn state(&self) -> &Arc<AppState> {
        &self.state
    }

    /// Get a reference to the session manager.
    pub fn sessions(&self) -> &SessionManager {
        &self.state.sessions
    }
}
