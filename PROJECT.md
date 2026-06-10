# Project: LIVA macOS and Gemma 4 Compatibility Upgrade

## Architecture
LIVA is a hybrid-intelligence, multi-agent AI desktop assistant. On macOS (Apple Silicon), it runs native local inference using a compiled C++ `llama.cpp` engine.
The core communication loop when `LIVA_USE_NATIVE=true` is:
1. `liva-gateway` (Node.js) boots.
2. `ModelOrchestrator` (TypeScript) checks if the native python gRPC server is online. If not, it spawns `liva_native_engine.py` (Python).
3. `liva_native_engine.py` loads the compiled shared library `libllama.dylib` via `ctypes`.
4. `liva-gateway` communicates with `liva_native_engine.py` via gRPC (port 8100).
5. `liva_native_engine.py` manages loading and unloading of models (such as `gemma-4-12B-it-Q6_K.gguf`) into VRAM and handles raw inference with Metal GPU acceleration.

```
┌─────────────────┐       gRPC       ┌────────────────────────┐     ctypes      ┌────────────────────┐
│  liva-gateway   │ ◄──────────────► │  liva_native_engine.py │ ◄─────────────► │ libllama.dylib     │
│  (Node.js Core) │    (Port 8100)   │     (Python Daemon)    │                 │ (C++ llama.cpp)    │
└─────────────────┘                  └────────────────────────┘                 └────────────────────┘
```

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | M1: macOS Native Compilation Pipeline | Create `scripts/build_llama_mac.sh` to compile `libllama.dylib` and `llama-server` with Metal (`GGML_METAL=ON`) and stage to `liva-ai-engine/native_lib/`. | None | DONE |
| 2 | M2: macOS Native Inference Engine Compatibility | Modify `liva-ai-engine/liva_native_engine.py` to support `libllama.dylib` loading on macOS, bypass Windows-only APIs (DLL search path, winmode), and check macOS platform compatibility. | M1 | DONE |
| 3 | M3: macOS Startup and Lifecycle | Refine `scripts/start_all.sh` to run ports cleanup, virtual environment check, microservices, and graceful daemon teardown on macOS. | M2 | DONE |
| 4 | M4: Gemma 4 12B Configuration & gRPC Integration | Configure model names (`gemma-4-12B-it-Q6_K.gguf`) and context quantization settings (4-bit KV Cache) in the env/native engine, verifying gRPC chat responses. | M3 | DONE |
| 5 | M5: E2E and Unit Verification Testing | Run python pytest and vitest gateway suites. Perform white-box coverage hardening using adversarial test cases. | M4 | DONE |
| 6 | M6: macOS Crash Mitigation & Safe Fallback Recovery | Implement memory pressure checks to conditionally disable mmap, dynamic wait loops during engine warmups/swaps, and graceful fallback recovery when local engines are offline. | M5 | DONE |
| 7 | M7: Bottleneck Investigation & Root Cause Analysis | Research CPU/GPU thread coordination, Metal acceleration, and ctypes binding overhead on macOS. | M6 | DONE |
| 8 | M8: Build Pipeline Optimization | Rebuild libllama.dylib & llama-server with optimized compilation flags. | M7 | DONE |
| 9 | M9: Python Engine Runtime Optimization | Optimize thread counts (P-core pinning), batch sizes, and memory usage. | M8 | DONE |
| 10| M10: Performance Verification & Regressions | Run benchmark suite, verify target throughput, and run full test suites. | M9 | DONE |
| 11| M11: Speculative Decoding Alignment & Thread Optimization | Modify `liva_native_engine.py` to partition threads for draft model (`max(2, n_threads // 2)`) and main model, avoiding E-core thrashing. | None | DONE |
| 12| M12: KV Cache Prefix Matching & Pruning Optimization | Verify prefix matching and sliding-window KV cache pruning run in lockstep for both main and draft contexts on macOS. | M11 | DONE |
| 13| M13: Preemptive VRAM Mutex & VRAMGuard macOS Porting | Replace Windows-specific `tasklist` process monitoring in Python's `vram_guard_loop` with a macOS `ps`-based check. Align Gateway's VRAM guard logic. | M12 | DONE |
| 14| M14: Memory Dreaming Pipeline & Consolidation Cron Optimization | Verify background dreaming pipeline and consolidation cron schedule execution does not block Node.js event loop on macOS. | M13 | DONE |
| 15| M15: Integrated E2E Verification & Hardening | Execute full test suites (`vitest` in gateway and `pytest` in python engine) to verify complete feature alignment and Metal compatibility. | M14 | DONE |
| 16| M16: Gemma 4 E4B Sequential Hot-Swap Integration | Download the Gemma 4 E4B model and configure it as the active Router model in LIVA's hot-swap architecture, and verify/profile swapping flow, cooldown, and performance on macOS. | M15 | DONE |


## Interface Contracts
### Gateway ↔ Native Engine (gRPC on Port 8100)
- Protocol defined in: `liva-gateway/src/proto/liva_engine.proto`
- `liva_native_engine.py` compiles gRPC bindings using `grpc_tools.protoc` at startup/build time.
- Standard methods:
  - `HealthCheck`: verifies model load status and engine viability.
  - `Chat`: handles streaming text generation.
  - `SwapModel`: swaps GGUF models in/out of VRAM (Router ↔ Expert).

## Code Layout
- `liva-ai-engine/liva_native_engine.py` — Native Python inference engine using ctypes.
- `liva-ai-engine/native_lib/` — Destination directory for compiled C++ shared libraries and executables.
- `scripts/build_llama_mac.sh` — Compilation script for macOS.
- `scripts/start_all.sh` — System orchestrator startup script for macOS.
- `liva-gateway/src/core/ModelOrchestrator.ts` — Gateway agent brain controller for spawning the native engine.
