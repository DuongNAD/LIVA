---
title: "anti_patterns"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-07-23T00:00:00Z"
---

# Knowledge: Anti-Patterns

## Executive Summary
This document acts as a repository of anti-patterns, performance failures, and hard-won lessons from production debugging in LIVA. Treat these as compile-time constraints — violations break the system.

## Detailed Description
### Resource & VRAM Management
- **VRAM Thrashing**: NEVER swap model immediately after inference. Use Expert Cooldown TTL (120s-180s) to keep the heavy model in memory for follow-up questions.
- **Concurrent Model Load**: NEVER load both Router and Expert models simultaneously on a single GPU. It will cause CUDA out-of-memory. Use `Sequential Hot-Swap Architecture`.
- **VRAM Zombie Process**: Quên kill tiến trình `llama-server.exe` khi tắt app sẽ khóa cứng 8GB VRAM vĩnh viễn. Phải kill ĐẦU TIÊN khi shutdown.
- **Hard VRAM Preemption**: NEVER hard-abort avatar rendering without graduated degradation. Use `PreemptiveVramMutex.acquireWithGraduation()` which tries eco (5fps) → freeze (0fps) → hard preempt. Direct `acquire()` is only for non-avatar tasks.
- **VRAM Graduation Skip**: When using `acquireWithGraduation()`, do NOT skip the 500ms wait between steps — the frontend needs time to actually release WebGL resources after receiving the `avatar_demote` event.
- **Hardcoded Sleep Database**: Dùng `setTimeout` chờ DB xả WAL là sai lầm. Bắt buộc dùng event native `await db.close()`.
- **Main Thread Vector Search**: TUYỆT ĐỐI không gọi vector search của node:sqlite trên Main Thread. Các tác vụ FTS5/Vector phải chạy qua DatabaseWorker để bảo vệ Event Loop.
- **LLM GPU for Embeddings**: KHÔNG dùng chung GPU LLM cho việc tạo Vector Embeddings. Tách Embedding sang CPU ONNX Model để Router sống độc lập khỏi VRAMGuard và bảo toàn LLM KV Cache.

### Networking
- **fetch Silent Failure**: `fetch` resolves on HTTP 400/500. ALWAYS use `safeFetch()`.
- **Timer Leak**: `clearTimeout` MUST be in `finally`, not after `await fetch()`.
- **ECONNREFUSED Location**: Native fetch error message = "fetch failed". Real error is in `e.cause.message`, NOT `e.message`.
- **Axios Ghost Properties**: After migration, `e.response?.data` is DEAD CODE. Native fetch errors don't have `.response`.
- **Oversized Local WebSocket IPC**: Cap complete WebSocket messages before parsing or spawning work.
  The Rust voice transport allows at most 1 MiB of text and 1 MiB plus the 9-byte `VoiceFrame`
  header for binary messages.

### Singleton & Resource Management
- **Duplicate Model Loading**: NEVER compute embeddings on CPU (blocks Event Loop). Use `EmbeddingService.getInstance()` which delegates to `llama-server` GPU API (`/v1/embeddings`).
- **Missing `dispose()`**: Every service with timers (`setInterval`/`setTimeout`) or ML models MUST expose a `dispose()` or `destroy()` method. Call them in `CoreKernel.shutdown()`.
- **Zombie Timer on Recursive setTimeout**: Store the timer ref (`this.#reconnectTimer = setTimeout(fn, ms)`) and `clearTimeout` it before reassignment AND in `stop()`/`dispose()`. Use true private `#field` to prevent external zombie modifications. (Fixed: TelegramBridge, EmailClientManager, useGateway, 2026-05-05)

### Database
- **SQLite WAL Mode**: Always enable `PRAGMA journal_mode = WAL` + `PRAGMA synchronous = NORMAL` on init. Without WAL, concurrent reads during writes cause `SQLITE_BUSY`.
- **Duplicate DB Instances**: Never `new StructuredMemory()` in multiple places. Inject via `MemoryManager.getStructuredMemoryInstance()`.

### File I/O
- **Atomic Write**: ALWAYS use `.tmp` + `rename()` pattern for persistent data files. Direct `writeFile` can corrupt data on crash/concurrent write.
- **Sync I/O in Hot Path**: `fs.readFileSync` + `fs.appendFileSync` × 3 = 3 blocking calls per event. Use debounced async writes.
- **Concurrent Config Patches**: Rust `update_config` writes must run behind one process-wide lock
  on `spawn_blocking`, reject malformed/non-object patches and existing non-object roots, flush a
  sibling temporary file, then atomically replace the destination. Never silently turn malformed
  or structurally invalid JSON into `{}`.
- **UTF-8 Byte Slicing**: Never truncate user/file text with `&text[..N]`; `N` can split a
  Vietnamese character and panic. Truncate by `chars()` and chunk Telegram output below its limit.

### Cache
- **Unbounded Map Cache**: NEVER use `new Map()` for caching without eviction. Use `lru-cache` with `{ max, ttl }`.
- **O(N) Cache Keys**: Don't use `array.join(",")` for cache keys. Use `Buffer.from(new Float32Array(v).buffer).toString("base64")`.

### Code Generation
- **AST on Main Thread (Event Loop Block)**: NEVER run `new Project()` or heavy `ts-morph` operations (e.g. `getPreEmitDiagnostics`) on the Main Thread. Always delegate to `ASTWorker.ts` via `ASTWorkerBridge.ts` to prevent freezing the gateway.
- **Greedy Regex JSON**: `/{[\s\S]*}/` swallows multiple JSON blocks. Use `indexOf('{')` + `lastIndexOf('}')` + `jsonrepair`.
- **Duplicate try Blocks**: Multi-edit tools can generate `try { try {` when replacing code inside existing try blocks. Always verify brace nesting after automated edits.
- **Singularity Circular Dependency**: Singularity Pipeline bắt buộc dùng AST nội bộ (`ASTActuator`), không dùng Skill Tool (`SkillRegistry`), và phải có luồng Rollback (`RollbackManager`).

### gRPC Streaming
- **Async Iterator Data Loss**: `GRPCStream.pushChunk()` MUST always queue chunks to the buffer array, then signal the iterator via `resolveNext()`. NEVER pass data through the promise resolution value — the iterator discards it.
- **Drain Before Error**: After the iterator wakes from `await`, ALWAYS loop back to drain the chunk queue before checking `this.error`. Otherwise, chunks received before `fail()` are silently dropped.

### Timer Management
- **Race Timeout in Promise.race**: NEVER use `Promise.race([task, new Promise(setTimeout)])`. The timeout's `setTimeout` leaks on every successful task. Use `withSafeTimeout(promise, ms, label)` from `HttpClient.ts` instead — it clears the timer in `.finally()`.

### Security Hardening
- **Duplicate Encryption**: Tuyệt đối không copy/paste logic mã hóa giữa các file. Bắt buộc phải import và dùng chung `EncryptionEngine` để tránh mất đồng bộ key và lộ secret.
- **Destructive Git Rollback**: NEVER use `git checkout -- src/` or `git clean -fd src/` in rollback logic. These commands nuke ALL uncommitted work in the entire `src/` tree. Use physical folder snapshot (`.src.rollback.bak`) via async `fs.promises.cp` instead (NEVER `fs.cpSync`!).
- **Unsanitized External Data in LLM Prompts**: NEVER inject clipboard/window title data directly into system prompts. Always run through `sanitizeSensoryData()` (max 2000 chars, HTML strip, control char escape). Attacker can manipulate LLM via clipboard poisoning.
- **Auto-leaking IP Geolocation**: NEVER call external IP lookup APIs unconditionally on boot. Geolocation must be OPT-IN via `LIVA_GEOLOCATION_ENABLED=true`.

### Tauri v2 / Packaging
- **Tauri Architecture**: liva-ui sử dụng Tauri v2 (Rust host + OS WebView) cho:
  - Transparent Widget Window (always on top, mouse passthrough)
  - System Tray với context menu
  - Secure Credential Vault (tauri-plugin-stronghold)
- **Sidecar Pattern**: Gateway chạy như sidecar process, giao tiếp qua Dynamic WS Handshake.
- **ABI Mismatch**: Native C++ addons (`isolated-vm`) crash with stale ABI. Prefer: `node:sqlite` (built-in) or WASM alternatives.
- **Node.js SEA (Single Executable Application)**: Khi bundle file bằng `esbuild` qua `build-sea.js`, **BẮT BUỘC** phải đưa các thư viện Native C++ (`sqlite-vec`) vào mục `external: [...]`. Script hậu kỳ phải copy thủ công các file `.node` từ `node_modules` ra nằm ngang hàng với file `.exe` sinh ra.
- **Bundled Browsers**: `puppeteer` downloads 500MB+ Chromium. Use `playwright-core` (API only, 2MB) + system Chrome via `executablePath`.

### Testing
- **False Green**: 100% pass rate means NOTHING if tests only mock happy paths. Every fetch mock MUST include at least one 4xx/5xx negative test case.
- **Mock fetch Correctly**: Use `vi.stubGlobal('fetch', vi.fn())` — NOT `axios-mock-adapter` or `nock`.
- **UIController Tests**: MUST push `--dev` to `process.argv` before creating UIController instance, and restore in `afterEach`. This bypasses `randomUUID`-based token auth that is inaccessible to test mocks.
- **Fake Timer + Promise Rejection**: When testing timeout behavior with `vi.useFakeTimers()`, attach a `.catch()` handler to the promise BEFORE calling `vi.advanceTimersByTimeAsync()`. Otherwise, the rejected promise becomes an unhandled rejection before `await expect().rejects` can catch it.
- **Module-level Mock Completeness**: When mocking `fs`, include ALL methods used by the target module (`readFile`, `writeFile`, `rename`, `existsSync`, `mkdirSync`). Missing methods cause silent failures in async handlers that swallow errors via try/catch.

### Performance
- **Double Eviction**: Don't call `evictExpired()` then `getAllFacts()` — the latter already calls eviction internally.
- **Health-Check Timer Leaks**: NEVER use `setInterval` for service health checks if it can be reactive. We rely on `safeFetch` timeout failures (boolean return) to trigger TTS engine hot-swaps, avoiding zombie timers completely.
- **TTS Word-by-Word Stuttering**: NEVER stream raw tokens directly to TTS engines. Emojis, Markdown, and single tokens cause robotic stuttering. ALWAYS buffer tokens into complete, sanitized **clauses** via `TTSFormatter` (Semantic Clause Chunking) before network transmission. TTSFormatter splits on Vietnamese conjunctions (và, thì, mà, nhưng...), clause punctuation (, : ; —), and 8-word overflow for <300ms TTFS.
- **~~Wake Word Feedback Loop~~ [DEPRECATED v22]**: ~~ALWAYS pause the Always-On microphone while TTS is playing~~ → **REPLACED BY**: Frontend `getUserMedia()` MUST enable `{ echoCancellation: true, noiseSuppression: true }`. Backend NEVER sends `mic_stop`/`mic_start`. Mic is **Always On** for True Full-Duplex. WebRTC AEC handles echo cancellation at the hardware/OS level.
- **Context-Aware Barge-in (v23 Two-Stage)**: NEVER abort LLM on `speech_start` — causes false positives from coughs/fillers, wasting VRAM. Instead: Stage 1 (speech_start) → Audio Ducking (TTS volume → 20%, LLM keeps running). Stage 2 (transcription_ready) → `BackchannelDetector.isBackchannel()` classifies text: backchannel ("ừm", "ok", <3 words) → restore volume; real speech → `agentLoop.bargeIn()` (AbortController + XML-safe memory truncation with `<interrupted>` tag).
- **VRAM Territorial Integrity**: When `AI_PROVIDER=local`, GPU VRAM is reserved exclusively for LLM. STT uses Nemotron 3.5 ONNX (CPU-only, `onnxruntime-node`) via `NemotronWorker.ts` worker thread — zero VRAM overhead. NEVER load any STT GPU model alongside local LLM — causes CUDA OOM crash.
- **Latency Masking**: For heavy routes (`deep_reasoning`, `system_command`), AgentLoop MUST emit `onLatencyMask` filler audio ("Dạ vâng...", "Hmm...") BEFORE LLM starts generating. This masks the 1.5-3s TTFT behind natural Vietnamese conversational fillers. Perceived latency = 0ms.
- **KV Cache Shifting**: llama-server MUST run with `--cache-reuse 256` to preserve system prompt KV cache across barge-in turns. On interrupt, LLM only recomputes the new user text — saves 40-60% GPU energy.

### Asynchronous & Evolution Anti-Patterns (v25 Hardening)
- **Active Skill Probing:** NEVER run scheduled dry-runs on external APIs (wastes Quota/Rate Limit). ALWAYS use a Passive Circuit Breaker wrapping `SkillRegistry.execute()` to detect failures dynamically. 3 consecutive errors → OPEN_CIRCUIT → `PromptBuilder` prunes the dead tool from `<tools>` XML → LLM won't hallucinate calls to broken skills.
- **Singularity Fork-Bomb:** NEVER use unbounded `while(true)` in `EvolutionPipeline`. MUST implement `MAX_EPOCHS`, Failure Circuit Breakers (`MAX_CONSECUTIVE_FAILURES`), Hypothesis Deduplication (`Set<string>`), and OS Hardware Budgeting (Battery, RAM, CPU load) before starting an epoch.
- **Silent Worker Deadlocks:** NEVER assume `worker_threads` (like VAD) only fail via `"error"` events. ONNX Runtime C++/WASM can deadlock silently. ALWAYS implement a Ping/Pong Watchdog Heartbeat to detect and auto-recover from silent deadlocks. `VADWorkerBridge` includes exponential backoff recovery (max 3 attempts).
- **FIFO VRAM Locks:** NEVER use basic FIFO locks for VRAM. Background tasks (Consolidation, Shadow Digest) MUST use Preemptive Locks (`AbortController`) so `AgentLoop` can instantly abort them and steal the GPU when the user speaks. Voice Full-Duplex latency = 0ms.
