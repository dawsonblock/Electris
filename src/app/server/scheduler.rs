use electro_core::types::message::InboundMessage;
use std::collections::{HashMap, VecDeque};

pub type RuntimeMessage = InboundMessage;

pub struct Scheduler {
    queue: VecDeque<RuntimeMessage>,
    active_per_chat: HashMap<String, usize>,
    max_queue: usize,
    max_active_per_chat: usize,
}

impl Scheduler {
    pub fn new(max_queue: usize, max_active_per_chat: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            active_per_chat: HashMap::new(),
            max_queue: max_queue.max(1),
            max_active_per_chat: max_active_per_chat.max(1),
        }
    }

    pub fn push(&mut self, msg: RuntimeMessage) -> Result<(), &'static str> {
        if self.queue.len() >= self.max_queue {
            return Err("overloaded");
        }
        self.queue.push_back(msg);
        Ok(())
    }

    pub fn next(&mut self) -> Option<RuntimeMessage> {
        let len = self.queue.len();
        for _ in 0..len {
            let msg = self.queue.pop_front()?;
            let active = self.active_per_chat.get(&msg.chat_id).copied().unwrap_or(0);
            if active < self.max_active_per_chat {
                return Some(msg);
            }
            self.queue.push_back(msg);
        }
        None
    }

    pub fn mark_dispatched(&mut self, chat_id: &str) {
        *self.active_per_chat.entry(chat_id.to_string()).or_insert(0) += 1;
    }

    pub fn mark_complete(&mut self, chat_id: &str) {
        if let Some(active) = self.active_per_chat.get_mut(chat_id) {
            if *active > 0 {
                *active -= 1;
            }
            if *active == 0 {
                self.active_per_chat.remove(chat_id);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}
