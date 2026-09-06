//! Dual-Tier Sandboxing Engine (Milestone 3 & 4 — RFC-003 R3/R4)
//!
//! Provides defense-in-depth execution isolation:
//! 1. **Tier 1 (Wasmtime WASI-0.2 Component Sandbox)**:
//!    In-process WebAssembly isolation with 64MB memory cap (`StoreLimitsBuilder`),
//!    CPU fuel metering, and background epoch interruption.
//! 2. **Tier 2 (OS Process Containment Matrix)**:
//!    Native process containment executing host commands (`cargo`, `python`, `git`)
//!    inside macOS Seatbelt SBPL profiles, Linux Bubblewrap / Landlock namespaces,
//!    and Windows Win32 Job Objects.
//! 3. **Unified Policy & Security Guardrails**:
//!    Capability token enforcement, monotonic attenuation, path canonicalization,
//!    and network SSRF filters.

pub mod path_validator;
pub mod policy;
pub mod ssrf_filter;
pub mod tier1_wasm;
pub mod tier2_os;

pub use path_validator::CanonicalPathValidator;
pub use policy::{
    validate_command, CapabilityToken, SandboxPolicy, SandboxViolation,
};
pub use ssrf_filter::SsrfFilter;
pub use tier1_wasm::{
    EpochTicker, SandboxStoreContext, WasmSandboxConfig, WasmSandboxError, WasmSandboxRunner,
    WASM_ENGINE,
};
pub use tier2_os::{
    execute_windows_sandbox, JobAccountingStats, OsExecutionResult, OsPlatformBackend,
    OsSandboxEngine, OsSandboxError, OsSandboxPolicy, OsSandboxRunner, WindowsJobConfig,
    WindowsJobSandbox,
};

use std::sync::Arc;

/// Unified Sandboxing Engine coordinating Tier 1 Wasm and Tier 2 OS isolation.
#[derive(Clone)]
pub struct SandboxingEngine {
    wasm_runner: Arc<WasmSandboxRunner>,
    os_runner: Arc<OsSandboxRunner>,
}

impl SandboxingEngine {
    /// Initializes the Dual-Tier Sandboxing Engine.
    pub fn new() -> Result<Self, SandboxViolation> {
        let wasm_runner = WasmSandboxRunner::new()
            .map_err(|e| SandboxViolation::ExecutionError(e.to_string()))?;
        let os_runner = OsSandboxRunner::new();

        Ok(Self {
            wasm_runner: Arc::new(wasm_runner),
            os_runner: Arc::new(os_runner),
        })
    }

    /// Access the Tier 1 Wasm runner.
    pub fn wasm(&self) -> &WasmSandboxRunner {
        &self.wasm_runner
    }

    /// Access the Tier 2 OS runner.
    pub fn os(&self) -> &OsSandboxRunner {
        &self.os_runner
    }
}
