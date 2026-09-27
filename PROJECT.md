# Project: LIVA SOTA Architecture Upgrade & Strategic Technology Roadmap

## Architecture Overview
LIVA (**Local Intelligent Virtual Assistant**) is a privacy-first, desktop-native personal AI assistant operating on Windows 10/11 x64. The architecture is centered around a unified high-performance Rust core (`liva-native-core`), connected via native in-process IPC (`tauri::ipc::Channel`) to a Tauri v2 desktop shell (`liva-desktop`) and a Vue 3 / Three.js user interface (`liva-ui`).

### Core Architectural Pillars
1. **Pillar 1: Multi-Agent Orchestration & Planning**: Asynchronous DAG planning (`petgraph`, `dagrs`), hierarchical reasoning (RouteLLM), Reflexion verbal self-healing, Multi-Agent Debate voting consensus, and poison-pill Dead Letter Queues (DLQ).
2. **Pillar 2: Local Memory & Hybrid Retrieval**: Hierarchical memory (Working Core, Recall SQLite, Archival Vector), Mem0 two-phase fact extraction & reconciliation, Lock-free double-buffered HippoRAG (`ArcSwap<CsrGraph>`), dynamic Ebbinghaus decay, two-stage hybrid search (`sqlite-vec` + FTS5 + Cross-Encoder reranker), and live Obsidian PKM graph integration.
3. **Pillar 3: Multimodal & Real-time Duplex**: Edge-optimized visual GUI grounding (ShowUI UVTS token pruning on Qwen2.5-VL-2B), SIMD screen diffing with co-scale fallback, two-stage adaptive voice turn-taking (`Smart Turn v3.2` active gate < 300ms), Sonora AEC3 + GTCRN + Silero VAD + Kokoro/Piper TTS, and multi-threaded OffscreenCanvas Three-VRM avatar kinematics.
4. **Pillar 4: Sandboxed System Automation & Security**: Dual-layer process containment (Windows Job Objects + AppContainer for Win32 tools; Wasmtime WASI 0.2 for untrusted agent plugins), high-throughput native Rust PII scrubbing (`liva-sanitizer`, Decree 13/2023 compliant), and tiered authorization with two-phase human-in-the-loop confirmation.

---

## Feature Inventory
Every surveyed capability and optimization is indexed below with its assigned milestone.

| # | Feature Key | Description | Milestone | Source |
|---|---|---|---|---|
| F01 | `dag-task-engine` | Asynchronous DAG task scheduler using `petgraph` & Tokio, supporting parallel fork/join execution | M1 | SOTA Survey (LangGraph, dagrs) |
| F02 | `hierarchical-router` | RouteLLM semantic complexity routing between local SLM (3B/7B) and Cloud LLMs | M1 | SOTA Survey (RouteLLM, Stanford) |
| F03 | `reflexion-self-healing` | Episodic verbal feedback reflection loop for automated tool retry and error recovery | M1 | SOTA Survey (Reflexion NeurIPS) |
| F04 | `voting-consensus` | Swarm deliberation panel with Borda count / majority voting consensus for critical actions | M1 | SOTA Survey (Multiagent Debate) |
| F05 | `task-dlq-quarantine` | Task-level Dead Letter Queue isolating poisoned tool executions into SQLite `tasks_dlq` | M1 | SOTA Survey (Temporal/Actor DLQ) |
| F06 | `mem0-two-phase` | Two-phase cognitive extraction & deterministic reconciliation engine (ADD, UPDATE, DELETE, NOOP) | M2 | SOTA Survey (Mem0, Letta) |
| F07 | `reflection-daemon` | Autonomous background triples extraction daemon running during system idle periods | M2 | SOTA Survey (Letta, Cognee) |
| F08 | `ebbinghaus-dynamic-decay` | Continuous time-based memory forgetting curve discounting in RRF scoring | M2 | SOTA Survey (Generative Agents) |
| F09 | `lock-free-csr-graph` | Lock-free double-buffered HippoRAG matrix via `ArcSwap<CsrGraph>` for non-blocking PPR | M2 | SOTA Survey (HippoRAG NeurIPS) |
| F10 | `two-stage-hybrid-rerank` | Hybrid search (sqlite-vec + FTS5, K=60 RRF) + Stage-2 ONNX Cross-Encoder (`bge-reranker-small`) | M2 | SOTA Survey (Qdrant, ColBERT) |
| F11 | `obsidian-pkm-sync` | Live Obsidian Vault watcher (`notify`) + AST parser (`pulldown-cmark`) mapping `[[wikilinks]]` to L3 | M2 | SOTA Survey (Obsidian Native) |
| F12 | `sqlite-vec-static` | Static C/Rust linkage of `sqlite-vec` directly into binary, eliminating runtime `vec0.dll` dependency | M2 | LIVA Codebase Gap Analysis |
| F13 | `showui-token-pruning` | Lightweight 2B VLM visual UI grounding with UI-Guided Visual Token Selection (UVTS 70-80% pruning) | M3 | SOTA Survey (ShowUI, UI-TARS) |
| F14 | `simd-roi-co-scale` | SIMD screen frame diffing with dynamic bounding-box cropping and 720p/1080p fallback scaling | M3 | LIVA Codebase Gap Analysis |
| F15 | `adaptive-turn-gate` | Two-stage adaptive voice turn-taking gate (`Smart Turn v3.2`), cutting conversational latency <300ms | M3 | SOTA Survey (Kyutai Moshi, LIVA) |
| F16 | `full-duplex-voice` | Full-duplex audio pipeline with Sonora AEC3, GTCRN denoiser, Silero VAD, and Kokoro/Piper TTS | M3 | SOTA Survey (LiveKit, sherpa-onnx)|
| F17 | `offscreen-avatar-worker`| Decoupled Three-VRM kinematics on OffscreenCanvas Web Worker, locking 60 FPS | M3 | SOTA Survey (Three-VRM, LIVA UI) |
| F18 | `windows-job-sandbox` | Native Win32 Job Object containment with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` & 512MB RAM cap | M4 | SOTA Survey (Windows OS Security)|
| F19 | `wasmtime-wasi-sandbox` | WebAssembly / WASI 0.2 capability sandboxing for untrusted agent plugins with fuel metering | M4 | SOTA Survey (Wasmtime, Bytecode) |
| F20 | `native-pii-sanitizer` | High-throughput pure Rust PII scrubber (`liva-sanitizer`, <1.5ms, Decree 13/2023 compliant) | M4 | SOTA Survey (Presidio, LLM Guard)|
| F21 | `tiered-hitl-auth` | Window-scoped `CommandPrincipal` tiered authorization with interactive two-phase confirmation | M4 | LIVA Codebase Audit |
| F22 | `retire-websocket` | Complete retirement of internal loopback `websocket.rs` in favor of native Tauri v2 IPC channels | M5 | LIVA Codebase Gap Analysis |
| F23 | `ram-guardrails-4gb` | Comprehensive hardware governor ensuring steady-state RAM < 4.0GB and VRAM < 6.0GB on Windows | M5 | System Resource Requirement |
| F24 | `oss-license-guardrails`| Strict `deny.toml` open-source license audit (MIT/Apache-2.0 pass, copyleft isolation, eSpeak removal)| M5 | System Legal Compliance |

---

## Milestones

| # | Name | Scope & Deliverable Focus | Dependencies | Status |
|---|---|---|---|---|
| M1 | Multi-Agent Orchestration & Planning | Technical architecture and design for dynamic DAGs, RouteLLM, Reflexion loops, voting consensus, and DLQ (F01–F05) | Survey complete | DONE |
| M2 | Local Memory, Hybrid Retrieval & PKM | Architecture and specs for static `sqlite-vec`, Mem0 reconciliation, Lock-free `CsrGraph`, Dynamic Ebbinghaus, Cross-Encoder, and Obsidian sync (F06–F12) | Survey complete | DONE |
| M3 | Multimodal, Duplex Voice & 3D Avatar | Architecture and design for ShowUI token pruning, Smart Turn active gate, full-duplex voice pipeline, and OffscreenCanvas avatar (F13–F17) | Survey complete | DONE |
| M4 | Sandboxed Automation, PII & Security | Architecture and specs for Windows Job Objects, Wasmtime WASI sandbox, `liva-sanitizer`, and tiered HITL authorization (F18–F21) | Survey complete | DONE |
| M5 | Master Technical Report & Strategic Roadmap | Authoritative technical deliverables in `docs/` and Obsidian Vault, Mermaid diagrams, Feature & Performance Matrix, Phased Implementation Roadmap, and Backlog items (F22–F24, all pillars synthesized) | M1, M2, M3, M4 | DONE |

---

## Interface Contracts

### 1. Swarm Orchestrator ↔ Specialized Agent Worker
- **Channel**: Async MPSC `tokio::sync::mpsc::channel(32)`
- **Request**: `TaskExecutionRequest { task_id: Uuid, parent_task_id: Option<Uuid>, prompt: String, allowed_tools: Vec<ToolDefinition>, timeout_ms: u64 }`
- **Response**: `TaskExecutionResult { task_id: Uuid, status: TaskStatus, output_artifacts: Vec<Artifact>, error: Option<TaskError>, execution_trace: ExecutionTrace }`
- **Error Handling**: On failure, retry with `ReflexionCritique`. If max retries ($N=3$) exceeded, push to `DeadLetterQueue` and mark dependent nodes as `Blocked`.

### 2. Retrieval Engine ↔ SQLite Storage Actor
- **Channel**: Actor command channel with `tokio::sync::oneshot::channel()`
- **Request**: `StorageQuery::HybridSearch { query: String, dense_vector: Vec<f32>, top_k: usize, rrf_k: f32, decay_alpha: f32 }`
- **Response**: `Vec<ScoredDocument { id: String, content: String, score: f32, metadata: serde_json::Value }`
- **Concurrency**: Lock-free reads via SQLite WAL; single-writer actor for mutations with micro-batching.

### 3. Voice Pipeline ↔ Tauri Desktop Shell
- **Channel**: `tauri::ipc::Channel<VoiceIpcEvent>`
- **Events**:
  - `VoiceIpcEvent::UserSpeechStarted { timestamp_ms: u64 }`
  - `VoiceIpcEvent::VisemeChunk { viseme_id: u8, duration_ms: u32, blendshapes: [f32; 52] }`
  - `VoiceIpcEvent::BargeInInterrupt { epoch_id: u32 }`
- **Guarantees**: Binary stream over memory buffer; latency < 20ms; dropped frames handled via epoch tracking.

### 4. Sandboxed Tool Execution ↔ Host OS Driver
- **Channel**: Isolated WASI or Job Object boundary
- **Contract**: `SandboxRunner::execute(wasm_bytes: &[u8], fuel_limit: u64, memory_limit_bytes: usize, stdin: &[u8]) -> Result<ExecutionResult, SandboxViolation>`
- **Guarantees**: Memory cap strictly enforced (64MB for Wasm, 512MB for Win32 process); child processes killed immediately on close.

---

## Code Layout
- `liva-native-core/`: Core Rust engine (daemon, pipeline, db, webrtc, stt, tts, cua, authorization)
  - `src/agent/`: Swarm DAG orchestrator, state machine, reflection, consensus
  - `src/db/`: Micro-batched SQLite WAL connection pool, `CsrGraph` HippoRAG, hybrid search
  - `src/webrtc/`: Sonora AEC3, GTCRN denoiser, Silero VAD, Smart Turn v3.2
  - `src/evolution/`: Wasmtime WASI sandbox and automated tool verification
- `crates/liva-storage/`: Unified transactional storage, schema migrations, and embedded vector virtual tables
- `crates/liva-cua/`: Computer-Use Agent Win32 driver, SIMD frame diffing, kill-switch
- `crates/liva-tools/`: Operating system tools, web tools, and sandboxed runner
- `liva-desktop/src-tauri/`: Tauri v2 desktop application, window management, native IPC channels
- `liva-ui/`: Vue 3 + Three.js frontend, OffscreenCanvas 3D avatar worker, audio capture worklet
- `docs/`: Master architectural specifications, benchmarking reports, and implementation roadmaps
- `teamwork_projects/obsidian_llm_wiki/vault/`: Obsidian knowledge vault integration and guidelines
