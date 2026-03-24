use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPolicyConfig {
    pub allow_shell: bool,
    pub allow_network: bool,
    pub allow_filesystem: bool,
    pub writable_roots: Vec<String>,
}

impl Default for ToolPolicyConfig {
    fn default() -> Self {
        Self {
            allow_shell: true,
            allow_network: true,
            allow_filesystem: true,
            writable_roots: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_concurrency: usize,
    pub worker_timeout: u64,
    pub tool_timeout_secs: u64,
    pub max_queue: usize,
    pub max_active_per_chat: usize,
    pub remote_threshold_chars: usize,
    pub remote_workers: Vec<String>,
    pub remote_auth_token: Option<String>,
    pub remote_retries: usize,
    pub tool_policy: ToolPolicyConfig,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 8,
            worker_timeout: 1800,
            tool_timeout_secs: 60,
            max_queue: 1024,
            max_active_per_chat: 1,
            remote_threshold_chars: 500,
            remote_workers: Vec::new(),
            remote_auth_token: None,
            remote_retries: 3,
            tool_policy: ToolPolicyConfig::default(),
        }
    }
}
