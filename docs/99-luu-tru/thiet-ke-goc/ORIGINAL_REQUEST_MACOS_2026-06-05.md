# Original User Request

## Initial Request — 2026-06-05T09:48:11Z

Fixing, upgrading, and optimizing the LIVA cognitive system to run seamlessly on macOS, utilizing Apple Silicon (Metal) acceleration for LLM inference, specifically optimized for the Gemma 4 12B model (`gemma-4-12B-it-Q6_K.gguf`).

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. macOS Native Inference Engine Compatibility
- Resolve the Windows-specific DLL loading logic in `liva_native_engine.py` (e.g., `llama.dll`, `ctypes.windll`, and `os.add_dll_directory` which are Windows-specific).
- Enable support for loading `libllama.dylib` (or `libllama.so`) on macOS.
- Ensure that the process-polling and thread-management calls remain robust and don't throw platform errors on macOS.

### R2. macOS Native Compilation Pipeline
- Create a macOS-compatible compilation pipeline (e.g., `scripts/build_llama_mac.sh` or a unified cross-platform python build script) to replace `liva_first_run_build.ps1` for building `llama.cpp`.
- The compilation pipeline must build both `libllama.dylib` and `llama-server` with Apple Silicon (Metal) acceleration (`GGML_METAL=ON`).
- Stage the compiled binaries (`libllama.dylib` and `llama-server`) to `liva-ai-engine/native_lib/`.

### R3. macOS Startup and Lifecycle Management
- Verify and refine the `scripts/start_all.sh` script to orchestrate all LIVA processes (Whisper STT, Voice Engine, Gateway, UI, and Tauri Desktop Shell) on macOS.
- Ensure that port conflicts are resolved, virtual environment setups are handled gracefully, and services are correctly terminated upon exit.

### R4. Gemma 4 12B Native Optimization
- Ensure that the Native Inference Engine is optimized for running the `gemma-4-12B-it-Q6_K.gguf` model on Apple Silicon (M1/M2/M3/M4) via Metal.
- Enable support for `LIVA_USE_NATIVE=true` on macOS to achieve zero HTTP/REST overhead for LLM and embedding generation.
- Ensure the KV cache uses 4-bit quantization (GGML_TYPE_Q4_0) to fit the 12B model context efficiently in macOS unified memory.

### R5. Automated Testing and Verification
- Ensure that existing Python tests (`liva-ai-engine/tests`) and gateway tests (`liva-gateway`) run and pass successfully on macOS.

## Acceptance Criteria

### Build & Run
- [ ] The compilation pipeline runs to completion on macOS and outputs the required binaries to `native_lib/`.
- [ ] `start_all.sh` successfully launches all 5 services without throwing shell or environment errors.
- [ ] llama-server runs with Metal (Apple Silicon) GPU acceleration active when executing queries.

### Functional Verification
- [ ] The gRPC connection between the LIVA Gateway (Node.js) and LIVA Native Engine (Python) is successfully established with `LIVA_USE_NATIVE=true`.
- [ ] All unit and integration tests run successfully.
- [ ] The Gemma 4 12B model loads successfully and generates responses via the native gRPC engine.

## Follow-up — 2026-06-06T07:51:37+07:00

Identify and resolve the LIVA gateway startup/connectivity issue on macOS, where users experience a premature "Hệ thống AI lỗi đang bận xử lý..." circuit-breaker message because the local GGUF model takes longer to load than the gateway's hardcoded 10-second wait limit.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Root Cause Resolution
The system must wait for the local gRPC Python Native Engine to fully load the GGUF model (which can take 30–90 seconds on macOS depending on system resources) instead of timing out and triggering the circuit breaker after only 10 seconds.

### R2. Seamless Startup Behavior
Ensure that during model startup, the gateway waits dynamically or does not route user requests to a circuit breaker state if the engine is actively launching and warming up.

## Acceptance Criteria

### Correct Startup Behavior
- [ ] The gateway boot loop waits up to a reasonable timeout (e.g. 90 seconds) for the Python Native Engine to report healthy status.
- [ ] Initial user messages sent after startup do not trigger the "Hệ thống AI lỗi..." circuit breaker unless the model actually fails to boot after the extended timeout.
- [ ] No regression of existing features or tests (e.g. all 2300+ unit tests continue to pass).

## Follow-up — 2026-06-06T13:14:54+07:00

Resolve the macOS native engine SIGBUS crash during prefill of large prompts and the gateway credentials crash when fallback AI is not configured.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Configurable Memory Mapping (use_mmap)
- Make `use_mmap` configurable via an environment variable `NATIVE_USE_MMAP` in the Python Native Engine (`liva_native_engine.py`).
- On macOS, `use_mmap` should default to `False` under memory pressure to avoid SIGBUS crashes during prefill of large prompts. On other platforms, it can default to `True`.
- Allow overriding via the environment variable (e.g., `NATIVE_USE_MMAP=true` or `NATIVE_USE_MMAP=false`).

### R2. Configurable GPU Layers
- Document `NATIVE_N_GPU_LAYERS` in the gateway `.env.example` to allow users to specify the number of layers offloaded to GPU (defaulting to `-1` for all layers), helping manage Metal memory usage.

### R3. Safe Fallback Client Initialization
- In the gateway `AgentLoop.ts`, before instantiating the cloud fallback client (`OpenAI`), check if the fallback credentials (`FALLBACK_AI_API_KEY` and `FALLBACK_AI_BASE_URL`) are configured.
- If they are empty or unconfigured, do not instantiate `OpenAI` to avoid throwing a raw credentials initialization error.
- Instead, throw a clean custom error indicating that the local engine is offline/restarting and no cloud fallback is configured.
- Catch this error in the user input execution loop, treat it as a connection/recovery issue to trigger local engine recovery (`restartRouter`), and display a friendly waiting message to the user rather than crashing the chat thread.

## Acceptance Criteria

### Correct Execution and Recovery
- [ ] Processing a prompt of 2000+ tokens on macOS does not trigger a `SIGBUS` crash when `NATIVE_USE_MMAP=false` is configured.
- [ ] Instantiating fallback client when `FALLBACK_AI_API_KEY` is not configured does not throw a raw `Missing credentials` error that crashes the gateway.
- [ ] A friendly offline/rewarming message is shown to the user when the local engine is offline and fallback is not configured.
- [ ] No regression of existing features or tests (e.g., all 2300+ gateway unit tests pass).

## Follow-up — 2026-06-08T00:33:35+07:00

Research why token generation is extremely slow when running LIVA on macOS (Apple Silicon) and optimize the engine's performance, hardware utilization, and configurations.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Root Cause Analysis
Investigate and document the specific bottlenecks causing slow token generation on macOS (including CPU/GPU thread coordination, Metal acceleration status, ctypes binding overhead, thread contention, and memory pressure).

### R2. Build and Runtime Optimization
Optimize the C++ compilation flags (e.g., Accelerate framework, Metal compiler settings) and Python ctypes binding/runtime configurations (e.g., CPU thread counts, batch sizes, KV cache settings) to maximize performance on Apple Silicon.

### R3. Verification and Benchmarking
Verify token generation speed improvements using objective metrics (tokens per second) and compare performance before and after optimization.

## Acceptance Criteria

### Performance Benchmark
- [ ] Token generation speed on macOS (Apple Silicon) is significantly improved (target: matching optimal hardware limits, aiming for 15-20+ tokens per second for Gemma-12B or equivalent).
- [ ] GPU (Metal) hardware acceleration is verified to be fully active during inference.
- [ ] Thread count optimization is completed, eliminating CPU thread contention and efficiency-core bottlenecking.

## Follow-up — 2026-06-07T17:49:11Z

Hi Sentinel! The baseline llama-bench runs are complete. I have saved the results to .agents/orchestrator_gen3_1/baseline_benchmark.md and .agents/teamwork_preview_explorer_m7_3/baseline_benchmark.md. Please instruct the active orchestrator (orchestrator_gen3_1) and Explorer 3 (e84867f2-563e-4c82-b09c-b2b562189a7a) to read these files to complete their investigation milestone without running new benchmarks.
