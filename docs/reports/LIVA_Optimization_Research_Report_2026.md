# LIVA System Comprehensive Optimization & Upgrade Research Report

**Date:** 2026-06-10  
**Author:** LIVA Optimization Team (Orchestrator Gen 2)  
**Version:** 1.0.0  

---

## Executive Summary
This research report provides a deep-dive audit and optimization roadmap for the LIVA hybrid-intelligence, multi-agent AI desktop assistant. The audit covers all four major LIVA components (`liva-gateway`, `liva-ui`, `liva-desktop`, and `liva-ai-engine`) on macOS (Apple Silicon). We identify **21 unique optimization opportunities** across performance, memory footprint (RAM/VRAM), reliability, and architecture, complete with concrete strategies and code blueprints.

---

## Part 1: Optimization Opportunities Matrix (21 Unique Opportunities)

### 1. `liva-gateway` (Node.js/TypeScript Backend)

#### Opportunity 1: Non-Blocking Memory Dreaming Pipeline
*   **Problem:** The current `MemoryDreamingPipeline` executes log parsing and SHA-256 hashing synchronously on Node.js's main thread. As session log files grow, this blocks the event loop for hundreds of milliseconds, freezing incoming TCP/gRPC requests.
*   **Solution:** Refactor log processing using asynchronous chunking (`AsyncChunker.processNonBlocking` yielding control every 100 lines) or offload to a dedicated `worker_threads` instance.
*   **Impact:** P0 - Prevents main thread freeze during background dreaming execution.

#### Opportunity 2: SQLite Write-Ahead Logging (WAL) and `sqlite-vec` Tuning
*   **Problem:** Synchronous/blocking disk writes in SQLite database queries degrade RAG vector retrieval throughput.
*   **Solution:** Configure SQLite to use `journal_mode = WAL`, `synchronous = NORMAL`, and `wal_autocheckpoint = 500` to separate read and write logs, boosting write throughput.
*   **Impact:** P1 - Substantially increases concurrent read/write query performance.

#### Opportunity 3: SQLite Memory-Mapped I/O
*   **Problem:** High system call overhead (`read`/`write` kernel transitions) when querying SQLite DB pages.
*   **Solution:** Set `PRAGMA mmap_size = 268435456` (256MB) to map the entire database file into the gateway's virtual memory address space.
*   **Impact:** P1 - Reduces OS context switching and latency for SQLite DB reads to near 0ms.

#### Opportunity 4: Standardized RAG Ingestion Pipeline with SQLite FTS5 Unicode61 Tokenizer
*   **Problem:** Lack of a standardized ingestion pipeline causes inconsistent chunking, while standard FTS search lacks proper Unicode text segmentation.
*   **Solution:** Implement a unified `RAGIngestionPipeline` singleton that coordinates Markdown/text chunking and utilizes SQLite FTS5 with the `unicode61` tokenizer for robust text search.
*   **Impact:** P1 - Ensures precise document retrieval and hybrid search correctness.

#### Opportunity 5: Preemptive VRAM Mutex & Graduated Degradation
*   **Problem:** Sudden CUDA/Metal Out-of-Memory (OOM) errors due to concurrent WebGL rendering and heavy LLM inference.
*   **Solution:** Implement `PreemptiveVramMutex` to govern model loading. Add a 3-step graduated degradation model: Eco Mode (5 FPS), Freeze Mode (0 FPS WebGL rendering), and Preempt Mode (forcibly terminate low-priority tasks and unload Expert model).
*   **Impact:** P0 - Prevents system crashes and OOM faults.

#### Opportunity 6: Fact Touch Buffer for Debouncing Writes
*   **Problem:** Direct SQL `UPDATE` queries to disk for every accessed Fact degrade SSD lifespan and increase event loop latency.
*   **Solution:** Implement `#factTouchBuffer` to queue metadata updates in memory, and flush them in a batch transaction every 60s or upon gateway shutdown.
*   **Impact:** P2 - Reduces SQLite write-amplification and I/O congestion.

#### Opportunity 7: Database Worker Watchdog and Deadlock Recovery
*   **Problem:** CPU-intensive vector calculations or locked DB files can block the database worker thread permanently.
*   **Solution:** Add a watchdog loop that pings the worker thread every 10s. If no reply is received within 25s, kill and respawn the worker thread (up to 3 times).
*   **Impact:** P1 - Ensures high reliability and resilience to deadlock states.

#### Opportunity 8: safeRename Atomic Write Protocol
*   **Problem:** Sudden power failures or antivirus scanning locks during file writes can corrupt user configuration or model states.
*   **Solution:** Implement atomic writes: write to `.tmp` files first, then execute a `safeRename` with exponential backoff retries.
*   **Impact:** P1 - Eliminates state file corruption risks.

#### Opportunity 9: Database Self-Healing Recovery
*   **Problem:** File system corruption can make the SQLite database unreadable.
*   **Solution:** Run `PRAGMA integrity_check` on startup. If corrupted, automatically restore from `<dbPath>.bak` (created during successful consolidation runs) and remove auxiliary WAL files.
*   **Impact:** P1 - Self-heals from disk corruption.

#### Opportunity 10: Multi-Tier Cache Layering
*   **Problem:** Redundant calculations of prompt formats, tool descriptions, and user profile reads degrade latency.
*   **Solution:** Implement structured caching with `lru-cache`: `memCache` (chat history), `hybridCache` (RAG vector results), `profileCache` (SWR for user profile), `descEmbeddingCache` (tool embeddings), and `#promptCache` (sealed system prompts).
*   **Impact:** P2 - Reduces end-to-end response time for cached flows.

---

### 2. `liva-ui` (Vue 3 / WebGL Frontend)

#### Opportunity 11: Deep Disposal Lifecycle for WebGL 3D Model
*   **Problem:** Unloading 3D models without releasing GPU buffers leads to massive browser VRAM leakage, eventually crashing the UI.
*   **Solution:** Implement `deepDispose` to recursively release geometries, materials, and textures, and call `renderer.forceContextLoss()` to immediately drop the WebGL context.
*   **Impact:** P0 - Fixes UI memory leaks on avatar hotswaps.

#### Opportunity 12: WebGL Render Loop Throttling
*   **Problem:** Constant WebGL render loops at 60 FPS drain battery and consume GPU cycles even when the window is hidden or memory is constrained.
*   **Solution:** Throttle render loop: 60 FPS (active window), 15 FPS (background tab via `visibilitychange`), 5 FPS (Eco Mode), and 0 FPS (Freeze Mode).
*   **Impact:** P1 - Saves GPU capacity and system battery.

#### Opportunity 13: Viseme-Mapped Lip-Sync Synchronization
*   **Problem:** Amplitude-based mouth movement is stiff and does not match phonetic spoken words.
*   **Solution:** Implement Real-time audio frequency analysis using Web Audio API `AnalyserNode` mapped to 5 visemes ('aa', 'oh', 'ee', 'ih', 'ou') with dead zone noise gates and smoothing coefficients.
*   **Impact:** P2 - Delivers natural-looking mouth movements.

#### Opportunity 14: Spring-Damped LookAt Damping
*   **Problem:** Jittery camera tracking data causes the avatar's eyes and head to shake erratically.
*   **Solution:** Apply a spring-damped interpolation formula: $\theta_{new} = \theta_{old} + (\theta_{target} - \theta_{old}) \cdot (1 - 0.001^{\Delta t})$ to filter yaw/pitch tracking inputs.
*   **Impact:** P2 - Smooths out avatar movement.

#### Opportunity 15: Offloading Core Calculations to Web Workers
*   **Problem:** Heavy tasks (audio FFT, WebSocket parsing, IPC message parsing) on the UI thread cause noticeable frame drops.
*   **Solution:** Move non-UI processing (WebSocket payload parsing, telemetry logging) to Web Workers.
*   **Impact:** P2 - Maintains a locked 60 FPS user interface.

---

### 3. `liva-desktop` (Tauri / Rust Host)

#### Opportunity 16: VRAMGuard macOS Porting
*   **Problem:** Windows-specific VRAM monitoring via `tasklist` is a no-op on macOS, ignoring memory pressure in unified memory systems.
*   **Solution:** Port `vram_guard_loop` to macOS using `ps -ax -o comm` to check for active resource-intensive applications (Xcode, Blender, DaVinci Resolve) and trigger preemptive LLM memory yielding.
*   **Impact:** P0 - Avoids virtual memory swap thrashing on Apple Silicon.

#### Opportunity 17: Process Port Cleanup & Sidecar Lifecycle Management
*   **Problem:** Crashed inference engines leave orphaned `llama-server` processes binding to ports, preventing new instances from launching.
*   **Solution:** Rust host execution on startup and shutdown to clean up orphaned local ports and terminate dangling Python/C++ sidecars.
*   **Impact:** P1 - Eliminates port binding conflicts and stale engine states.

---

### 4. `liva-ai-engine` (Python Inference Daemon)

#### Opportunity 18: Thread Capping for Apple Silicon (Draft & Batch)
*   **Problem:** llama.cpp batch threads default to all physical cores (including slower E-cores). Barrier synchronization forces fast P-cores to wait for slow E-cores. Additionally, running draft models with high thread counts causes excessive context-switch overhead.
*   **Solution:** Cap `res_threads_batch` to P-cores only on macOS, and limit draft model thread count to `max(1, min(4, n_threads // 2))`.
*   **Impact:** P0 - Increases speculative decoding prefill speed and generation throughput.

#### Opportunity 19: KV Cache Pruning Duplication Fix
*   **Problem:** The sliding window KV cache pruning shifts indices but forgets to explicitly remove the last token slot before calling `llama_decode` to re-evaluate it. This creates duplicate positions in the KV cache, causing numerical divergence and C++ segmentation faults.
*   **Solution:** Add explicit removal of the last token slot: `lib.llama_kv_cache_seq_rm(self.ctx, 0, n_past - 1, n_past)` before performing the shift and decoding.
*   **Impact:** P0 - Fixes C++ crashes and token corruption in long-context conversations.

#### Opportunity 20: try-except Guard for KV Cache Prefix Matching
*   **Problem:** When a `llama_decode` call fails mid-generation and throws an exception, `_cached_tokens` remains set but the native C++ KV cache is cleared, leading to corrupted prefix reuse on the next prompt.
*   **Solution:** Wrap the generation loop in a `try...except` block and clear `_cached_tokens = None` upon failure.
*   **Impact:** P1 - Ensures the native engine recovers cleanly from decoding exceptions.

#### Opportunity 21: Coherent Sliding-Window KV Cache Pruning for Speculative Decoding
*   **Problem:** Main model context size differs from draft model context size, risking out-of-sync pruning boundaries.
*   **Solution:** Verify that prefix matching and sliding-window KV cache pruning run in lockstep for both main and draft contexts on macOS.
*   **Impact:** P1 - Ensures correct alignment during long generation runs.

---

## Part 2: Implementation Roadmap
The 21 opportunities will be implemented across the remaining milestones:
*   **Milestone 2 (RAG & Gateway Upgrades):** Focuses on Opportunities 2, 3, 4, 10 (RAG ingestion, database upgrades, weighted RRF).
*   **Milestone 3 (UI/Desktop/Engine Upgrades):** Focuses on Opportunities 1, 5, 6, 7, 8, 9, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21.
*   **Milestone 4 (E2E Verification):** Performs unit and integration verification for all implemented optimizations.
*   **Milestone 5 (Documentation & Technical Debt):** Final reporting, tech ledger updates, and developer context alignment.
