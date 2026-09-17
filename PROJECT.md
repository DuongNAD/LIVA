# Project: LIVA Architecture Blueprint 2026 Upgrade (LIVA-ARCH-BLUEPRINT-2026)

## Architecture
- **Unified Native Engine (Rust)**: `liva-native-core` and `liva-desktop/src-tauri`. Single source of truth for all backend logic, database connection pooling, SQLite WAL, vector/FTS search, audio pipelines (VAD, denoise, STT, TTS), and local LLM runtime.
- **Desktop UI (Vue 3 + TypeScript + Three.js)**: `liva-ui` bundled into Tauri desktop shell. Implements transparent widget window, 3D avatar VRM additive kinematics & phoneme lip-sync, reactive streaming dialogue, and system dashboard.
- **Subsystem 1 — Hierarchical Memory & Temporal GraphRAG**:
  - L3 Knowledge Graph multi-hop associative retrieval via HippoRAG PPR on In-Memory `CsrGraph` Cache (P95 < 10.0ms, 0 LLM tokens during traversal).
  - AES-256-GCM decryption in `memory_consolidation.rs` before `triple_extractor`.
  - Serialized writes exclusively through `DbActorHandle` bounded MPSC queue (eliminating `SQLITE_BUSY` and silent drop turns).
  - Multi-threaded vector embedding via lock-free `Arc<EmbeddingEngine>` pool (`&self`).
  - Temporal Memory Decay with Ebbinghaus math and lower floor clamping (`clamp(0.05, 1.0)`).
- **Subsystem 2 — Full-Duplex Low-Latency Voice Engine**:
  - Two-Stage Adaptive Turn-Taking Gate (Stage 1 Fast Cut @ 200ms $p>0.92$; Stage 2 Vietnamese Pause Buffer @ 200-450ms $0.50 \le p \le 0.92$; Stage 3 Timeout @ 450ms) using Smart Turn v3.2 ONNX model.
  - AEC3 / WASAPI loopback capture using `cpal 0.15.3` feeding `push_loopback_render` and causal GTCRN STFT denoiser for < 100ms barge-in detection during loud speaker playback.
  - Elimination of frontend 400ms wake mute hack.
  - Clause Streaming TTS (`TtsChunker` 2-9 words) + 150ms client jitter buffer + `OP_VISME (0x06)` phoneme packets (Fast Voice P90 < 480ms).
- **Subsystem 3 — Avatar 3D VRM Additive Kinematics & Screen Vision**:
  - Deterministic 6-step pose pipeline with additive offsets preserving breathing and eye saccades (2-4 Hz, ±0.85° micro-movements).
  - Distance-based stride calibration (`currentSpeed * delta / STRIDE_LENGTH`) eliminating foot-slide.
  - Dedicated Web Worker isolation for MediaPipe FaceLandmarker via Transferable ImageBitmap keeping 60 FPS.
  - Screen vision SIMD Diff ROI cropping (`compute_roi_patch`) with co-scale fallback to 720p when ROI > 35%, visual tokens 144-384, VLM inference < 350ms.
- **Subsystem 4 — Local Agent Graph DAG & SLM Routing**:
  - Tokio `StateGraph` DAG with per-node intermediate hop SQLite WAL checkpointing (`SqliteCheckpointer`) with AES-256-GCM encryption for crash recovery.
  - RouteLLM embedding centroids for query complexity classification (`phan_loai_do_kho`).
  - Semantic Top-K tool retrieval (`DEFAULT_TOP_K = 7`) with minimal 2-line prompt format and strict `ExecPolicy`.
- **Governance & Safety Constraints**:
  - Windows 10/11 x64: RAM <= 4.0 GB (target ~3,000 MB), VRAM <= 5,100 MB.
  - Sequential cargo execution strictly with `-j 2` and `-- --test-threads 2`.
  - Git boundary ends strictly at staging (`git add`).

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Memory Consolidation AES Decryption | Decrypt `vectors_meta.content` before calling `triple_extractor` in `memory_consolidation.rs` to enable real turn knowledge graph extraction | M1 | Memory Survey |
| 2 | DbActor Single-Writer Discipline | Route direct `pool.writer.get()` checkouts in `agent/memory.rs`, `active_recall.rs`, `memory_consolidation.rs`, `db.rs`, and `persistence_backup.rs` through `DbActorHandle` | M1 | Memory Survey |
| 3 | Lock-Free Arc Embedding Pool | Convert `AppState.embedder` from `Mutex<Option<EmbeddingEngine>>` to `Arc<EmbeddingEngine>` using `ort 2.0.0-rc.11` thread-safe `Session::run(&self)` | M1 | Memory Survey |
| 4 | Ebbinghaus Decay Lower Floor Clamping | Add `clamp(0.05, 1.0)` lower bound to `decay_factor` in `db.rs:1737` to preserve long-term historical memories | M1 | Memory Survey |
| 5 | Two-Stage Adaptive Turn-Taking Gate | Activate Smart Turn v3.2 ONNX model from shadow mode into active turn gating (Stage 1 @ 200ms $p>0.92$, Stage 2 @ 200-450ms $0.50 \le p \le 0.92$, Stage 3 @ 450ms) | M2 | Voice Duplex Survey |
| 6 | WASAPI Loopback Capture for AEC3 | Implement `WasapiLoopbackCapturer` using `cpal` to capture Windows audio loopback and feed `push_loopback_render`, enabling AEC3 by default | M2 | Voice Duplex Survey |
| 7 | Frontend Barge-In Unmuting | Remove client-side `isWakeWordMuted()` and 400ms echo tail block in `useVoicePipeline.ts` | M2 | Voice Duplex Survey |
| 8 | Clause Streaming Fast Voice SLA | Ensure `TtsChunker` (2-9 words) + 150ms client jitter buffer + `OP_VISME` meet Fast Voice P90 < 480ms SLA | M2 | Voice Duplex Survey |
| 9 | Screen Vision SIMD Diff ROI Integration | Wire `compute_roi_patch` and `downsample_rgb_to_720p` from `diff.rs` into `commands/vision.rs` and `capture_for_vision()`, capping tokens at 144-384 and VLM TTFT < 350ms | M3 | Avatar/Vision Survey |
| 10 | Avatar Fixational Eye Micro-Saccades | Add deterministic eye micro-saccades (2-4 Hz, ±0.85°) in step 5 of humanoid pose pipeline in `use3DModel.ts` | M3 | Avatar/Vision Survey |
| 11 | FaceLandmarker Web Worker Test Harness | Add automated Vitest unit tests in `liva-ui/tests/workers/faceTrackingWorker.test.ts` | M3 | Avatar/Vision Survey |
| 12 | StateGraph Intermediate Checkpointing | Add per-node intermediate hop checkpointing inside `StateGraph::run()` in `agent/graph.rs` to persist execution across interruptions | M4 | Agent Graph Survey |
| 13 | RouteLLM Embedding Centroids Classifier | Upgrade `phan_loai_do_kho` in `complexity.rs` from heuristic regex/keywords to RouteLLM embedding centroids | M4 | Agent Graph Survey |
| 14 | E2E Dual-Track Test Suite (Tiers 1-4) | Opaque-box test cases for Memory, Voice, Vision/Avatar, and Agent Graph | M5 / Test Track | Blueprint Verification |
| 15 | Tier 5 Adversarial Coverage Hardening | White-box stress-testing, concurrent write pressure, barge-in simulation, and memory budget audits | M5 | Blueprint Verification |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Hierarchical Memory & Temporal GraphRAG | Features 1, 2, 3, 4: Decrypt in consolidation, DbActor single-writer discipline, Arc embedding pool, Ebbinghaus floor clamp | none | DONE |
| M2 | Full-Duplex Low-Latency Voice Engine | Features 5, 6, 7, 8: Smart Turn Two-Stage Gate, WASAPI loopback AEC3, unblock frontend barge-in, Fast Voice SLA | none | DONE |
| M3 | Screen Vision SIMD Diff ROI & Avatar Saccades | Features 9, 10, 11: SIMD Diff ROI & 720p co-scale wiring, Avatar eye saccades, FaceLandmarker worker test | none | DONE |
| M4 | Local Agent Graph Checkpointing & RouteLLM | Features 12, 13: Per-node StateGraph WAL checkpointing, RouteLLM embedding centroids | M1 | DONE |
| M5 | Final Milestone: 100% E2E Quality Gates & Tier 5 Hardening | Features 14, 15: Pass 100% Quality Gates (Cargo check/clippy/test, vue-tsc, eslint, vitest, doctor, skills:audit) + Tier 5 Adversarial Hardening | M1, M2, M3, M4 | DONE |

## Prompt Budget, Streaming Cancellation & Runtime Suites (2026-09-17)
- Exact token budgeting (`ContextTokenBudget` + `compile_exact_budgeted_prompt`) được nối vào cả bốn caller LLM (`handle_chat_completion_scoped`, graph closure, `task_plan_chat`, dialogue WebSocket) qua `LlamaRouterManager::generate_budgeted_completion`; không còn fallback âm thầm bỏ qua budgeting.
- `CompletionStream` (engine.rs): phát hiện hủy stream ngay trên heartbeat reasoning ẩn; stream bị hủy không còn được ghi telemetry/báo thành công.
- Đường vision đo tổng mtmd thật (`chunks.total_tokens()`) sau compile template; persona + ảnh + câu hỏi hiện tại là mandatory (từ chối có cấu trúc, không âm thầm cắt).
- Bằng chứng (53 test model-free pass; runtime text-path thật trên gemma-4-E2B: 1 pass × 2 lượt release; red-gate fail-loud xác minh; vision runtime BLOCKED do không có mmproj trên máy): `docs/03-danh-gia/vision-prompt-budget-acceptance.md` §8–9. Benchmark P50/P95 và CUDA chưa chạy; trạng thái các milestone trên không đổi.

## Interface Contracts
### 1. Memory Consolidation & Decryption
- `consume_pending_once(db: &DatabasePool, crypto: &EncryptionEngine) -> Result<usize>`
- Content read from `vectors_meta` where `type = "conversation_turn"` must be decrypted via `crypto.decrypt(&ciphertext)?` before passing to `extract_triples`.

### 2. DbActor Single-Writer Contract
- Direct calls to `DatabasePool.writer.get()` are forbidden in application code.
- All writes must execute via `DbActorHandle`:
  - `save_agent_checkpoint(thread_id, state_json)`
  - `reinforce_memories(vec_ids, now_ms)`
  - `insert_l3_triple(subject, predicate, object)`
  - `checkpoint_wal()`

### 3. Thread-Safe Embedding Engine
- `EmbeddingEngine` methods: `embed_query(&self, text: &str) -> Result<Vec<f32>>` and `embed_passage(&self, text: &str) -> Result<Vec<f32>>` taking `&self`.
- `AppState.embedder`: `Arc<EmbeddingEngine>` (or `Arc<dyn ToolEmbedder + Send + Sync>`), eliminating `tokio::sync::Mutex`.

### 4. Smart Turn Two-Stage Gate
- Silence duration < 192ms (6 frames): Accumulate audio.
- Silence duration == 192ms (6 frames): Evaluate Smart Turn ONNX model on accumulated turn audio.
  - If $p > 0.92$: Emit `VadEvent::SpeechEnd` immediately (Stage 1 Fast Cut ~204ms).
  - If $0.50 \le p \le 0.92$: Enter Stage 2 (Vietnamese Pause Buffer), keep accumulating up to 14 frames (~448ms).
  - If 14 frames reached: Emit `VadEvent::SpeechEnd` unconditionally (Stage 3 Safety Timeout).

### 5. WASAPI Loopback Capture
- `WasapiLoopbackCapturer`: Initializes `cpal` input stream on default render device loopback.
- Mono 16kHz resampled PCM pushed to `session.push_loopback_render(pcm)`.
- `LIVA_AEC_ENABLED` default: `true` on Windows.

### 6. Screen Vision SIMD Diff ROI
- `capture_for_vision()`: Captures screen frame, calls `DiffEngine::compute_roi_patch(&prev, &curr)`.
- If changes detected:
  - If ROI area <= 35% of screen: Crop ROI with 20px padding (visual tokens 144-384).
  - If ROI area > 35%: Bilinear downsample entire frame to max 1280x720 (visual tokens <= 384).
- VLM TTFT latency SLA: < 350ms.

## Code Layout
- `liva-native-core/src/`:
  - `db/`: SQLite connection pool, `csr_graph.rs` (HippoRAG PPR), `db_actor.rs` (write queue)
  - `memory_consolidation.rs`: Background triple extraction & consolidation
  - `llm/`: `embedder.rs` (Arc embedding pool), `tool_calling.rs`
  - `webrtc/`: `vad.rs`, `turn_shadow.rs`, `aec.rs` (WASAPI loopback), `denoise.rs`, `pipeline.rs`
  - `tts/`: `mod.rs` (TtsChunker), `audio.rs`, `vieneu/`
  - `vision/`: `diff.rs` (SIMD Diff ROI & 720p co-scale), `capture.rs`
  - `agent/`: `graph.rs` (StateGraph), `graph/pipeline.rs`, `graph/complexity.rs` (RouteLLM), `memory.rs`
- `liva-ui/`:
  - `src/composables/`: `use3DModel.ts`, `useAvatarAnimation.ts`, `useFaceTracking.ts`, `useVoicePipeline.ts`, `useSpeakerPlayback.ts`
  - `src/workers/`: `faceTrackingWorker.ts`
  - `tests/workers/`: `faceTrackingWorker.test.ts`
