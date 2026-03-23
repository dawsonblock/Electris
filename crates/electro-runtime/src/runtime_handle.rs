use std::sync::Arc;

use electro_agent::AgentRuntime;
use electro_core::types::config::{ElectroMode, MemoryStrategy};
use electro_core::types::message::InboundMessage;
use tokio::sync::{mpsc, RwLock};

#[derive(Clone)]
pub struct RuntimeHandle {
    pub agent: Arc<RwLock<Option<Arc<AgentRuntime>>>>,
    pub queue_tx: mpsc::Sender<InboundMessage>,
    pub shared_mode: Arc<RwLock<ElectroMode>>,
    pub shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
}

impl RuntimeHandle {
    pub fn new(
        queue_tx: mpsc::Sender<InboundMessage>,
        shared_mode: Arc<RwLock<ElectroMode>>,
        shared_memory_strategy: Arc<RwLock<MemoryStrategy>>,
    ) -> Self {
        Self {
            agent: Arc::new(RwLock::new(None)),
            queue_tx,
            shared_mode,
            shared_memory_strategy,
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
}

#[cfg(test)]
mod tests {
    use super::RuntimeHandle;
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
        assert_eq!(queued.text.as_deref(), Some("hello from runtime handle"));
    }
}
