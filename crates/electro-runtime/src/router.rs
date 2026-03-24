use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use electro_core::types::message::InboundMessage;

#[derive(Debug, Clone)]
pub enum ExecutionTarget {
    Local,
    Remote(String),
}

#[derive(Debug, Clone)]
pub struct WorkerRegistry {
    workers: Vec<String>,
    next: Arc<AtomicUsize>,
}

impl WorkerRegistry {
    pub fn new(workers: Vec<String>) -> Self {
        Self {
            workers,
            next: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.workers.is_empty()
    }

    pub fn pick(&self) -> Option<String> {
        if self.workers.is_empty() {
            return None;
        }
        let idx = self.next.fetch_add(1, Ordering::Relaxed) % self.workers.len();
        self.workers.get(idx).cloned()
    }

    pub fn workers(&self) -> &[String] {
        &self.workers
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionRouter {
    pub remote_threshold_chars: usize,
    pub registry: WorkerRegistry,
}

impl ExecutionRouter {
    pub fn new(remote_threshold_chars: usize, workers: Vec<String>) -> Self {
        Self {
            remote_threshold_chars,
            registry: WorkerRegistry::new(workers),
        }
    }

    pub fn route(&self, task: &InboundMessage) -> ExecutionTarget {
        let input_len = task.text.as_deref().map(str::len).unwrap_or(0);
        if input_len > self.remote_threshold_chars {
            if let Some(worker) = self.registry.pick() {
                return ExecutionTarget::Remote(worker);
            }
        }
        ExecutionTarget::Local
    }
}
