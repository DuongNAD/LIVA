//! Tier-1 Wasmtime WASI-0.2 Component & Core Sandbox Subsystem
//!
//! Exposes sandboxed execution for WebAssembly components and modules with
//! strict 64MB memory limits, instruction fuel budgets, and epoch deadline timeouts.

pub mod engine;
pub mod limits;

pub use engine::{EpochTicker, WasmSandboxRunner, WASM_ENGINE};
pub use limits::{SandboxStoreContext, WasmSandboxConfig, WasmSandboxError};
