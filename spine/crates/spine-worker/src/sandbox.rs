//! Resource sandboxing for tool execution.
//!
//! The sandbox enforces resource limits on tool execution.

/// Resource limits for execution.
#[derive(Debug, Clone, Copy)]
pub struct ResourceLimits {
    /// Maximum CPU time in seconds
    pub cpu_time_secs: u64,
    /// Maximum memory in MB
    pub memory_mb: u64,
    /// Maximum output size in KB
    pub output_kb: u64,
    /// Maximum number of processes
    pub max_pids: u32,
    /// Maximum file size in MB
    pub max_file_size_mb: u64,
}

impl ResourceLimits {
    /// Create limits for quick operations.
    pub fn quick() -> Self {
        Self {
            cpu_time_secs: 5,
            memory_mb: 64,
            output_kb: 64,
            max_pids: 10,
            max_file_size_mb: 10,
        }
    }

    /// Create limits for standard operations.
    pub fn standard() -> Self {
        Self {
            cpu_time_secs: 60,
            memory_mb: 512,
            output_kb: 1024,
            max_pids: 100,
            max_file_size_mb: 100,
        }
    }

    /// Create limits for heavy operations.
    pub fn heavy() -> Self {
        Self {
            cpu_time_secs: 300,
            memory_mb: 2048,
            output_kb: 8192,
            max_pids: 500,
            max_file_size_mb: 500,
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::standard()
    }
}

/// A sandbox for isolated execution.
#[derive(Debug)]
pub struct Sandbox {
    limits: ResourceLimits,
}

impl Sandbox {
    /// Create a new sandbox with the given limits.
    pub fn new(limits: ResourceLimits) -> Self {
        Self { limits }
    }

    /// Get the resource limits.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Check if an operation would exceed limits.
    pub fn check_operation(
        &self,
        estimated_memory: u64,
        estimated_time: u64,
    ) -> Result<(), SandboxError> {
        if estimated_memory > self.limits.memory_mb {
            return Err(SandboxError::MemoryLimitExceeded {
                requested: estimated_memory,
                limit: self.limits.memory_mb,
            });
        }
        if estimated_time > self.limits.cpu_time_secs {
            return Err(SandboxError::TimeLimitExceeded {
                requested: estimated_time,
                limit: self.limits.cpu_time_secs,
            });
        }
        Ok(())
    }

    /// Check if file size is within limits.
    pub fn check_file_size(&self, size_mb: u64) -> Result<(), SandboxError> {
        if size_mb > self.limits.max_file_size_mb {
            return Err(SandboxError::FileSizeExceeded {
                size: size_mb,
                limit: self.limits.max_file_size_mb,
            });
        }
        Ok(())
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new(ResourceLimits::default())
    }
}

/// Sandbox violation errors.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum SandboxError {
    #[error("memory limit exceeded: requested {requested} MB, limit {limit} MB")]
    MemoryLimitExceeded { requested: u64, limit: u64 },
    #[error("time limit exceeded: requested {requested} s, limit {limit} s")]
    TimeLimitExceeded { requested: u64, limit: u64 },
    #[error("file size exceeded: {size} MB, limit {limit} MB")]
    FileSizeExceeded { size: u64, limit: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_enforces_memory_limit() {
        let sandbox = Sandbox::new(ResourceLimits::quick());
        assert!(sandbox.check_operation(100, 1).is_err());
        assert!(sandbox.check_operation(32, 1).is_ok());
    }

    #[test]
    fn sandbox_enforces_time_limit() {
        let sandbox = Sandbox::new(ResourceLimits::quick());
        assert!(sandbox.check_operation(1, 10).is_err());
        assert!(sandbox.check_operation(1, 3).is_ok());
    }

    #[test]
    fn sandbox_enforces_file_size() {
        let sandbox = Sandbox::new(ResourceLimits::standard());
        assert!(sandbox.check_file_size(200).is_err());
        assert!(sandbox.check_file_size(50).is_ok());
    }

    #[test]
    fn limits_scale_properly() {
        let quick = ResourceLimits::quick();
        let standard = ResourceLimits::standard();
        let heavy = ResourceLimits::heavy();

        assert!(quick.memory_mb < standard.memory_mb);
        assert!(standard.memory_mb < heavy.memory_mb);
    }
}
