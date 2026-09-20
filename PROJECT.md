# Project: LIVA Next-Gen Architecture Remake (LIVA-REMAKE-2026)

## Architecture

LIVA is a native Cognitive Desktop Operating System designed to run fully offline on Windows 10/11 x64 workstations within strict resource budgets:
- **System RAM Ceiling**: $\le 4.0\text{ GB}$ (operational steady-state target $\sim 3.0\text{ GB}$).
- **GPU VRAM Ceiling**: $\le 5.1\text{ GB}$.

The next-generation remake transitions LIVA from a monolithic native crate with unbatched disk persistence and serialized mutex locks into a **high-performance, actor-based, multi-crate workspace topology** with unified in-process IPC and parallelized CI/CD.

```
                           ┌────────────────────────────────────────────────────────┐
                           │                 LIVA DESKTOP SHELL                     │
                           │                    (liva-desktop)                      │
                           │     Tauri v2 Application · System Tray · Windows       │
                           └───────────────────────────┬────────────────────────────┘
                                                       │ Native Tauri v2 IPC
                                                       │ (Channels & Binary Streams)
                                                       ▼
┌───────────────────────────────────────────────────────────────────────────────────────────────────┐
│                             LIVA NATIVE ORCHESTRATION FACADE                                      │
│                                    (liva-native-core)                                             │
│                 handle_command · Lifecycle Coordinator · AppState Context                         │
├──────────────────────┬───────────────────────┬─────────────────────┬──────────────────────────────┤
│                      │                       │                     │                              │
│       crates/        │        crates/        │       crates/       │           crates/            │
│   liva-core-types    │      liva-storage     │      liva-llm       │          liva-voice          │
│                      │                       │                     │                              │
│  - Pure Domain Types │  - SQLite Pool (WAL)  │  - LlmActor Worker  │  - Full-Duplex WebRTC        │
│  - Errors & Results  │  - Micro-Batched      │  - Priority Queue   │  - Silero VAD / Sonora AEC3  │
│  - Permissions/Auth  │    Single-Writer      │    (High/Norm/Low)  │  - GTCRN Denoiser            │
│  - IPC Message       │  - Trie Active Recall │  - Broadcast Stream │  - Parakeet STT (ONNX)       │
│    Contracts         │  - Double-Buffered    │  - SHA-256 Trust    │  - VieNeu/Piper TTS          │
│  - Zero Heavy Deps   │    CsrGraph (HippoRAG)│    Artifact Cache   │  - Phoneme / Viseme Stream   │
│                      │  - Calibrated Pragmas │  - Dynamic Prompt   │  - Audio Ring Buffers        │
│                      │  - Scoped Vector ANN  │    Budgeting        │                              │
└──────────────────────┴───────────────────────┴─────────────────────┴──────────────────────────────┘
                                                       ▲
                                                       │ Command Dispatch & Diagnostics
                               ┌───────────────────────┴───────────────────────┐
                               │                 crates/liva-tools             │
                               │        Consolidated CLI & Diagnostic Suite    │
                               │   (Replaces 26 separate probe/test binaries)  │
                               └───────────────────────────────────────────────┘
```

### Core Subsystems

#### Subsystem 1 — Calibrated Persistence & Knowledge Graph (`crates/liva-storage`)
- **Calibrated SQLite Engine**: Standardized 4KB page size (`PRAGMA page_size = 4096;`) aligning with OS physical memory clusters; calibrated cache size (`PRAGMA cache_size = -2000;`) bounding SQLite page cache to ~11.7 MB total across reader/writer pools (saving >300 MB RAM compared to legacy configuration).
- **Micro-Batched Writer Actor**: Single dedicated writer connection on `DbActor` thread with transactional coalescing (commits flushed upon 50 operations or 5ms elapsed time window), multiplying write throughput by 50x (1,500–3,000 ops/sec).
- **Radix Trie Active Recall Indexing**: In-memory prefix Trie for personal facts keyword filtering (<1ms lookup); eliminates legacy full table scans and mass AES-256-GCM decryptions prior to chat completion turns.
- **Lock-Free HippoRAG CSR Graph**: Personalized PageRank associative retrieval running lock-free over `arc_swap::ArcSwap<CsrGraph>`, eliminating write-lock stalls and dirty cache drops during knowledge consolidation.
- **Accurate Scoped Vector Retrieval**: Resolves `sqlite-vec` subquery post-filtering false negatives through isolated domain vector partitioning.

#### Subsystem 2 — Actor-Based Cognitive LLM Runtime (`crates/liva-llm`)
- **LlmActor Worker Queue**: Replaces synchronous global `Mutex<LlamaRouterManager>` with a bounded Tokio MPSC actor channel supporting a 3-tier priority queue (`Priority::High` for instant health checks and cancellations; `Priority::Normal` for user dialogue; `Priority::Low` for background consolidation and embeddings).
- **Decoupled Embedding Engine**: Lock-free multi-threaded embedding generation via `Arc<EmbeddingEngine>` using ORT thread-safe inference sessions without locking the text completion loop.
- **Artifact Trust Cache**: SHA-256 model verification cached in SQLite metadata with fast size/mtime checks, eliminating 3–6s cold-start disk thrashing upon waking from idle.
- **Deterministic Token Budgeting**: Exact prompt compilation (`ContextTokenBudget` + `compile_exact_budgeted_prompt`) with atomic cancellation heartbeat detection.

#### Subsystem 3 — Low-Latency Full-Duplex Audio & Voice (`crates/liva-voice`)
- **Two-Stage Turn-Taking Gate**: Smart Turn v3.2 ONNX classifier executing Stage 1 Fast Cut @ 200ms ($p > 0.92$), Stage 2 Vietnamese Pause Buffer @ 200–450ms ($0.50 \le p \le 0.92$), and Stage 3 Safety Timeout @ 450ms.
- **AEC3 Loopback & GTCRN Denoising**: WASAPI loopback capture (`cpal`) feeding WebRTC AEC3 (`sonora`) combined with causal GTCRN STFT denoiser for <100ms barge-in detection during speaker playback.
- **Fast Voice Streaming**: Clause streaming TTS (`TtsChunker` 2–9 words) with 150ms client jitter buffer and `OP_VISME` binary phoneme packets achieving Fast Voice P90 < 480ms SLA.

#### Subsystem 4 — 3D Avatar & Vision Engine (`liva-ui` Web Worker + Screen Diff)
- **OffscreenCanvas Web Worker Isolation**: Three.js rendering loop, bone deformation, and spring bone physics calculation migrated to `avatarRenderer.worker.ts` via `transferControlToOffscreen()`, guaranteeing locked 60 FPS without main-thread JavaScript starvation.
- **Event-Driven Ghost Mode**: Windows low-level mouse hooks (`WH_MOUSE_LL`) replacing legacy 33Hz cursor polling loop, dropping idle CPU overhead to near 0%.
- **SIMD Diff ROI Screen Vision**: SIMD pixel diff engine (`compute_roi_patch`) with co-scale fallback to 720p when change area > 35%, capping visual tokens at 144–384 and VLM TTFT < 350ms.

#### Subsystem 5 — Desktop Shell & Unified IPC (`liva-desktop`)
- **100% Native Tauri v2 Channels**: Elimination of legacy loopback TCP WebSocket server (`websocket.rs`, saving ~2,100 lines of duplicated code); zero port binding collisions, zero double-serialization.
- **Compile-Time Type Synchronization**: Automated TypeScript bindings generation (`specta` / `tauri-specta`) derived directly from `crates/liva-core-types`.

### Governance & Safety Constraints
- **Sequential Tooling Discipline**: All cargo commands must execute sequentially passing `-j 2` (`cargo check -j 2`, `cargo build -j 2`, `cargo test -j 2 -- --test-threads 2`).
- **RAM Guardrails**: Available machine RAM must remain >= 4.0 GB prior to heavy test suites. Never launch background MCP indexers or heavy graph analyzers.
- **Git Safety Boundary**: Git operations end strictly at staging (`git add`). Autonomous `git commit` or `git push` are strictly prohibited.

---

## Feature Inventory

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Workspace Dependencies & Release Profile Optimization | Configure `[workspace.dependencies]`, `[workspace.lints]`, and `[profile.release]` (LTO Thin, codegen-units=1, panic=abort, strip=symbols) in root `Cargo.toml` | M1 | CI/CD Survey |
| 2 | Fast Linker Configuration & Build Artifact Slimming | Configure `.cargo/config.toml` for MSVC `lld-link` acceleration; implement clean target pruning to reduce 94 GB target bloat | M1 | CI/CD Survey |
| 3 | Integration Test & Binary Probe Consolidation | Consolidate 78 integration tests into `tests/harness.rs` submodule router and 26 binary probes into `crates/liva-tools` CLI | M1 | CI/CD Survey |
| 4 | 4-Job Parallel DAG GitHub Actions Pipeline | Modernize `.github/workflows/test.yml` into 4 parallel DAG jobs (Ubuntu lints, Ubuntu web, Ubuntu Rust quality, Windows native build) with pre-built `cargo-deny` | M1 | CI/CD Survey |
| 5 | Npm Supply Chain Audit Remediation | Resolve 6 High-severity vulnerabilities in root `package.json` through dependency overrides and hoisted lockfile hygiene | M1 | CI/CD Survey |
| 6 | Calibrated SQLite Pragmas & Page Size Standardization | Apply `page_size = 4096`, `cache_size = -2000`, `wal_autocheckpoint = 1000` in `liva-storage/src/pragmas.rs` saving ~300 MB RAM | M2 | Database Survey |
| 7 | Micro-Batched Single-Writer DbActor | Implement 50-operation / 5ms transactional batch coalescing on exclusive connection in `DbActor`, boosting throughput to >1,500 ops/s | M2 | Database Survey |
| 8 | Radix Trie In-Memory Active Recall Indexing | Build in-memory prefix Trie for personal facts keyword filtering; decrypt AES-256-GCM only for matched candidate facts (<2ms SLA) | M2 | Database Survey |
| 9 | Atomic Double-Buffered HippoRAG CSR Graph | Wrap in-memory `CsrGraph` with `arc_swap::ArcSwap` for lock-free Personalized PageRank reading during concurrent triple ingestion | M2 | Database Survey |
| 10 | Missing Secondary Indexes & Scoped Vector Retrieval | Add indexes on `facts(sourceTurnId)`, `events(timestamp DESC)`; refactor `sqlite-vec` query to eliminate subquery recall dropouts | M2 | Database Survey |
| 11 | Priority-Queued Non-Blocking LlmActor | Transition `LlamaRouterManager` to Tokio MPSC actor with 3-tier priority queue (High/Normal/Low), non-blocking health checks and cancellation | M3 | Codebase Survey |
| 12 | Model Trust Artifact Caching (Instant Wake) | Store SHA-256 model verification hashes in SQLite; verify mtime and size at boot to eliminate 3–6s wake freeze | M3 | Codebase Survey |
| 13 | Unified Tauri v2 IPC Channels | Remove redundant TCP WebSocket server (`websocket.rs`), migrating audio streaming, visemes, and LLM text chunks to native Tauri v2 Channels | M3 | Codebase Survey |
| 14 | OffscreenCanvas Web Worker Avatar Rendering | Decompose `WidgetApp.vue` into atomic components and migrate Three.js render loop to Web Worker via `transferControlToOffscreen()` | M3 | Codebase Survey |
| 15 | Full Regression Testing & Performance Validation | Execute full automated test suite (`cargo test -j 2`, `vue-tsc`, `vitest`, `cargo clippy`) and benchmark RAM, latency, and throughput SLAs | M4 | Architecture Plan |
| 16 | Operational Documentation & Architecture Runbooks | Update README, architecture runbooks, and developer onboarding guides reflecting the multi-crate workspace and new IPC patterns | M4 | Architecture Plan |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| **M1** | **Workspace & CI/CD Modernization** | Features 1, 2, 3, 4, 5: Root workspace dependencies, release profiles, `lld-link`, single test harness, `liva-tools` CLI, 4-job parallel DAG CI, npm audit fix | none | DONE |
| **M2** | **Database & Storage Engine Optimization** | Features 6, 7, 8, 9, 10: Calibrated SQLite pragmas, micro-batched DbActor, Radix Trie active recall, `ArcSwap` CSR graph, index hardening, scoped vector fix | M1 | DONE |
| **M3** | **LLM Concurrency & Unified IPC** | Features 11, 12, 13, 14: Priority-queued `LlmActor`, SHA-256 trust caching, WebSocket retirement, unified Tauri v2 channels, OffscreenCanvas Web Worker | M1, M2 | DONE |
| **M4** | **Documentation, Verification & Packaging** | Features 15, 16: Full test suite verification with zero errors, benchmark confirmation, installer generation (<85 MB), updated architecture documentation | M1, M2, M3 | DONE |

---

## Interface Contracts

### 1. LlmActor Priority Request Contract (`crates/liva-llm`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    High = 0,   // Health check, parameter inspection, stream cancellation
    Normal = 1, // User chat completions, interactive voice turns
    Low = 2,    // Background embeddings, memory consolidation, logging
}

pub enum LlmRequest {
    Generate {
        messages: Vec<ChatMessage>,
        budget: ContextTokenBudget,
        stream_tx: tokio::sync::broadcast::Sender<TokenChunk>,
        cancel_token: tokio_util::sync::CancellationToken,
        reply: tokio::sync::oneshot::Sender<Result<GenerationMetrics, LlmError>>,
    },
    HealthCheck {
        reply: tokio::sync::oneshot::Sender<LlmHealthStatus>,
    },
    CancelCurrent,
}
```

### 2. DbActor Micro-Batching Contract (`crates/liva-storage`)
```rust
pub enum DbWriteCommand {
    InsertTurn { record: ConversationTurnRecord, ack: Option<oneshot::Sender<Result<i64, DbError>>> },
    UpsertFact { key: String, encrypted_value: Vec<u8>, ack: Option<oneshot::Sender<Result<(), DbError>>> },
    InsertTriples { triples: Vec<KnowledgeTriple>, ack: Option<oneshot::Sender<Result<usize, DbError>>> },
    PruneRetention { cutoff_timestamp: i64, ack: Option<oneshot::Sender<Result<usize, DbError>>> },
}

pub struct MicroBatchConfig {
    pub max_batch_size: usize,      // 50 operations
    pub max_linger_duration: Duration, // 5 milliseconds
}
```

### 3. Active Recall Radix Trie Interface (`crates/liva-storage`)
```rust
pub struct FactTrieIndex {
    trie: radix_trie::Trie<String, FactMetadataEntry>,
}

impl FactTrieIndex {
    pub fn build_from_db(conn: &rusqlite::Connection) -> Result<Self, DbError>;
    pub fn scan_utterance(&self, text: &str) -> Vec<&FactMetadataEntry>;
    pub fn insert_keyword(&mut self, keyword: &str, entry: FactMetadataEntry);
}
```

### 4. Tauri v2 Unified Channel Contract (`liva-desktop`)
```rust
#[tauri::command]
pub async fn stream_chat_completion(
    app_state: tauri::State<'_, AppState>,
    prompt: String,
    on_token_channel: tauri::ipc::Channel<String>,
) -> Result<GenerationSummary, String>;

#[tauri::command]
pub async fn stream_voice_audio(
    app_state: tauri::State<'_, AppState>,
    audio_channel: tauri::ipc::Channel<tauri::ipc::InvokeResponseBody>,
) -> Result<(), String>;
```

### 5. Calibrated SQLite Pragma Constants (`crates/liva-storage`)
```rust
pub const PRAGMA_PAGE_SIZE: u32 = 4096;
pub const PRAGMA_CACHE_SIZE_READER: i32 = -2000; // ~1.95 MiB
pub const PRAGMA_CACHE_SIZE_WRITER: i32 = -4000; // ~3.9 MiB
pub const PRAGMA_WAL_AUTOCHECKPOINT: u32 = 1000; // 4.0 MB max WAL file
pub const PRAGMA_BUSY_TIMEOUT_MS: u32 = 5000;
pub const PRAGMA_SYNCHRONOUS: &str = "NORMAL";
```

---

## Code Layout

```
LIVA/
├── Cargo.toml                         # Root Cargo workspace manifest with inheritance & release profiles
├── package.json                       # Root npm workspace manifest with overrides for supply-chain fixes
├── .cargo/
│   └── config.toml                    # Fast linker (lld-link), optimization flags (-C target-cpu=native)
├── .github/
│   └── workflows/
│       └── test.yml                   # 4-Job Parallel DAG GitHub Actions workflow
├── crates/
│   ├── liva-core-types/               # Pure domain models, error hierarchies, auth permissions, IPC schemas
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── errors.rs
│   │       ├── events.rs
│   │       ├── models.rs
│   │       └── permissions.rs
│   ├── liva-storage/                  # High-performance persistence, WAL pools, micro-batching, Trie recall
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── pragmas.rs             # Standardized 4KB page size and calibrated memory cache
│   │       ├── pool.rs                # r2d2 reader pool & single-writer connection manager
│   │       ├── db_actor.rs            # Micro-batched writer thread (50 tx / 5ms commit)
│   │       ├── trie_recall.rs         # In-memory prefix Trie for active recall (<2ms SLA)
│   │       ├── csr_graph.rs           # HippoRAG Personalized PageRank over ArcSwap<CsrGraph>
│   │       ├── migrations.rs          # Secondary index migrations and schema cleanups
│   │       └── vector_search.rs       # sqlite-vec scoped ANN search without destructive subqueries
│   ├── liva-llm/                      # Non-blocking Actor runtime, model routing, token budgeting
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── actor.rs               # LlmActor background worker with 3-tier priority queue
│   │       ├── router.rs              # Router vs Expert model auto-swapping
│   │       ├── prompt_budget.rs       # Exact token budgeting compiler
│   │       ├── embedder.rs            # Lock-free Arc<EmbeddingEngine> multi-threaded ORT pool
│   │       └── trust_cache.rs         # SQLite-backed SHA-256 artifact trust verification
│   ├── liva-voice/                    # Low-latency full-duplex speech interaction pipeline
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── vad.rs                 # Silero Voice Activity Detector
│   │       ├── smart_turn.rs          # Smart Turn v3.2 ONNX two-stage turn-taking classifier
│   │       ├── aec.rs                 # WASAPI loopback capture & WebRTC AEC3 (sonora)
│   │       ├── denoise.rs             # Causal GTCRN STFT denoiser
│   │       ├── stt.rs                 # Parakeet-vi CTC streaming speech recognition
│   │       ├── tts.rs                 # VieNeu / Piper clause-streaming speech synthesis
│   │       └── viseme.rs              # Real-time OP_VISME phoneme blendshape generator
│   └── liva-tools/                    # Consolidated diagnostic, probe, and benchmark CLI
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs                # Subcommand dispatcher (probe-db, bench-voice, inspect-wal)
│           ├── commands/
│           │   ├── db_probe.rs
│           │   ├── voice_bench.rs
│           │   └── llm_bench.rs
├── liva-native-core/                  # Top-level orchestration facade preserving backward compatibility
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                     # Public AppState, command routers, bootloader coordinator
│   │   ├── boot.rs                    # Service lifecycle and resource governor integration
│   │   └── commands/                  # Domain command dispatchers (memory, config, system, skills)
│   └── tests/
│       └── harness.rs                 # Single consolidated integration test harness with submodules
├── liva-desktop/                      # Tauri v2 Desktop Application Shell
│   ├── Cargo.toml
│   ├── tauri.conf.json                # Multi-window config (widget, dashboard, setup)
│   └── src-tauri/
│       └── src/
│           ├── main.rs
│           ├── lib.rs                 # Tauri native commands, IPC channels, low-level mouse hooks
│           └── bindings.rs            # Generated TypeScript bindings (specta)
├── liva-ui/                           # Vue 3 Modular Frontend Application
│   ├── package.json
│   ├── src/
│   │   ├── App.vue                    # Root application
│   │   ├── WidgetApp.vue              # Clean transparent floating avatar widget (<300 lines)
│   │   ├── DashboardApp.vue           # System monitoring, settings & knowledge explorer
│   │   ├── components/
│   │   │   ├── avatar/
│   │   │   │   └── AvatarViewport.vue # OffscreenCanvas container
│   │   │   ├── dialogue/
│   │   │   │   ├── DialogueBubble.vue # Streaming typography chat bubble
│   │   │   │   └── TypingIndicator.vue
│   │   │   └── tools/
│   │   │       └── HitlConfirmCard.vue
│   │   ├── composables/
│   │   │   ├── useTauriIpc.ts         # Unified Tauri v2 Channel consumer
│   │   │   └── useVoiceStream.ts      # Audio chunking & viseme dispatch
│   │   └── workers/
│   │       └── avatarRenderer.worker.ts # Three.js, spring bones, and pose pipeline in Web Worker
└── packages/
    └── liva-common/                   # Shared TypeScript type definitions and protocol constants
```
