use std::sync::Arc;

use electro_agent::AgentRuntime;
use electro_core::types::config::{ElectroMode, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use tokio::sync::{broadcast, mpsc, RwLock};

use crate::events::OutboundEvent;

#[derive(Clone)]
pub struct RuntimeHandle {
    pub agent: Arc<RwLock<Option<Arc<AgentRuntime>>>>,
    pub outbound_events: broadcast::Sender<OutboundEvent>,
    pub queue_tx: mpsc::Sender<InboundMessage>,
    pub shared_mode: Arc<RwLock<ElectroMode>>,
    pub shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
    pub active_provider: Arc<RwLock<Option<String>>>,
}

impl RuntimeHandle {
    pub fn new(
        queue_tx: mpsc::Sender<InboundMessage>,
        shared_mode: Arc<RwLock<ElectroMode>>,
        shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
    ) -> Self {
        let (outbound_events, _) = broadcast::channel(64);
        Self {
            agent: Arc::new(RwLock::new(None)),
            outbound_events,
            queue_tx,
            shared_mode,
            shared_memory_strategy,
            active_provider: Arc::new(RwLock::new(None)),
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
