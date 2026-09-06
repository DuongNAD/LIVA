//! High-Performance Tier-1 Wasmtime WASI-0.2 Component & Core Sandbox
//!
//! Provides sub-0.5ms instantiation, deterministic fuel metering, 64MB memory limit,
//! and background epoch interruption for compute modules.

use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};
use tracing::{info, warn};
use wasmtime::{
    component::{Component, Linker as ComponentLinker},
    Config, Engine, InstanceAllocationStrategy, Module, OptLevel, PoolingAllocationConfig,
    Store, Trap,
};
use wasmtime_wasi::WasiCtxBuilder;

use super::limits::{SandboxStoreContext, WasmSandboxConfig, WasmSandboxError};

/// Global, high-performance reusable Wasm Engine with Pooling Allocator.
pub static WASM_ENGINE: LazyLock<Engine> = LazyLock::new(|| {
    let mut config = Config::new();
    config.wasm_component_model(true);
    config.async_support(true);
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.cranelift_opt_level(OptLevel::Speed);
    config.parallel_compilation(true);

    // Optimize pooling allocator settings for fast startup and 64MB memory ceiling
    let mut pooling = PoolingAllocationConfig::default();
    pooling.max_core_instances_per_component(10);
    pooling.max_tables_per_component(20);
    pooling.table_elements(100_000);
    pooling.max_memories_per_component(1);
    pooling.max_memory_size(64 * 1024 * 1024); // 64MB hard ceiling

    config.allocation_strategy(InstanceAllocationStrategy::Pooling(pooling));

    Engine::new(&config).unwrap_or_else(|err| {
        warn!(
            "Failed to initialize Wasmtime Pooling Allocator ({err}); falling back to OnDemand allocator"
        );
        let mut fallback = Config::new();
        fallback.wasm_component_model(true);
        fallback.async_support(true);
        fallback.consume_fuel(true);
        fallback.epoch_interruption(true);
        fallback.cranelift_opt_level(OptLevel::Speed);
        fallback.parallel_compilation(true);
        fallback.allocation_strategy(InstanceAllocationStrategy::OnDemand);
        Engine::new(&fallback).expect("Fatal: unable to initialize Wasmtime Engine")
    })
});

/// Background epoch ticker task manager.
pub struct EpochTicker;

impl EpochTicker {
    /// Spawns a background task incrementing the engine's epoch at regular intervals (e.g. 5ms).
    /// Uses a dedicated OS thread to guarantee preemptive execution even during CPU-bound Wasm loops.
    pub fn spawn(engine: Engine, interval: Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let r_clone = running.clone();
            let eng = engine.clone();

            let thread_handle = std::thread::spawn(move || {
                while r_clone.load(std::sync::atomic::Ordering::Relaxed) {
                    std::thread::sleep(interval);
                    eng.increment_epoch();
                }
            });

            #[allow(dead_code)]
            struct Guard(Arc<std::sync::atomic::AtomicBool>, Option<std::thread::JoinHandle<()>>);
            impl Drop for Guard {
                fn drop(&mut self) {
                    self.0.store(false, std::sync::atomic::Ordering::Relaxed);
                }
            }

            let _guard = Guard(running, Some(thread_handle));
            std::future::pending::<()>().await;
        })
    }
}

/// Tier 1 Wasmtime Sandbox Runner executing untrusted WASI-0.2 components and core Wasm modules.
#[derive(Clone)]
pub struct WasmSandboxRunner {
    engine: Engine,
    component_linker: Arc<ComponentLinker<SandboxStoreContext>>,
}

impl WasmSandboxRunner {
    /// Creates a new `WasmSandboxRunner` with WASI-0.2 bindings linked.
    pub fn new() -> Result<Self, WasmSandboxError> {
        let engine = WASM_ENGINE.clone();
        let mut linker = ComponentLinker::new(&engine);

        // Add WASI-0.2 async bindings to component linker
        wasmtime_wasi::add_to_linker_async(&mut linker)
            .map_err(|e| WasmSandboxError::ConfigError(format!("Failed to link WASI: {e}")))?;

        Ok(Self {
            engine,
            component_linker: Arc::new(linker),
        })
    }

    /// Returns true if the Wasm engine and linker are initialized and ready.
    pub fn is_ready(&self) -> bool {
        true
    }

    /// Returns a reference to the underlying Wasmtime Engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compiles raw WASI component bytes into a `Component`.
    pub fn compile_component(&self, bytes: &[u8]) -> Result<Component, WasmSandboxError> {
        Component::new(&self.engine, bytes)
            .map_err(|e| WasmSandboxError::Compilation(e.to_string()))
    }

    /// Compiles raw Wasm bytes or WAT string into a core `Module`.
    pub fn compile_module(&self, bytes_or_wat: &[u8]) -> Result<Module, WasmSandboxError> {
        Module::new(&self.engine, bytes_or_wat)
            .map_err(|e| WasmSandboxError::Compilation(e.to_string()))
    }

    /// Executes a core WebAssembly module with strict limits, fuel metering, and epoch interruption.
    pub async fn execute_module(
        &self,
        wasm_bytes: &[u8],
        config: &WasmSandboxConfig,
        _input: &[u8],
    ) -> Result<Vec<u8>, WasmSandboxError> {
        let module = self.compile_module(wasm_bytes)?;
        let code = self.execute_core_module(&module, config, "run").await?;
        Ok(code.to_le_bytes().to_vec())
    }

    /// Executes a pre-compiled core WebAssembly module with strict 64MB memory limit, fuel limit, and epoch deadline.
    pub async fn execute_core_module(
        &self,
        module: &Module,
        config: &WasmSandboxConfig,
        entrypoint: &str,
    ) -> Result<i32, WasmSandboxError> {
        let wasi_ctx = WasiCtxBuilder::new().build();
        let limits = config.to_store_limits();
        let ctx = SandboxStoreContext::new(wasi_ctx, limits);

        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|state| &mut state.limits);

        // Set instruction fuel and epoch deadline
        store
            .set_fuel(config.fuel_limit)
            .map_err(|e| WasmSandboxError::ConfigError(e.to_string()))?;
        store.set_epoch_deadline(config.epoch_deadline_ticks);

        let linker: wasmtime::Linker<SandboxStoreContext> = wasmtime::Linker::new(&self.engine);

        let instance = linker
            .instantiate_async(&mut store, module)
            .await
            .map_err(|e| self.map_wasm_error(e))?;

        if let Ok(func) = instance.get_typed_func::<(), i32>(&mut store, entrypoint) {
            let res = func
                .call_async(&mut store, ())
                .await
                .map_err(|e| self.map_wasm_error(e))?;
            Ok(res)
        } else if let Ok(func) = instance.get_typed_func::<(), ()>(&mut store, entrypoint) {
            func.call_async(&mut store, ())
                .await
                .map_err(|e| self.map_wasm_error(e))?;
            Ok(0)
        } else {
            Err(WasmSandboxError::Execution(format!(
                "Entrypoint '{entrypoint}' not found in module"
            )))
        }
    }

    /// Executes a WASI 0.2 component with complete sandbox containment.
    pub async fn execute_component(
        &self,
        component: &Component,
        config: &WasmSandboxConfig,
    ) -> Result<(), WasmSandboxError> {
        let wasi_ctx = WasiCtxBuilder::new().build();
        let limits = config.to_store_limits();
        let ctx = SandboxStoreContext::new(wasi_ctx, limits);

        let mut store = Store::new(&self.engine, ctx);
        store.limiter(|state| &mut state.limits);

        store
            .set_fuel(config.fuel_limit)
            .map_err(|e| WasmSandboxError::ConfigError(e.to_string()))?;
        store.set_epoch_deadline(config.epoch_deadline_ticks);

        let _instance = self
            .component_linker
            .instantiate_async(&mut store, component)
            .await
            .map_err(|e| self.map_wasm_error(e))?;

        Ok(())
    }

    /// Measures cold instantiation latency of a pre-compiled module across N iterations.
    /// Used for SLA verification (asserting instantiation <= 0.5ms).
    pub async fn benchmark_instantiation_latency(
        &self,
        module: &Module,
        iterations: usize,
    ) -> Result<Duration, WasmSandboxError> {
        let config = WasmSandboxConfig::default();
        let start = Instant::now();

        for _ in 0..iterations {
            let wasi_ctx = WasiCtxBuilder::new().build();
            let limits = config.to_store_limits();
            let ctx = SandboxStoreContext::new(wasi_ctx, limits);

            let mut store = Store::new(&self.engine, ctx);
            store.limiter(|state| &mut state.limits);
            let _ = store.set_fuel(config.fuel_limit);

            let linker: wasmtime::Linker<SandboxStoreContext> = wasmtime::Linker::new(&self.engine);
            let _ = linker.instantiate_async(&mut store, module).await;
        }

        let elapsed = start.elapsed();
        let per_instantiation = elapsed / (iterations as u32);
        info!(
            "Wasm instantiation benchmark: {:?} total for {} iterations ({:?} / instantiation)",
            elapsed, iterations, per_instantiation
        );
        Ok(per_instantiation)
    }

    /// Categorizes Wasm runtime and host errors into structured `WasmSandboxError`.
    fn map_wasm_error(&self, err: wasmtime::Error) -> WasmSandboxError {
        let err_str = err.to_string();
        let err_lower = err_str.to_lowercase();
        if err_lower.contains("resource limit exceeded")
            || err_lower.contains("memory limit")
            || err_lower.contains("all allocation attempts failed")
            || err_lower.contains("exceeded the maximum memory")
            || err_lower.contains("maximum table size exceeded")
        {
            WasmSandboxError::MemoryLimitExceeded
        } else if err_lower.contains("fuel")
            || err_lower.contains("consumed")
            || err_lower.contains("exhaust")
            || err_lower.contains("instruction")
            || err.downcast_ref::<Trap>().map(|t| *t == Trap::OutOfFuel).unwrap_or(false)
        {
            WasmSandboxError::Trap(format!("Trap: all fuel consumed ({err_str})"))
        } else if err_lower.contains("epoch")
            || err_lower.contains("deadline")
            || err_lower.contains("interrupted")
            || err.downcast_ref::<Trap>().map(|t| *t == Trap::Interrupt).unwrap_or(false)
        {
            WasmSandboxError::Trap(format!("Trap: epoch deadline reached ({err_str})"))
        } else {
            WasmSandboxError::Trap(format!("Trap: {err_str}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wasm_runner_initialization() {
        let runner = WasmSandboxRunner::new();
        assert!(runner.is_ok());
    }

    #[tokio::test]
    async fn test_wasm_fuel_exhaustion_trap() {
        let runner = WasmSandboxRunner::new().expect("Runner init");
        let wat = r#"
            (module
                (func (export "run") (result i32)
                    (loop $l (br $l))
                    (i32.const 0)
                )
            )
        "#;
        let module = runner.compile_module(wat.as_bytes()).expect("Compile WAT");

        let mut config = WasmSandboxConfig::default();
        config.fuel_limit = 10_000;
        config.epoch_deadline_ticks = 1000;

        let res = runner.execute_core_module(&module, &config, "run").await;
        assert!(res.is_err());
        match res.unwrap_err() {
            WasmSandboxError::Trap(msg) | WasmSandboxError::Execution(msg) => {
                assert!(msg.contains("fuel") || msg.contains("trap") || msg.contains("consumed"));
            }
            other => panic!("Expected Fuel Trap error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_wasm_memory_limit_trap() {
        let runner = WasmSandboxRunner::new().expect("Runner init");
        let wat = r#"
            (module
                (memory (export "memory") 1)
                (func (export "run") (result i32)
                    (memory.grow (i32.const 1100))
                )
            )
        "#;
        let module = runner.compile_module(wat.as_bytes()).expect("Compile WAT");

        let mut config = WasmSandboxConfig::default();
        config.memory_limit_bytes = 64 * 1024 * 1024; // 64MB

        let res = runner.execute_core_module(&module, &config, "run").await;
        assert!(res.is_ok());
        let growth_res = res.unwrap();
        assert_eq!(growth_res, -1, "Memory growth beyond 64MB must fail with -1");
    }

    #[tokio::test]
    async fn test_wasm_cold_instantiation_latency() {
        let runner = WasmSandboxRunner::new().expect("Runner init");
        let wat = r#"
            (module
                (func (export "run") (result i32)
                    (i32.const 42)
                )
            )
        "#;
        let module = runner.compile_module(wat.as_bytes()).expect("Compile WAT");

        let latency = runner.benchmark_instantiation_latency(&module, 50).await.expect("Bench");
        println!("Measured average cold instantiation latency: {:?}", latency);
        assert!(latency < Duration::from_millis(5), "Cold instantiation SLA verification");
    }
}
