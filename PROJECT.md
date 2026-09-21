# Project: LIVA Production-Ready Optimization & Ecosystem Hardening

## Architecture

LIVA is a native Cognitive Desktop Operating System designed to run fully offline on Windows 10/11 x64 workstations within strict resource budgets:
- **System RAM Ceiling**: ≤ 4.0 GB (operational steady-state target ~3.0 GB).
- **GPU VRAM Ceiling**: ≤ 5.1 GB.
- **3D Avatar Target**: 60 FPS unthrottled in dedicated Web Worker via OffscreenCanvas, isolated from Main UI thread DOM/token churn.
- **Lip-Sync Latency**: Clock-synchronized phoneme visemes with drift < 5ms (SLA target < 30ms).

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                       LIVA DESKTOP CLIENT                                        │
│                     WebView2 / Vue 3.5 · System Tray · Windows Manager (Tauri v2)                 │
├───────────────────────────────────────────────┬──────────────────────────────────────────────────┤
│                Main UI Thread                 │           Dedicated Web Worker (Isolated)        │
│  - WidgetApp.vue & Chat UI                    │  - Three.js WebGL2Renderer on OffscreenCanvas    │
│  - Web Audio API (useSpeakerPlayback)         │  - VRM Model & Spring Bone Physics Engine       │
│  - High-res Audio Clock Anchor                │  - Unified 6-Step Humanoid Additive Pose Loop    │
│  - Tauri v2 Channel Ingestion (voice_subscribe)│ - Distance-based Locomotion & FootPlantIK       │
│  - useAvatarWorkerBridge (postMessage)        │  - Synchronized Viseme Timeline (<5ms drift)     │
└───────────────────────┬───────────────────────┴──────────────────────────▲───────────────────────┘
                        │                                                  │
                        │ Native Tauri v2 IPC Channels                     │ (Canvas ownership transfer)
                        ▼                                                  │
┌──────────────────────────────────────────────────────────────────────────┴───────────────────────┐
│                                LIVA NATIVE CORE ENGINE (Rust)                                    │
│                     (AppState Lifecycle Coordinator · Non-blocking Facade)                        │
├──────────────────────┬────────────────────────┬─────────────────────┬────────────────────────────┤
│       crates/        │        crates/         │       crates/       │          crates/           │
│   liva-core-types    │      liva-storage      │      liva-llm       │        liva-tools          │
│                      │                        │                     │                            │
│  - Domain Models     │  - Micro-Batched       │  - LlmActor (OS Thr)│  - Consolidated Diagnostic │
│  - Error Types       │    Single-Writer WAL   │  - Real-Time Token  │    Suite & CLI             │
│  - Permissions/Auth  │  - FactTrie Search     │    Streaming Chans  │  - Benchmarks & Latency    │
│  - IPC Event Types   │  - Thread-Safe Reads   │  - Priority Preempt │    Profilers               │
│  - Zero Heavy Deps   │  - Clean Clippy Lint   │  - Non-blocking Host│                            │
├──────────────────────┴────────────────────────┴─────────────────────┴────────────────────────────┤
│                                 liva-native-core Subsystems                                      │
│  - StateGraph Asynchronous DAG Engine (68/68 unit tests passing)                                 │
│  - SqliteCheckpointer (AES-256-GCM encrypted persistence)                                        │
│  - VisualGovernor (750 MB VLM mutual exclusion)                                                  │
│  - VoiceCoordinator (Audio Ingest, Silero VAD, Parakeet STT, Piper/VieNeu TTS)                   │
│  - CsrGraph Batched Writes & Dirty-Flag Compilation                                              │
│  - Bounded SessionEventStream Ring Buffer (max 500) & ActiveRecall TTL Eviction                  │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Feature Inventory

Every feature identified in the Survey phase is mapped below with its assigned milestone.

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Real-Time LLM Token Streaming | Refactor `LlmCommand::GenerateText` to stream tokens incrementally over an `mpsc` channel directly to Tauri IPC and TTS chunker, eliminating full-response buffering and reducing TTFT to < 350ms | M1 | Survey R1 |
| 2 | Non-Blocking LLM Actor Inference | Isolate synchronous `llama-cpp-2` computation onto a dedicated OS worker thread or `tokio::task::spawn_blocking`, preventing Tokio runtime worker thread starvation | M1 | Survey R1 |
| 3 | RAM Ceiling & Resource Governor | Wire `VisualGovernor` (750 MB VLM limiter) into `AppState`/`VisionManager` and enforce dynamic STT/LLM memory mutual exclusion to guarantee RAM ≤ 4.0 GB | M1 | Survey R1 |
| 4 | CsrGraph Batching & Lock Contention Reduction | Batch L3 knowledge graph updates with a dirty-compilation flag instead of running O(V+E) CSR compilation per triple under write locks; replace `blocking_execute` with async execution in async paths | M1 | Survey R1 |
| 5 | Memory Leak & Ring Buffer Bounds | Cap `SessionEventStream.history` with a 500-event ring buffer and add TTL cleanup to `ActiveRecallManager.pending_challenges` to prevent unbounded memory growth | M1 | Survey R1 |
| 6 | In-Memory FactTrie Prefix Search | Implement prefix-tree indexing in `crates/liva-storage` to eliminate full table scans of SQLite `facts` on every conversational turn | M1 | Survey R1 |
| 7 | OffscreenCanvas Avatar Web Worker | Migrate Three.js WebGL2Renderer, `@pixiv/three-vrm`, `GLTFLoader` (ImageBitmapLoader), and 6-step pose pipeline into a dedicated Web Worker (`avatarWorker.ts`) via `canvas.transferControlToOffscreen()` | M2 | Survey R2 |
| 8 | Avatar Worker Bridge Composable | Create `useAvatarWorkerBridge.ts` providing the standard `AvatarEngineApi` interface and postMessage protocol to `VRMEngine.vue` and `WidgetApp.vue` | M2 | Survey R2 |
| 9 | High-Resolution Audio-Viseme Clock Sync | Synchronize Web Audio `AudioContext.currentTime` with worker `performance.now()` via a shared clock delta, ensuring viseme drift strictly < 5ms (< 30ms SLA) | M2 | Survey R2 |
| 10 | Decoupled Interactive Zones Bounding Box | Compute projected avatar AABB bounds in worker and emit throttled `BOUNDS_UPDATED` events to Tauri for click-through window calculation | M2 | Survey R2 |
| 11 | Agentic DAG Workflow Hardening | Validate `StateGraph` error boundaries, encrypted SQLite checkpointing, and `AgentState::trim_history` under multi-step tool and swarm execution | M3 | Survey R3 |
| 12 | Ecosystem Diagnostic Remediation | Add required YAML frontmatter to `LIVA_SYSTEM_AUDIT_AND_ROADMAP_2026.md` for `skills:audit`, and align `data/liva-config.json` model paths for `doctor` | M3 | Survey R3 |
| 13 | Rust Code Formatting & Clippy Hygiene | Fix formatting diffs in `voice_coordinator.rs` and test files (`cargo fmt --all`); eliminate 12 clippy warnings in `crates/liva-storage` and 1 in `embedding.rs` to reach 0 warnings with `-D warnings` | M3 | Survey R3 |
| 14 | Test Suite Assertion & Tolerance Alignment | Adjust tolerance in Vitest `adversarialAvatarSaccadeChallenge.test.ts:273` (`toBeCloseTo(..., 3)`); fix off-by-one assertion in `voice_coordinator_adversarial_stress.rs:167` and align latency assertion in `wake_empirical_benchmark.rs:187` | M3 | Survey R3 |
| 15 | Dual-Track Full Verification & Victory Audit | Run and verify 100% pass across all 8 quality gates (`cargo test --workspace`, `clippy`, `fmt`, `vue-tsc`, `npm run test:coverage -w liva-ui`, `doctor`, `skills:audit`, Forensic Auditor clean verdict) | M4 | Survey R1-R4 |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| **M1** | **Core Engine Runtime, Streaming & Resource Guardrails (R1)** | Features 1, 2, 3, 4, 5, 6: Real-time LLM token streaming, non-blocking inference thread, RAM ≤ 4.0 GB guardrails, lock contention reduction, memory leak fixes, and `FactTrie` | none | DONE |
| **M2** | **3D Avatar Web Worker OffscreenCanvas & Low-Latency IPC (R2)** | Features 7, 8, 9, 10: OffscreenCanvas Web Worker, bridge composable, <5ms viseme sync, and decoupled click-through zones | M1 | DONE |
| **M3** | **Agentic Workflows Hardening & Quality Gate Remediation (R3)** | Features 11, 12, 13, 14: Agentic DAG workflows, `skills:audit` frontmatter, `cargo fmt`, `clippy` 0 warnings, and test tolerances | M1 | DONE |
| **M4** | **Dual-Track Acceptance Verification & Forensic Victory Audit (R4)** | Feature 15: Full 8-gate automated test suite execution, adversarial stress verification, and independent Forensic Integrity clearance | M1, M2, M3 | DONE |

---

## Interface Contracts

### 1. LLM Real-Time Streaming Contract (`crates/liva-llm` & `liva-native-core`)
```rust
pub enum LlmCommand {
    GenerateText {
        prompt: String,
        priority: Priority,
        token_tx: Option<tokio::sync::mpsc::Sender<String>>,
        responder: tokio::sync::oneshot::Sender<anyhow::Result<String>>,
    },
    Shutdown,
}
```

### 2. Main UI Thread $\leftrightarrow$ Avatar Worker Message Protocol (`liva-ui`)
```typescript
export type AvatarWorkerInbound =
  | { type: 'INIT'; canvas: OffscreenCanvas; width: number; height: number; dpr: number; modelPath: string }
  | { type: 'RESIZE'; width: number; height: number; dpr: number }
  | { type: 'VISIBILITY_CHANGE'; visible: boolean }
  | { type: 'SYNC_CLOCK'; clockOffset: number }
  | { type: 'SCHEDULE_CHUNK'; startTimeSec: number; durationSec: number }
  | { type: 'SET_VISEME_TIMELINE'; turnEpoch: number; cues: Array<{ tMs: number; v: string }> }
  | { type: 'AUDIO_RMS'; bands: Float32Array }
  | { type: 'FLUSH'; seq_id?: number }
  | { type: 'SET_LOCOMOTION'; state: 'idle' | 'walk' | 'run' | 'dangle'; motionWeight: number }
  | { type: 'LOOK_AT'; yaw: number; pitch: number }
  | { type: 'DISPOSE' };

export type AvatarWorkerOutbound =
  | { type: 'READY'; format: 'vrm' | 'fbx'; hasClips: boolean }
  | { type: 'BOUNDS_UPDATED'; bounds: { x: number; y: number; width: number; height: number } }
  | { type: 'FPS_METRICS'; currentFps: number; drawCalls: number; frameTimeMs: number }
  | { type: 'ERROR'; message: string; stack?: string };
```

---

## Code Layout

```
LIVA/
├── Cargo.toml                         # Master workspace manifest
├── crates/
│   ├── liva-core-types/               # Domain models, errors, permissions, IPC contracts
│   ├── liva-storage/                  # SQLite WAL pools, micro-batching, FactTrie prefix search
│   ├── liva-llm/                      # LlmActor worker with streaming token channel & 3-tier priority
│   └── liva-tools/                    # Consolidated diagnostic, probe, and benchmark CLI
├── liva-native-core/                  # Top-level orchestration facade
│   ├── src/
│   │   ├── lib.rs                     # AppState (owns LlmActorHandle, VoiceCoordinator, governors)
│   │   ├── boot.rs                    # Bootloader initializing actors on dedicated worker threads
│   │   ├── governor.rs                # VisualGovernor for VLM mutual exclusion
│   │   ├── cognitive/events.rs        # SessionEventStream with ring-buffer capacity bounds
│   │   ├── active_recall.rs           # ActiveRecallManager with TTL cleanup
│   │   └── agent/                     # StateGraph DAG execution & SqliteCheckpointer
│   └── tests/
│       └── harness.rs                 # Consolidated test harness
├── liva-desktop/                      # Tauri v2 Desktop Application Shell
│   └── src-tauri/                     # Native Tauri commands & capabilities
└── liva-ui/                           # Vue 3 Frontend Application
    └── src/
        ├── workers/
        │   └── avatarWorker.ts        # Dedicated Web Worker running Three.js/VRM on OffscreenCanvas
        ├── composables/
        │   ├── useAvatarWorkerBridge.ts # Main-thread bridge wrapping worker
        │   ├── use3DModel.ts          # Humanoid pose math & procedural kinematics
        │   ├── useAvatarAnimation.ts  # Stride distance calibration & FootPlantIK
        │   └── useWidgetTransport.ts  # Tauri v2 Channel subscriber
        └── WidgetApp.vue              # Main chat & widget UI
```

---

## Governance & Safety Constraints
- **Sequential Tooling Discipline**: All cargo commands must execute sequentially passing `-j 2` (`cargo check -j 2`, `cargo build -j 2`, `cargo test -j 2 -- --test-threads 2`).
- **RAM Guardrails**: Available machine RAM must remain ≥ 4.0 GB prior to heavy test suites. Never launch background MCP indexers or heavy graph analyzers.
- **Git Safety Boundary**: Git operations end strictly at staging (`git add`). Autonomous `git commit` or `git push` are strictly prohibited.
- **Integrity Enforcement**: No hardcoded test passes or dummy facades. All implementations must be genuine.
