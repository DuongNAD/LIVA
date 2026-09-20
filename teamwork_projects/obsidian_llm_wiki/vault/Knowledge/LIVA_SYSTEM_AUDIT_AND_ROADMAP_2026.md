# LIVA System Audit Baseline & Strategic Architectural Roadmap (2026)

- **Document Type**: Architecture Baseline, Post-Audit Assessment & Future Evolution Roadmap
- **Vault Location**: `Knowledge/LIVA_SYSTEM_AUDIT_AND_ROADMAP_2026.md`
- **Target Platform**: Windows 10/11 x64 (MSVC toolchain, DirectX/WASAPI, CUDA/DirectML)
- **Baseline Version**: Post-Audit Hardened Baseline (2026-09-18)
- **Status**: **APPROVED & VERIFIED (Zero Integrity Violations)**

---

## 1. Current Architectural Baseline (Post-Audit & Remediation)

Following the comprehensive multi-agent system audit, bug remediation, and performance optimization phases, the LIVA project has achieved a fortified, zero-regression operational baseline across its three primary layers: **`liva-native-core`** (Rust native engine), **`liva-desktop/src-tauri`** (Tauri v2 IPC bridge), and **`liva-ui`** (Vue 3 / Three.js desktop client).

```
+---------------------------------------------------------------------------------------------------+
|                                      POST-AUDIT ARCHITECTURAL BASELINE                            |
+---------------------------------------------------------------------------------------------------+
|  FRONTEND (`liva-ui`)                                                                             |
|  - Vue 3 + TypeScript 5.6 + Pinia reactive state                                                  |
|  - ErrorBoundary crash shield (`onErrorCaptured`) & global Vue error handler                      |
|  - Three.js VRM avatar engine with 30 FPS eco-mode idle throttling (33ms RAF interval)            |
|  - Normalized multi-listener Set callback registry with explicit unlisten handlers                |
|  - Adaptive window reflow throttling (150ms -> 500ms when stationary)                             |
|  - Muted audio gain loopback preventing microphone feedback to speakers                           |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 | Tauri IPC (Dual-envelope token extraction)
                                                 v
+---------------------------------------------------------------------------------------------------+
|  TAURI IPC BRIDGE (`liva-desktop/src-tauri`)                                                      |
|  - Tauri v2 IPC command dispatch with permission capabilities (`widget`, `dashboard`, `setup`)    |
|  - Windows multi-monitor cursor hit-test normalization (`cursor_pos.x / scale_factor`)            |
|  - Process-wide Stronghold snapshot synchronization (`VAULT_FILE_LOCK: Mutex<()>`)                |
|  - Parity in dynamic window recreation (`.decorations(false)`, `.min_inner_size(900.0, 600.0)`)   |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 | In-Process FFI & Memory Bus
                                                 v
+---------------------------------------------------------------------------------------------------+
|  NATIVE ENGINE (`liva-native-core`)                                                               |
|  - Single-Writer Actor (`DbActor`) with 1024 bounded channel, connection reuse & batch inserts    |
|  - SQLite WAL optimized PRAGMAs: 64MB cache, 256MB mmap, 64MB journal limit, 5s busy timeout      |
|  - Non-blocking fact access tracking (`tx.try_send`) eliminating Tokio worker thread sleeps       |
|  - Atomic LLM auto-swap & inference under `ss.llm.blocking_lock()` eliminating TOCTOU races       |
|  - Poison-safe Scoped Tool Registry (`unwrap_or_else(|e| e.into_inner())`)                         |
|  - Bounded binary dictionary parsing in G2P (`read_u32_at` with checked math)                     |
|  - Windows DPAPI null pointer and zero-length buffer validation with `LocalFree` cleanup          |
|  - Shared WASAPI audio loopback stream manager (`SharedLoopbackManager`)                          |
|  - Zero-copy float audio sample serialization (`bytemuck::cast_slice::<f32, u8>`)                 |
+---------------------------------------------------------------------------------------------------+
```

### 1.1 Verified System Metrics
- **Rust Core Test Suite**: 738 unit tests passed (0 failed, 3 ignored), 32 integration tests passed (100% pass).
- **Tauri IPC Test Suite**: 23/23 tests passed (100% pass across capability matrix, streaming stress, and Stronghold migration).
- **Frontend Vitest Suite**: 50 test files passed, 550 tests passed (100% pass).
- **Database Concurrency Stress**: 650 concurrent writes across 100 async tasks and 30 sync OS threads with 20 WAL checkpoints executed without a single `SQLITE_BUSY` error or timeout.
- **Resource Footprint**: Native Core idle RAM ~185 MB (limit: <= 350 MB); full voice pipeline < 1.2 GB (limit: <= 2.0 GB); Three.js idle GPU load reduced by 50-87%.

---

## 2. Phased Architectural Upgrade Roadmap (2026)

Building upon the fortified post-audit baseline, this roadmap outlines four distinct, sequential upgrade phases designed to elevate LIVA into a state-of-the-art, enterprise-grade personal AI assistant on Windows 10/11 x64.

```
+---------------------------------------------------------------------------------------------------+
|                                 LIVA 2026 STRATEGIC UPGRADE PHASES                                |
+---------------------------------------------------------------------------------------------------+
| Phase 1: Native Core & Voice Pipeline Hardening                                                   |
| Real-time full-duplex WebRTC | AEC3 tuning | Wake Word v3 | Whisper.cpp CUDA | Clause TTS         |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 v
+---------------------------------------------------------------------------------------------------+
| Phase 2: Hierarchical Memory & Knowledge Graph Evolution                                          |
| 4-tier L0-L3 memory compaction | HippoRAG PPR graph traversal | SQLite-vec HNSW | Schema guards   |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 v
+---------------------------------------------------------------------------------------------------+
| Phase 3: Desktop UI & Multimodal Expansion                                                        |
| Multi-window ghost mode | Web Worker MediaPipe | Distance stride kinematics | SIMD Diff ROI vision |
+---------------------------------------------------------------------------------------------------+
                                                 |
                                                 v
+---------------------------------------------------------------------------------------------------+
| Phase 4: Enterprise Security & Governance                                                         |
| Cross-process Stronghold locks | PDG taint tracking | Two-phase HITL confirmation | PII masking   |
+---------------------------------------------------------------------------------------------------+
```

---

### Phase 1: Native Core & Voice Pipeline Hardening

**Objective**: Deliver a seamless, natural, full-duplex offline voice conversation loop on Windows x64 with sub-450ms turn-taking latency and instant barge-in interruption.

#### 1. Real-Time Duplex Streaming & WebRTC Hardening
- **Loopback Audio Pipeline**: Harden the `SharedLoopbackManager` in `webrtc/aec.rs` to support dynamic Windows audio endpoint switching (e.g., unplugging headphones or switching to external USB audio DAC) without restarting the desktop application.
- **Acoustic Echo Cancellation (AEC3)**: Calibrate WebRTC AEC3 filter delay estimators specifically for Windows WASAPI shared audio render latencies (typically 10ms–25ms audio buffers).
- **GTCRN Neural Denoising**: Integrate lightweight causal STFT-domain neural noise suppression (GTCRN ONNX runtime) running on CPU/DirectML with <15ms latency per frame.

#### 2. Wake Word Engine Optimization ("Hey Liva")
- **Architecture**: Deploy a streaming micro-conformer or TC-ResNet ONNX model (<15 MB footprint) with continuous rolling audio frame classification.
- **Accuracy Targets**:
  - False Positive Rate per Hour (FPPH) < 1.0 in noisy environments (TV, background conversation).
  - Recall >= 90% on both standalone ("Hey Liva") and prepended ("Hey Liva, check my schedule") utterances.
  - Activation latency <= 150ms from keyword termination.

#### 3. Two-Stage Adaptive Turn-Taking & Barge-in
- **Silero VAD v5 + Smart Turn v3.2**: Implement a two-stage speech end detector. Stage 1 monitors physical audio energy and VAD speech probability; Stage 2 evaluates trailing phoneme/text semantics via a lightweight classifier.
- **Latency Target**: Reduce SpeechEnd cutoff latency from ~704ms down to **200ms–450ms** without clipping natural Vietnamese hesitation pauses ("à...", "ừm...").
- **Instant Barge-in Interruption**: When the user speaks while LIVA is playing audio through TTS:
  1. VAD flags voice activity within 80ms.
  2. Native core immediately emits `OP_CANCEL_PLAYBACK` to Tauri/Frontend.
  3. Audio playback stops, and state switches to `Listening` within **< 150ms**.

#### 4. High-Performance Local ASR & TTS
- **Whisper.cpp / GGML Quantization**: Offload streaming speech recognition to quantized GGML models (INT8/INT4) leveraging CUDA or DirectML on Windows.
- **Clause-Streaming TTS**: Tokenize LLM output by punctuation boundaries (commas, periods) and synthesize audio clauses concurrently, streaming audio chunks to the frontend with a 150ms client jitter buffer for First-Chunk Latency (TTFB) < 300ms.

---

### Phase 2: Memory & Knowledge Graph Evolution

**Objective**: Evolve the 4-tier memory architecture into an associative, self-compacting knowledge engine that retrieves context in sub-10ms without exhausting LLM context budgets.

```
+---------------------------------------------------------------------------------------------------+
|                                   4-TIER HIERARCHICAL MEMORY ARCHITECTURE                         |
+---------------------------------------------------------------------------------------------------+
| [L0] Working Context Buffer     | In-memory rolling prompt window (dynamic token pruning)        |
| [L1] Episodic Session Memory    | SQLite WAL turns, recent dialogue history, active recall buffer |
| [L2] Dynamic Profile & Semantic | User preferences, habits, cached vector embeddings (HNSW)       |
| [L3] Temporal Knowledge Graph   | Multi-hop entity-relation triples (HippoRAG PPR graph traversal)|
+---------------------------------------------------------------------------------------------------+
```

#### 1. 4-Tier Memory Lifecycle & Compaction
- **L0 Working Context**: Governed by `DynamicPromptAssembler`, dynamically allocating token quotas across system persona, retrieved skills, memory facts, and recent conversation turns to prevent `n_ctx` overflow.
- **L1 Episodic Memory**: Automatic session boundary detection and conversation turn compression after 15 minutes of idle time.
- **L2 Semantic Cache**: Vectorized storage of user facts with cosine similarity deduplication, preventing duplicate fact insertions.
- **L3 Knowledge Graph**: Entity-Attribute-Value (EAV) triples stored with bidirectional graph edges.

#### 2. Temporal Memory Decay & HippoRAG PPR Traversal
- **Power-Law Memory Decay**: Model fact recency using an exponential decay function:
  $$S(t) = S_0 \cdot e^{-\lambda (t - t_0)} + R$$
  where $R$ is reinforced upon each retrieval.
- **HippoRAG Personalized PageRank (PPR)**: Implement in-memory Compressed Sparse Row (CSR) graph indexing in Rust. Multi-hop associative fact retrieval traverses up to 3 graph hops in **P95 < 10ms**, extracting linked entities without wasting LLM reasoning tokens on graph traversal.

#### 3. Hybrid FTS5 + Vector Search Indexing
- **SQLite-vec / HNSW Integration**: Combine SQLite FTS5 BM25 text search with `sqlite-vec` HNSW vector embeddings for hybrid retrieval:
  $$\text{Score} = \alpha \cdot \text{BM25}(q, d) + (1 - \alpha) \cdot \text{Cosine}(v_q, v_d)$$
- **Schema Migration Guardrails**: Formalize database schema versioning in `liva-native-core/src/db.rs` with automatic rollback on migration failure.

---

### Phase 3: Desktop UI & Multimodal Expansion

**Objective**: Elevate the desktop experience with ultra-fluid 3D avatar animations, multi-window transparent overlays, and responsive computer vision screen grounding.

#### 1. Multi-Window Transparent Ghost Mode
- **Native OS Hit-Testing**: Expand the normalized cursor hit-testing logic (`rx = cursor_pos.x / scale_factor`) to support multiple floating desktop widgets across arbitrary multi-monitor layouts (mixed DPI: 100%, 125%, 150%, 200%).
- **Window Hierarchy**: Isolate the 3D avatar into an unbordered, transparent overlay (`WS_EX_LAYERED | WS_EX_TRANSPARENT`) while the chat drawer and dashboard operate as interactive floating tools.

#### 2. Web Worker Face Tracking & Additive Pose Kinematics
- **MediaPipe Offloading**: Move MediaPipe `FaceLandmarker` and facial mesh tracking completely into a Web Worker, ensuring zero WebGL render thread interruptions and maintaining a steady 60 FPS UI frame rate.
- **Deterministic Additive Pose Pipeline**: Maintain the strict 6-step humanoid pose execution order:
  1. Base retargeted clip evaluation.
  2. Upper-body rest pose reset.
  3. Distance-based stride locomotion (stride phase calibrated to actual movement velocity, eliminating foot-sliding).
  4. Additive idle breathing (sine oscillation) and OpenSimplex micro-sway.
  5. Eye saccades, spring look-at, and viseme lip-sync blendshapes.
  6. VRM spring-bone secondary physics evaluation.

#### 3. Real-Time Screen Vision & ROI SIMD Extraction
- **SIMD Diff Region-of-Interest (ROI)**: When the user asks visual questions about their screen ("Look at this error"), capture the desktop screen via Windows Desktop Duplication API (DirectX GI) and compute a pixel-diff matrix via SIMD.
- **Co-Scale Fallback**: If the changed ROI exceeds 35% of the display, downscale adaptively to 720p, capping visual tokens between 144 and 384 tokens to keep local VLM inference latency **< 350ms**.

---

### Phase 4: Enterprise Security & Governance

**Objective**: Enforce military-grade data protection, hardware-backed encryption, and regulatory compliance for enterprise desktop deployment.

#### 1. Hardware-Backed Stronghold & Cross-Process Synchronization
- **TPM2 / DPAPI Key Sealing**: Seal Stronghold snapshot master keys directly against the local machine's TPM2 or Windows DPAPI keystore, ensuring vault files cannot be decrypted on another machine if exfiltrated.
- **Cross-Process File Mutex**: Upgrade `VAULT_FILE_LOCK` from an in-process `Mutex<()>` to a named Windows OS Mutex (`CreateMutexW`), preventing race conditions across multi-process launcher instances.

#### 2. Program Dependence Graph (PDG) & Taint Tracking
- **Data Flow Sanitization**: Implement static taint tracking on all incoming tool arguments. Parameters sourced from unauthenticated web or chat inputs are tagged as *Tainted* and cannot reach dangerous system sinks (e.g., shell command execution, file deletion) without passing through a sanitization barrier.
- **Scoped Tool Sandboxing**: Enforce declarative tool capabilities per agent turn (e.g., read-only filesystem access vs read-write access).

#### 3. Two-Phase Confirmation Protocol (Human-in-the-Loop)
- **Sensitive Action Interception**: Any external tool invocation classified as High Impact (e.g., sending emails, modifying financial ledgers, deleting disk directories, changing system settings) requires a two-phase confirmation:
  1. **Phase 1 (Draft & Preview)**: Agent stages the command and generates an interactive confirmation card in the frontend UI.
  2. **Phase 2 (Execution)**: The action is dispatched to the native core only upon explicit user click or voice confirmation.

#### 4. Regulatory Compliance (Decree 13/2023/NĐ-CP & GDPR)
- **On-Premise Privacy Filter**: Implement local regex and NER pattern scanning to mask Personally Identifiable Information (PII) — including Citizen IDs (CCCD), bank account numbers, passwords, and phone numbers — before context is processed by local LLMs or logged to disk.
- **Zero Cloud Leakage Guarantee**: Enforce strict network firewall policies blocking external telemetry transmission.

---

## 3. Technical Debt Backlog & Refactoring Priorities

The post-audit assessment identified several technical debt items and architectural cleanups scheduled for upcoming maintenance sprints.

```
+---------------------------------------------------------------------------------------------------+
|                                 TECHNICAL DEBT BACKLOG MATRIX                                     |
+---------------------------------------------------------------------------------------------------+
| Priority | Category       | Component / Path            | Target Remediation Description          |
+----------+----------------+-----------------------------+-----------------------------------------+
| High     | Code Quality   | liva-native-core/webrtc/aec | Refactor complex nested Mutex/Weak type |
| High     | Bundle Size    | liva-ui/widget.html         | Lazy-load live2d.min.js on demand       |
| Medium   | Performance    | liva-ui/DashboardApp.vue    | Convert 14 subviews to async components |
| Medium   | Code Hygiene   | liva-ui/src/App.vue & main  | Remove dead legacy single-window files  |
| Low      | Tooling        | scripts/                    | Deprecate legacy Node.js migration stubs|
+---------------------------------------------------------------------------------------------------+
```

### 3.1 Detailed Debt Items

#### Item 1: WebRTC AEC Complex Type Refactoring
- **Severity**: High (Code Cleanliness / Clippy Compliance)
- **File**: `liva-native-core/src/webrtc/aec.rs:310, 364`
- **Current State**: Uses raw nested types:
  ```rust
  Arc<Mutex<Vec<std::sync::Weak<Mutex<Option<SelfEchoCanceller>>>>>>
  ```
- **Remediation**: Introduce named type aliases and a dedicated subscriber wrapper struct:
  ```rust
  type WeakAecSubscriber = std::sync::Weak<Mutex<Option<SelfEchoCanceller>>>;
  struct AecSubscriberRegistry(Mutex<Vec<WeakAecSubscriber>>);
  ```
- **Benefit**: Resolves `clippy::type_complexity` warnings and improves code readability.

#### Item 2: Live2D Script Lazy-Loading
- **Severity**: High (Memory & Startup Footprint)
- **File**: `liva-ui/widget.html:13`
- **Current State**: Loads 151 KB `live2d.min.js` synchronously via `<script>` tag on every widget launch, even when 3D VRM is the active engine.
- **Remediation**: Dynamically inject the script tag inside `Live2DEngine.vue` only when the user explicitly selects Live2D in settings.
- **Benefit**: Fulfills the architectural guarantee: 3D users consume **0 bytes RAM** for 2D libraries.

#### Item 3: Dashboard Subview Code-Splitting
- **Severity**: Medium (Frontend Bundle Optimization)
- **File**: `liva-ui/src/DashboardApp.vue:26-40`
- **Current State**: Statically imports all 14 subviews (`MemoryViewer.vue`, `BiAnalyticsView.vue`, `SkillsView.vue`, etc.), bloating the initial dashboard bundle to 167 KB.
- **Remediation**: Replace static imports with Vue's `defineAsyncComponent`:
  ```typescript
  const MemoryViewer = defineAsyncComponent(() => import('./components/dashboard/MemoryViewer.vue'));
  ```
- **Benefit**: Reduces initial dashboard chunk parse time by ~60%, loading management views only when navigated to.

#### Item 4: Dead Legacy Entry Points Pruning
- **Severity**: Medium (Repository Hygiene)
- **Files**: `liva-ui/src/App.vue` (497 lines), `liva-ui/src/main.ts` (15 lines)
- **Current State**: Leftover files from the early single-window prototype; neither is referenced in `vite.config.ts` (`widget.html` and `dashboard.html` are the active entry points).
- **Remediation**: Delete both files and remove legacy test references.
- **Benefit**: Prevents confusion for future developers and reduces repository clutter.

---

## 4. Verification & Implementation Governance

All future development cycles must adhere to the verified engineering standards established in this audit:

1. **Pre-flight Health Checks**:
   - System available RAM must be verified `>= 4.0 GB` prior to launching heavy compilations or test suites:
     ```powershell
     Get-CimInstance Win32_OperatingSystem | Select-Object @{Name="FreeGB";Expression={[math]::Round($_.FreePhysicalMemory/1MB,2)}}
     ```
2. **Resource-Constrained Compilation**:
   - Cargo commands must pass `-j 2` and `--test-threads 2`:
     ```powershell
     cargo check -j 2 --workspace
     cargo test -j 2 --workspace -- --test-threads 2
     ```
3. **Sequential Execution Only**:
   - Compilers and test runners must run sequentially. Concurrent multi-agent background compilation is strictly prohibited.
4. **Git Safety Invariant**:
   - Automated agent boundaries end strictly at staging (`git add`). Commit, push, and branch manipulation are user-only privileges.
