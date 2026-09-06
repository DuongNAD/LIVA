//! Tier-1 Wasmtime Resource Limiting, Store Limits & Context Definitions
//!
//! Enforces hardware-level sandboxing constraints:
//! - 64MB RAM hard ceiling via `StoreLimitsBuilder`.
//! - Instruction fuel budgeting (deterministic CPU throttling).
//! - Background epoch deadline interrupt (500ms wall-clock SLA).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use wasmtime::{StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiView};

/// Default hardware resource caps for Tier 1 Wasm execution.
pub const DEFAULT_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024; // 64 MB
pub const DEFAULT_INITIAL_FUEL: u64 = 100_000_000;            // 100 Million instructions
pub const DEFAULT_EPOCH_DEADLINE_TICKS: u64 = 10;             // 10 ticks
pub const DEFAULT_MAX_TABLE_ELEMENTS: usize = 100_000;

/// Configuration parameters for Wasmtime sandbox instance execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmSandboxConfig {
    /// Hard memory cap in bytes (default: 64MB).
    pub memory_limit_bytes: usize,
    /// Instruction fuel budget allocated per invocation.
    pub fuel_limit: u64,
    /// Wall-clock epoch deadline ticks before asynchronous interruption.
    pub epoch_deadline_ticks: u64,
    /// Allowed outbound host network names.
    pub allowed_hosts: Vec<String>,
    /// Allowed filesystem paths.
    pub allowed_paths: Vec<PathBuf>,
}

impl Default for WasmSandboxConfig {
    fn default() -> Self {
        Self {
            memory_limit_bytes: DEFAULT_MAX_MEMORY_BYTES,
            fuel_limit: DEFAULT_INITIAL_FUEL,
            epoch_deadline_ticks: DEFAULT_EPOCH_DEADLINE_TICKS,
            allowed_hosts: vec![],
            allowed_paths: vec![],
        }
    }
}

impl WasmSandboxConfig {
    /// Builds `wasmtime::StoreLimits` enforcing the memory ceiling and table limits.
    pub fn to_store_limits(&self) -> StoreLimits {
        StoreLimitsBuilder::new()
            .memory_size(self.memory_limit_bytes)
            .table_elements(DEFAULT_MAX_TABLE_ELEMENTS)
            .instances(1)
            .memories(1)
            .tables(10)
            .build()
    }

    /// Alias for `to_store_limits`.
    pub fn build_store_limits(&self) -> StoreLimits {
        self.to_store_limits()
    }
}

/// Store context encapsulating WASI state and resource limits.
pub struct SandboxStoreContext {
    pub wasi_ctx: WasiCtx,
    pub table: ResourceTable,
    pub limits: StoreLimits,
}

impl SandboxStoreContext {
    pub fn new(wasi_ctx: WasiCtx, limits: StoreLimits) -> Self {
        Self {
            wasi_ctx,
            table: ResourceTable::new(),
            limits,
        }
    }
}

impl WasiView for SandboxStoreContext {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi_ctx
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

/// Errors occurring during Wasm sandboxed execution.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WasmSandboxError {
    #[error("RAM memory limit exceeded (64MB ceiling)")]
    MemoryLimitExceeded,

    #[error("Execution fuel exhausted (CPU instruction budget exceeded)")]
    OutOfFuel,

    #[error("Execution timed out (epoch deadline reached)")]
    ExecutionTimeout,

    #[error("Component/Module compilation failed: {0}")]
    Compilation(String),

    #[error("Instantiation failed: {0}")]
    Instantiation(String),

    #[error("Execution failed: {0}")]
    Execution(String),

    #[error("Trap encountered during execution: {0}")]
    Trap(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limits_config() {
        let config = WasmSandboxConfig::default();
        assert_eq!(config.memory_limit_bytes, 64 * 1024 * 1024);
        assert_eq!(config.fuel_limit, 100_000_000);
        assert_eq!(config.epoch_deadline_ticks, 10);

        let _limits = config.to_store_limits();
    }
}
