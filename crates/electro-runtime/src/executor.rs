use std::sync::Arc;

use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct ExecutionController {
    semaphore: Arc<Semaphore>,
}

impl ExecutionController {
    pub fn new(max: usize) -> Self {
        let bounded = max.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(bounded)),
        }
    }

    pub async fn acquire(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("execution semaphore closed")
    }

    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }
}
