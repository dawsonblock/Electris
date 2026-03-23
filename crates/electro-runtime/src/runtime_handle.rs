use std::sync::Arc;

use anyhow::anyhow;
use electro_agent::AgentRuntime;
use electro_core::traits::Observable;
use electro_core::types::config::{ElectroMode, MemoryStrategy};
use electro_core::types::message::ChatMessage;
use electro_core::types::message::InboundMessage;
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::config::RuntimeConfig;
use crate::executor::ExecutionController;
use crate::events::OutboundEvent;
use crate::remote::{MAX_REMOTE_REQUEST_BYTES, RemoteRequest, RemoteResponse};
use crate::router::ExecutionRouter;

/// A Write implementation that counts bytes without storing them.
/// Used to efficiently check serialized JSON size without allocating.
struct CountingWriter {
    count: usize,
}

impl CountingWriter {
    fn new() -> Self {
        Self { count: 0 }
    }

    fn count(&self) -> usize {
        self.count
    }
}

impl std::io::Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.count += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct RuntimeHandle {
    pub agent: Arc<RwLock<Option<Arc<AgentRuntime>>>>,
    pub outbound_events: broadcast::Sender<OutboundEvent>,
    pub queue_tx: mpsc::Sender<InboundMessage>,
    pub shared_mode: Arc<RwLock<ElectroMode>>,
    pub shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
    pub active_provider: Arc<RwLock<Option<String>>>,
    pub executor: ExecutionController,
    pub router: ExecutionRouter,
    pub runtime_config: RuntimeConfig,
    pub observable: Option<Arc<dyn Observable>>,
}

impl RuntimeHandle {
    pub fn new(
        queue_tx: mpsc::Sender<InboundMessage>,
        shared_mode: Arc<RwLock<ElectroMode>>,
        shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
    ) -> Self {
        Self::new_with_config(
            queue_tx,
            shared_mode,
            shared_memory_strategy,
            RuntimeConfig::default(),
        )
    }

    pub fn new_with_config(
        queue_tx: mpsc::Sender<InboundMessage>,
        shared_mode: Arc<RwLock<ElectroMode>>,
        shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
        runtime_config: RuntimeConfig,
    ) -> Self {
        Self::new_with_config_and_observable(
            queue_tx,
            shared_mode,
            shared_memory_strategy,
            runtime_config,
            None,
        )
    }

    pub fn new_with_config_and_observable(
        queue_tx: mpsc::Sender<InboundMessage>,
        shared_mode: Arc<RwLock<ElectroMode>>,
        shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
        runtime_config: RuntimeConfig,
        observable: Option<Arc<dyn Observable>>,
    ) -> Self {
        let (outbound_events, _) = broadcast::channel(64);
        let executor = ExecutionController::new(runtime_config.max_concurrency);
        let router = ExecutionRouter::new(
            runtime_config.remote_threshold_chars,
            runtime_config.remote_workers.clone(),
        );

        Self {
            agent: Arc::new(RwLock::new(None)),
            outbound_events,
            queue_tx,
            shared_mode,
            shared_memory_strategy,
            active_provider: Arc::new(RwLock::new(None)),
            executor,
            router,
            runtime_config,
            observable,
        }
    }

    pub async fn agent(&self) -> Option<Arc<AgentRuntime>> {
        self.agent.read().await.clone()
    }

    pub async fn set_agent(&self, agent: AgentRuntime) {
        self.set_agent_arc(Arc::new(agent)).await;
    }

    pub async fn set_agent_arc(&self, agent: Arc<AgentRuntime>) {
        *self.agent.write().await = Some(agent);
    }

    pub async fn active_provider(&self) -> Option<String> {
        self.active_provider.read().await.clone()
    }

    pub async fn set_active_provider(&self, provider: impl Into<String>) {
        *self.active_provider.write().await = Some(provider.into());
    }

    pub fn subscribe_outbound_events(&self) -> broadcast::Receiver<OutboundEvent> {
        self.outbound_events.subscribe()
    }

    pub fn emit_outbound_event(
        &self,
        event: OutboundEvent,
    ) -> Result<usize, broadcast::error::SendError<OutboundEvent>> {
        self.outbound_events.send(event)
    }

    pub async fn increment_counter(&self, name: &str, labels: &[(&str, &str)]) {
        if let Some(observable) = self.observable.clone() {
            if let Err(error) = observable.increment_counter(name, labels).await {
                tracing::debug!(metric = name, ?error, "failed to increment counter");
            }
        }
    }

    pub async fn record_metric(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        if let Some(observable) = self.observable.clone() {
            if let Err(error) = observable.record_metric(name, value, labels).await {
                tracing::debug!(metric = name, value, ?error, "failed to record metric");
            }
        }
    }

    pub async fn observe_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        if let Some(observable) = self.observable.clone() {
            if let Err(error) = observable.observe_histogram(name, value, labels).await {
                tracing::debug!(metric = name, value, ?error, "failed to observe histogram");
            }
        }
    }

    pub async fn run_remote(
        &self,
        request_id: String,
        input: String,
        worker: String,
        channel: String,
        chat_id: String,
        user_id: String,
        history: Vec<ChatMessage>,
    ) -> anyhow::Result<RemoteResponse> {
        if !self.runtime_config.remote_workers.is_empty()
            && !self.runtime_config.remote_workers.iter().any(|w| w == &worker)
        {
            return Err(anyhow!("remote worker not allowlisted"));
        }

        let url = format!("http://{}/execute", worker);
        let client = reqwest::Client::new();
        let request_body = RemoteRequest {
            request_id: request_id.clone(),
            input: input.clone(),
            channel: channel.clone(),
            chat_id: chat_id.clone(),
            user_id: user_id.clone(),
            history: history.clone(),
        };
        
        // Count bytes without allocating using a custom Write implementation
        let mut counter = CountingWriter::new();
        serde_json::to_writer(&mut counter, &request_body)
            .map_err(|error| anyhow!(error))?;
        if counter.count() > MAX_REMOTE_REQUEST_BYTES {
            return Err(anyhow!("remote request exceeds max payload size"));
        }

        let mut last_error: Option<anyhow::Error> = None;
        for _ in 0..self.runtime_config.remote_retries.max(1) {
            let mut request = client.post(&url).json(&request_body);

            if let Some(token) = self.runtime_config.remote_auth_token.as_deref() {
                request = request.bearer_auth(token);
            }

            match request.send().await {
                Ok(response) => match response.json::<RemoteResponse>().await {
                    Ok(body) => {
                        return Ok(body);
                    }
                    Err(error) => {
                        last_error = Some(anyhow!(error));
                    }
                },
                Err(error) => {
                    last_error = Some(anyhow!(error));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("remote execution failed")))
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeHandle;
    use crate::events::OutboundEvent;
    use electro_core::types::config::{ElectroMode, MemoryStrategy};
    use electro_core::types::message::InboundMessage;
    use std::sync::Arc;
    use tokio::sync::{mpsc, RwLock};

    fn test_message(text: &str) -> InboundMessage {
        InboundMessage {
            id: "msg-1".to_string(),
            channel: "test".to_string(),
            chat_id: "chat-1".to_string(),
            user_id: "user-1".to_string(),
            username: Some("tester".to_string()),
            text: Some(text.to_string()),
            attachments: Vec::new(),
            reply_to: None,
            timestamp: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn clone_shares_runtime_state_and_queue() {
        let (queue_tx, mut queue_rx) = mpsc::channel(1);
        let handle = RuntimeHandle::new(
            queue_tx,
            Arc::new(RwLock::new(ElectroMode::Play)),
            Arc::new(RwLock::new(MemoryStrategy::Lambda)),
        );
        let cloned = handle.clone();

        *cloned.shared_mode.write().await = ElectroMode::Work;
        *cloned.shared_memory_strategy.write().await = MemoryStrategy::Echo;
        cloned
            .queue_tx
            .send(test_message("hello from runtime handle"))
            .await
            .expect("queue send should succeed");
        cloned.set_active_provider("anthropic").await;

        let queued = queue_rx.recv().await.expect("message should be queued");

        assert!(handle.agent().await.is_none());
        assert!(matches!(
            *handle.shared_mode.read().await,
            ElectroMode::Work
        ));
        assert!(matches!(
            *handle.shared_memory_strategy.read().await,
            MemoryStrategy::Echo
        ));
        assert_eq!(handle.active_provider().await.as_deref(), Some("anthropic"));
        assert_eq!(queued.text.as_deref(), Some("hello from runtime handle"));
    }

    #[tokio::test]
    async fn clone_shares_outbound_event_bus() {
        let (queue_tx, _queue_rx) = mpsc::channel(1);
        let handle = RuntimeHandle::new(
            queue_tx,
            Arc::new(RwLock::new(ElectroMode::Play)),
            Arc::new(RwLock::new(MemoryStrategy::Lambda)),
        );
        let cloned = handle.clone();
        let mut events = handle.subscribe_outbound_events();

        cloned
            .emit_outbound_event(OutboundEvent::Started {
                request_id: "req-1".to_string(),
            })
            .expect("event send should succeed");

        let event = events.recv().await.expect("event should be received");
        assert_eq!(
            event,
            OutboundEvent::Started {
                request_id: "req-1".to_string(),
            }
        );
    }
}
