# Original User Request

## Initial Request — 2026-06-08T01:11:16+07:00

Implement and integrate key architectural upgrades into the LIVA engine, focusing on optimizing compute efficiency (Speculative Decoding and Persistent Prompt Caching) and enhancing memory retrieval and compaction (Hybrid Query Routing and Idle-Time Consolidation).

Working directory: e:\project\openclaw_remake
Integrity mode: development

## Requirements

### R1. Compute Engine Upgrades (Speculative Decoding & Prompt Caching)
- Integrate speculative decoding in the local native inference core (using a smaller draft model like Qwen-0.5B alongside the target model).
- Implement persistent system-prompt caching at the gRPC/IPC layer to prevent recalculating the system instructions on subsequent conversational turns, reducing Time-to-First-Token (TTFT) to near-zero.

### R2. Memory Subsystem Upgrades (Hybrid Routing & Background Consolidation)
- Build a hybrid routing layer using a small language model (SLM) or embedding similarity to analyze query intent and direct queries to either the Knowledge Graph (SQLite) or Vector Database (sqlite-vec) to avoid redundant querying.
- Implement an idle-time memory consolidation daemon/cron-job that runs in the background when the system is idle, clustering and summarizing short-term memories from the conversational turns into long-term structured knowledge (meta-entities) and cleaning cache.

### R3. Codebase Integration & Compliance
- Ensure all additions comply strictly with LIVA's code architecture constraints defined in `AI_CONTEXT.md` (e.g., no blocked event loops, use of `safeFetch`, SQLite debounced writes, Pino logging, and TypeScript strict safety).
- Ensure existing Tauri and Gateway functionalities remain fully operational and backward-compatible.

## Acceptance Criteria

### Compute Engine Verification
- [ ] Speculative decoding can be toggled on/off via configuration. When enabled, token generation speed (tokens/second) shows a measurable increase (at least 1.3x speedup on standard prompts).
- [ ] System prompt KV cache is successfully retained across consecutive messages, showing a TTFT of under 100ms on subsequent turns.

### Memory Subsystem Verification
- [ ] The hybrid routing layer successfully classifies user intents into target data sources (e.g., code/project logic -> Knowledge Graph; conversational memory/old experience -> Vector DB) with at least 90% accuracy on a test suite of 20 benchmark prompts.
- [ ] The idle consolidation job activates after a configured idle interval, correctly clusters short-term memory nodes, updates the SQLite DB, and does not leak memory or block the main Node.js event loop.

### Integration Verification
- [ ] All existing and new integration tests in the gateway build and pass successfully (`npm run test:gateway`).
- [ ] An automated verification script (`npm run test:upgrades` or similar) is provided in the repository to programmatically verify the speculative decoding throughput and routing accuracy.

## Follow-up — 2026-06-08T09:05:13+07:00

Verify, optimize, and align the latest LIVA features from the `main` branch (including Speculative Decoding, Prompt Caching, Preemptive VRAM Mutex, and Memory Dreaming Pipeline) for seamless execution on macOS (Apple Silicon).

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. macOS Verification of Speculative Decoding
- Ensure that speculative decoding (`LIVA_ENABLE_SPECULATIVE=true`) functions correctly on macOS without causing context allocation errors or crashing llama-server/python-native-engine.
- The agent team may automatically detect or download a lightweight draft model (e.g. Gemma-2B GGUF) if needed for verification.
- Optimize thread allocation for the draft model vs. the main model on macOS to prevent efficiency-core thrashing.

### R2. macOS Prompt Caching & KV Cache Pruning
- Validate that prompt caching (prefix matching) and sliding-window KV cache pruning run stably during long conversations under macOS unified memory limits.

### R3. Preemptive VRAM Mutex & VRAM Yielding on macOS
- Verify that the new `PreemptiveVramMutex` and `VRAMGuard` yield local GPU memory correctly on macOS when heavy apps are detected, routing all traffic to the fallback API.
- Ensure macOS-specific professional apps (Xcode, Android Studio, Blender, VS Code, DaVinci Resolve) are correctly whitelisted and monitored.

### R4. Memory Dreaming & Idle Consolidation
- Confirm that the `MemoryDreamingPipeline` and `ConsolidationCron` run successfully as background tasks on macOS without event-loop blocking or performance degradation.

## Acceptance Criteria

### Execution & Stability
- [ ] Speculative decoding runs on Apple Silicon (Metal) with the draft model loading successfully.
- [ ] No VRAM/RAM out-of-memory or SIGBUS crashes occur during long chat sessions with active prompt caching and pruning.
- [ ] VRAM Guard correctly detects macOS professional apps (Xcode, Android Studio, Blender, VS Code, DaVinci Resolve) and yields local VRAM, successfully reclaims it on exit.
- [ ] All unit and integration tests for the new features (speculative decoding, preemptive VRAM mutex, dreaming pipeline) pass on macOS.

## Follow-up — 2026-06-08T16:02:41+07:00

Research, optimize, and upgrade LIVA's local inference engine on macOS, focusing on maximizing Metal GPU acceleration for Gemma 4, and prototyping/integrating Apple's MLX framework for state-of-the-art Apple Silicon performance.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. macOS Gemma 4 Inference Optimization
- Benchmark and optimize local Gemma 4 model execution speed (tokens/sec) using llama.cpp with Metal GPU acceleration.
- Tune thread counts (P-core capping), batch size (`n_batch`), micro-batch size (`n_ubatch`), and memory mapping (`mmap`) settings for Metal.

### R2. Apple MLX Framework Integration
- Research and prototype integration of Apple's MLX framework (`mlx` / `mlx-lm`) as an alternative high-performance local inference backend for Gemma 4.
- Provide a comparative analysis of throughput (prefill & decode tokens/sec), memory bandwidth usage, and latency between llama.cpp/Metal and MLX on Apple Silicon.

### R3. Unified Memory and OS Tuning
- Implement hardware-adaptive thread adjustments and memory protection triggers based on real-time OS memory pressure and CPU load under macOS.
- Ensure all logging and database operations remain asynchronous and do not block the Node.js event loop.

## Acceptance Criteria

### Performance & MLX Integration
- [ ] Deliver a working prototype or option inside LIVA's engine to execute Gemma 4 inference using the Apple MLX framework.
- [ ] Generate a comparative performance report detailing tokens/sec and memory overhead for both llama.cpp/Metal and MLX backends.
- [ ] Confirm no memory leaks or zombie processes exist in either engine backend under continuous conversational load.
- [ ] All existing vitest/pytest tests pass successfully on the `mac` branch.

## Follow-up — 2026-06-08T18:06:35+07:00

Hi! I see you are active and working on the macOS porting and optimization. Can you provide a summary of the current status of Milestone M17 (MLX Alternative Backend Prototype), including details on the latest integration, comparative speed results, and any outstanding tasks?

## Follow-up — 2026-06-08T21:03:28+07:00

Research, optimize, and upgrade LIVA's local inference engine on macOS, focusing on maximizing Metal GPU acceleration for Gemma 4, and prototyping/integrating Apple's MLX framework for state-of-the-art Apple Silicon performance.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. macOS Gemma 4 Inference Optimization
- Benchmark and optimize local Gemma 4 model execution speed (tokens/sec) using llama.cpp with Metal GPU acceleration.
- Tune thread counts (P-core capping), batch size (`n_batch`), micro-batch size (`n_ubatch`), and memory mapping (`mmap`) settings for Metal.

### R2. Apple MLX Framework Integration
- Research and prototype integration of Apple's MLX framework (`mlx` / `mlx-lm`) as an alternative high-performance local inference backend for Gemma 4.
- Provide a comparative analysis of throughput (prefill & decode tokens/sec), memory bandwidth usage, and latency between llama.cpp/Metal and MLX on Apple Silicon.

### R3. Unified Memory and OS Tuning
- Implement hardware-adaptive thread adjustments and memory protection triggers based on real-time OS memory pressure and CPU load under macOS.
- Ensure all logging and database operations remain asynchronous and do not block the Node.js event loop.

## Acceptance Criteria

### Performance & MLX Integration
- [ ] Deliver a working prototype or option inside LIVA's engine to execute Gemma 4 inference using the Apple MLX framework.
- [ ] Generate a comparative performance report detailing tokens/sec and memory overhead for both llama.cpp/Metal and MLX backends.
- [ ] Confirm no memory leaks or zombie processes exist in either engine backend under continuous conversational load.
- [ ] All existing vitest/pytest tests pass successfully on the `mac` branch.
