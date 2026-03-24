use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use async_trait::async_trait;
use electro_agent::AgentRuntime;
use electro_core::types::config::MemoryStrategy;
use electro_core::types::message::{InboundMessage, OutboundMessage};
use electro_core::types::session::SessionContext;
use electro_core::{Channel, SetupLinkGenerator};
use electro_tools::policy::{set_runtime_policy, ToolPolicy};
use electro_runtime::{MAX_REMOTE_REQUEST_BYTES, RemoteRequest, RemoteResponse};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, RwLock};

#[path = "../app/agent.rs"]
mod app_agent;
#[path = "../app/init.rs"]
mod app_init;
#[path = "../app/onboarding.rs"]
mod app_onboarding;

struct RemoteSinkChannel;

#[async_trait]
impl Channel for RemoteSinkChannel {
    fn name(&self) -> &str {
        "remote"
    }

    async fn start(&mut self) -> Result<(), electro_core::types::error::ElectroError> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), electro_core::types::error::ElectroError> {
        Ok(())
    }

    async fn send_message(
        &self,
        _msg: OutboundMessage,
    ) -> Result<(), electro_core::types::error::ElectroError> {
        Ok(())
    }

    fn file_transfer(&self) -> Option<&dyn electro_core::FileTransfer> {
        None
    }

    fn is_allowed(&self, _user_id: &str) -> bool {
        true
    }
}

#[derive(Clone)]
struct WorkerNodeState {
    auth_token: Option<String>,
    agent: Arc<AgentRuntime>,
    workspace_path: std::path::PathBuf,
    max_task_duration_secs: u64,
    completed_requests: Arc<Mutex<HashMap<String, RemoteResponse>>>,
    in_flight_requests: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = electro_core::config::load_config(None).map_err(|e| anyhow::anyhow!(e))?;
    let core = app_init::init_core_stack(&config).await?;
    let workspace_path = electro_core::paths::workspace_dir();
    std::fs::create_dir_all(&workspace_path).ok();

    set_runtime_policy(ToolPolicy {
        allow_shell: config.tools.shell,
        allow_network: config.tools.http,
        allow_filesystem: config.tools.file || config.tools.git,
        writable_roots: vec![workspace_path.clone()],
    });

    let shared_mode = Arc::new(RwLock::new(config.mode));
    let shared_memory_strategy = Arc::new(RwLock::new(MemoryStrategy::Lambda));
    let channel: Arc<dyn Channel> = Arc::new(RemoteSinkChannel);
    let pending_messages = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut tools = electro_tools::create_tools(
        &config.tools,
        Some(channel),
        Some(pending_messages),
        Some(core.memory.clone()),
        Some(Arc::new(core.setup_tokens.clone()) as Arc<dyn SetupLinkGenerator>),
        Some(core.usage_store.clone()),
        Some(shared_mode.clone()),
        core.vault.clone(),
    );
    let custom_registry = electro_tools::CustomToolRegistry::new();
    let custom_tools = custom_registry.load_tools();
    if !custom_tools.is_empty() {
        tracing::info!(count = custom_tools.len(), "Custom script tools loaded");
        tools.extend(custom_tools);
    }

    let (provider_name, api_key, model) = app_agent::resolve_credentials(&config)
        .ok_or_else(|| anyhow::anyhow!("worker-node requires configured provider credentials"))?;
    if electro_core::config::credentials::is_placeholder_key(&api_key) {
        return Err(anyhow::anyhow!("worker-node requires non-placeholder provider credentials"));
    }

    let provider_config = app_agent::build_provider_config(&config, &provider_name, &api_key, &model);
    let provider = app_agent::create_provider(&provider_config, &provider_name, &model).await?;
    let agent = Arc::new(
        app_agent::create_agent(
            &config,
            provider,
            core.memory.clone(),
            tools,
            model,
            Some(app_onboarding::build_system_prompt()),
            app_init::check_hive_enabled().await,
            shared_mode,
            shared_memory_strategy,
        )
        .await,
    );

    let state = WorkerNodeState {
        auth_token: std::env::var("ELECTRO_WORKER_TOKEN").ok(),
        agent,
        workspace_path,
        max_task_duration_secs: config.agent.max_task_duration_secs,
        completed_requests: Arc::new(Mutex::new(HashMap::new())),
        in_flight_requests: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/execute", post(execute))
        .with_state(Arc::new(state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9001")
        .await
        .expect("failed to bind worker-node on 0.0.0.0:9001");

    axum::serve(listener, app)
        .await
        .expect("worker-node server failed");

    Ok(())
}

async fn execute(
    State(state): State<Arc<WorkerNodeState>>,
    headers: HeaderMap,
    Json(req): Json<RemoteRequest>,
) -> (StatusCode, Json<RemoteResponse>) {
    let request_id = req.request_id.clone();
    let request_history = req.history.clone();

    if let Some(expected) = state.auth_token.as_deref() {
        let authorized = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(|token| token == expected)
            .unwrap_or(false);

        if !authorized {
            return (
                StatusCode::UNAUTHORIZED,
                Json(RemoteResponse {
                    request_id,
                    output: String::new(),
                    history: request_history,
                    error: Some("unauthorized".to_string()),
                }),
            );
        }
    }

    if let Some(existing) = state.completed_requests.lock().await.get(&request_id).cloned() {
        return (StatusCode::OK, Json(existing));
    }

    let request_size = match serde_json::to_vec(&req) {
        Ok(body) => body.len(),
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(RemoteResponse {
                    request_id,
                    output: String::new(),
                    history: request_history,
                    error: Some(format!("invalid remote request: {error}")),
                }),
            );
        }
    };

    if request_size > MAX_REMOTE_REQUEST_BYTES {
        return (
            StatusCode::BAD_REQUEST,
            Json(RemoteResponse {
                request_id,
                output: String::new(),
                history: request_history,
                error: Some("remote request exceeds max payload size".to_string()),
            }),
        );
    }

    let (notify, is_owner) = {
        let mut in_flight = state.in_flight_requests.lock().await;
        if let Some(existing) = in_flight.get(&request_id).cloned() {
            (existing, false)
        } else {
            let notify = Arc::new(Notify::new());
            in_flight.insert(request_id.clone(), notify.clone());
            (notify, true)
        }
    };

    if !is_owner {
        notify.notified().await;
        if let Some(existing) = state.completed_requests.lock().await.get(&request_id).cloned() {
            return (StatusCode::OK, Json(existing));
        }
        return (
            StatusCode::CONFLICT,
            Json(RemoteResponse {
                request_id,
                output: String::new(),
                history: request_history,
                error: Some("remote request finished without cached result".to_string()),
            }),
        );
    }

    let inbound = InboundMessage {
        id: req.request_id.clone(),
        channel: req.channel.clone(),
        chat_id: req.chat_id.clone(),
        user_id: req.user_id.clone(),
        username: None,
        text: Some(req.input.clone()),
        attachments: Vec::new(),
        reply_to: None,
        timestamp: chrono::Utc::now(),
    };
    let mut session = SessionContext {
        session_id: req.chat_id.clone(),
        channel: req.channel.clone(),
        chat_id: req.chat_id.clone(),
        user_id: req.user_id.clone(),
        history: req.history.clone(),
        workspace_path: state.workspace_path.clone(),
        tool_timeout_secs: 60, // TODO: Make configurable via worker-node config
        tool_policy: electro_tools::policy::ToolPolicy::for_workspace(state.workspace_path.clone()),
    };

    let result = tokio::time::timeout(
        Duration::from_secs(state.max_task_duration_secs),
        state
            .agent
            .process_message(&inbound, &mut session, None, None, None, None, None),
    )
    .await;

    let response = match result {
        Ok(Ok((reply, _usage))) => RemoteResponse {
            request_id: request_id.clone(),
            output: reply.text,
            history: session.history,
            error: None,
        },
        Ok(Err(e)) => RemoteResponse {
            request_id: request_id.clone(),
            output: String::new(),
            history: session.history,
            error: Some(e.to_string()),
        },
        Err(_) => RemoteResponse {
            request_id: request_id.clone(),
            output: String::new(),
            history: session.history,
            error: Some(format!(
                "request timed out after {} seconds",
                state.max_task_duration_secs
            )),
        },
    };

    state
        .completed_requests
        .lock()
        .await
        .insert(request_id, response.clone());
    state
        .in_flight_requests
        .lock()
        .await
        .remove(&response.request_id);
    notify.notify_waiters();

    (StatusCode::OK, Json(response))
}
