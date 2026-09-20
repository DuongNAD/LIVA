<div align="center">

  # LIVA - The AI Assistant 🧠
  *A Versatile Personal Cognitive Desktop Operating System*
  *Next-Gen Remake Architecture (LIVA-REMAKE-2026)*

  [![Architecture](https://img.shields.io/badge/Architecture-Multi--Crate_Workspace-blue.svg)](architecture_remake_proposal.md)
  [![RAM Ceiling](https://img.shields.io/badge/RAM_Ceiling-%E2%89%A4_4.0_GB-green.svg)](AGENTS.md)
  [![Engine](https://img.shields.io/badge/Engine-Rust_Native_/_Tauri_v2-orange.svg)](PROJECT.md)
  [![CI/CD](https://img.shields.io/badge/CI%2FCD-4--Job_Parallel_DAG-brightgreen.svg)](.github/workflows/test.yml)
  [![License](https://img.shields.io/badge/License-Custom_Personal_%26_Internal_Use-red.svg)](LICENSE)

</div>

## 👨‍💻 About the Author & Project Mission
Hello! I'm **Nguyen Anh Duong**, a software engineering student at **FPT University Hanoi**.

**LIVA** (Local Intelligent Virtual Assistant) is a native, privacy-first **Cognitive Desktop Operating System** designed to run fully offline on Windows 10/11 x64 workstations within strict resource budgets:
- **System RAM Ceiling**: $\le 4.0\text{ GB}$ (operational steady-state target $\sim 3.0\text{ GB}$).
- **GPU VRAM Ceiling**: $\le 5.1\text{ GB}$.

The **LIVA-REMAKE-2026** architectural initiative upgrades LIVA from its initial monolithic native prototype into a high-performance, actor-based, multi-crate workspace topology with calibrated database persistence, unified Tauri v2 IPC channels, and parallelized CI/CD.

For complete architectural analysis, forensic bottleneck investigations, and milestone tracking:
- 📖 **[Architecture Remake Proposal](architecture_remake_proposal.md)**: Exhaustive forensic diagnosis of legacy bottlenecks (global LLM mutex contention, SQLite cache bloat, 94 GB target bloat, dual IPC redundancy) and comparative architectural benchmarks.
- 📋 **[Project Specifications & Milestones (PROJECT.md)](PROJECT.md)**: Interface contracts, crate layouts, and verified milestone delivery statuses (M1–M4).

---

## 🚀 Core Remake Architectural Highlights

LIVA's next-generation remake addresses every critical bottleneck identified during forensic codebase audits:

### 1. ⚡ Actor-Based LLM Concurrency (`crates/liva-llm`)
- **Non-Blocking Priority Queue**: Replaced the synchronous global `tokio::sync::Mutex<LlamaRouterManager>` with a bounded Tokio MPSC actor (`LlmActor`) supporting a 3-tier priority queue:
  - `Priority::High`: Instant health checks (`llm:health_check`), parameter inspection, and cancellation signals.
  - `Priority::Normal`: Interactive user dialogue turns and streaming chat completions.
  - `Priority::Low`: Background embedding generation, memory consolidation, and maintenance tasks.
- **Immediate Cancellation & Zero Head-of-Line Blocking**: Health probes and cancellation heartbeats complete in $< 5\text{ ms}$ even while the inference worker is actively generating long token streams.
- **Decoupled Multi-Threaded Embeddings**: Semantic embeddings for long-term memory run lock-free across worker threads via `Arc<EmbeddingEngine>` using thread-safe ONNX Runtime sessions, completely isolated from llama.cpp text completion.
- **SHA-256 Model Trust Verification Cache**: Model integrity hashes are verified once upon installation and cached in SQLite metadata. Re-verifying models on subsequent boots checks fast file size and `mtime` ($< 1\text{ ms}$), eliminating 3–6s cold-start freezes.

### 2. 🗄️ Calibrated SQLite Storage Engine (`crates/liva-storage`)
- **OS-Aligned Page & Cache Sizing**: Standardized 4KB page size (`PRAGMA page_size = 4096;`) aligning with x86_64 memory pages and NTFS physical clusters; calibrated cache size (`PRAGMA cache_size = -2000;`) bounding memory consumption to ~11.7 MB across reader/writer pools—**saving over 300 MB of RAM** compared to uncalibrated defaults.
- **Micro-Batched Writer Actor (`DbActor`)**: Single dedicated writer connection on an isolated thread with transactional coalescing (commits flushed upon 50 operations or 5ms elapsed window). Write throughput increased from 25–40 tx/s to **1,500–3,000 operations/sec**, eliminating SSD I/O stalls.
- **Radix Trie Active Recall Indexing**: In-memory prefix Trie for personal facts keyword filtering (<1ms lookup). Replaced legacy full-table scans and mass AES-256-GCM decryptions; symmetric encryption is decrypted *only* for the 1–3 facts matching the active utterance, slashing query latency from 150ms to **$< 2\text{ ms}$**.
- **Lock-Free HippoRAG CSR Graph**: Knowledge graph associative retrieval running lock-free over `arc_swap::ArcSwap<CsrGraph>`, eliminating write-lock stalls and dirty-cache drops during live knowledge consolidation.
- **Accurate Scoped Vector Retrieval**: Isolated domain vector partitioning resolves `sqlite-vec` subquery post-filtering false negatives, restoring 100% recall accuracy.

### 3. 🌐 100% Native Tauri v2 IPC Channels
- **Retired TCP WebSocket Redundancy**: Completely removed the duplicate loopback TCP WebSocket server (`websocket.rs`, eliminating 2,146 lines of duplicated dispatching code).
- **Zero Port Collisions & Zero Double-Serialization**: Token streams, binary audio chunks, and viseme frames stream directly through native in-process Tauri v2 channels (`tauri::ipc::Channel`), cutting transfer latency to $< 0.1\text{ ms}$ and preventing Windows Firewall port warnings.
- **Compile-Time Type Safety**: TypeScript bindings generated directly from `crates/liva-core-types` via `specta` / `tauri-specta`, guaranteeing schema synchronization between Rust and Vue 3.

### 4. 🎭 OffscreenCanvas Web Worker 3D Avatar
- **Decoupled 60 FPS Render Loop**: Three.js rendering, bone deformation, and VRM spring-bone physics calculations run in `avatarRenderer.worker.ts` via `transferControlToOffscreen()`. Token bursts and reactive DOM re-renders never starve the rendering loop.
- **Event-Driven Ghost Mode**: Windows low-level mouse hooks (`WH_MOUSE_LL`) replace the legacy 33Hz polling loop, dropping idle background CPU overhead to near 0.0%.

### 5. 🛠️ Modernized Multi-Crate Workspace & 4-Job Parallel DAG CI/CD
- **Root Workspace Inheritance**: Standardized `[workspace.dependencies]`, `[workspace.lints]`, and aggressive release optimization (`[profile.release]` ThinLTO, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`). Installer payload size reduced from **291 MB to $< 85\text{ MB}$**.
- **Consolidated Test Harness & CLI**: Replaced 78 standalone integration test binaries with a single submodule harness (`tests/harness.rs`), and consolidated 26 binary probes into `crates/liva-tools` CLI. Local link time dropped from 20–60s to **$< 3\text{ s}$** with MSVC `lld-link`.
- **4-Job Parallel DAG Pipeline**: Modernized `.github/workflows/test.yml` running 3 parallel Ubuntu jobs (Lints, Web/UI, Rust Quality with pre-built `cargo-deny`) gating the Windows native build and NSIS bundler. CI turnaround reduced from $> 60\text{ min}$ to **$\sim 5.5\text{ min}$**.

---

## 🛡️ Hardware & Operational Resource Guardrails

To protect workstation stability, prevent native out-of-memory (OOM) conditions, and guarantee deterministic builds, all environments and agents MUST follow these strict operational rules:

| Constraint | Limit / Rule | Implementation / Command |
|---|---|---|
| **Max System RAM** | $\le 4.0\text{ GB}$ (steady-state $\sim 3.0\text{ GB}$) | SQLite calibrated cache (-2000), Trie indexing, mmap GGUF |
| **Max GPU VRAM** | $\le 5.1\text{ GB}$ | Layer offloading (`LIVA_LLM_N_GPU_LAYERS`), FP16/Q4_K_XL |
| **Sequential Build** | **Strictly sequential execution** | Never launch concurrent compilers or test suites |
| **Compiler Threads** | Capped at 2 concurrent jobs | Always pass `-j 2` to `cargo check`, `cargo build`, `cargo test` |
| **Test Threads** | Capped at 2 worker threads | Always pass `-- --test-threads 2` to cargo test runners |
| **RAM Pre-flight** | Free RAM $\ge 4.0\text{ GB}$ | Check via `Get-CimInstance Win32_OperatingSystem` before heavy runs |
| **Code Search** | Lightweight native ripgrep | `grep_search` / `find_by_name` only; NO heavy graph indexers |
| **Git Safety** | Boundary ends at staging | `git add` only; NO autonomous `git commit` or `git push` |

---

## 🏛️ System Architecture Topology

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
│  - Pure Domain Types │  - SQLite Pool (WAL)  │  - LlmActor Worker  │  - Full-Duplex Audio Engine  │
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

### Workspace Crate Layout

```
LIVA/
├── Cargo.toml                         # Root Cargo workspace manifest with inheritance & release profiles
├── package.json                       # Root npm workspace manifest with supply-chain security overrides
├── .cargo/
│   └── config.toml                    # Fast linker (lld-link), optimization flags (-C target-cpu=native)
├── .github/
│   └── workflows/
│       └── test.yml                   # 4-Job Parallel DAG GitHub Actions workflow
├── crates/
│   ├── liva-core-types/               # Pure domain models, error hierarchies, auth permissions, IPC schemas
│   ├── liva-storage/                  # High-performance persistence, WAL pools, micro-batching, Trie recall
│   │   └── src/ (pragmas.rs, pool.rs, db_actor.rs, trie_recall.rs, csr_graph.rs, vector_search.rs)
│   ├── liva-llm/                      # Non-blocking Actor runtime, model routing, token budgeting, trust cache
│   │   └── src/ (actor.rs, router.rs, prompt_budget.rs, embedder.rs, trust_cache.rs)
│   ├── liva-voice/                    # Low-latency full-duplex speech interaction pipeline
│   │   └── src/ (vad.rs, smart_turn.rs, aec.rs, denoise.rs, stt.rs, tts.rs, viseme.rs)
│   └── liva-tools/                    # Consolidated diagnostic, probe, and benchmark CLI
│       └── src/ (main.rs, commands/db_probe.rs, commands/voice_bench.rs, commands/llm_bench.rs)
├── liva-native-core/                  # Top-level orchestration facade preserving backward compatibility
│   ├── src/lib.rs                     # Public AppState, command routers, bootloader coordinator
│   └── tests/harness.rs               # Single consolidated integration test harness with submodules
├── liva-desktop/                      # Tauri v2 Desktop Application Shell
│   ├── tauri.conf.json                # Multi-window configuration (widget, dashboard, setup)
│   └── src-tauri/src/                 # Tauri native commands, IPC channels, low-level mouse hooks
├── liva-ui/                           # Vue 3 Modular Frontend Application
│   ├── src/workers/                   # avatarRenderer.worker.ts (OffscreenCanvas Web Worker)
│   └── src/composables/               # useTauriIpc.ts (Native Tauri v2 Channel consumers)
└── packages/
    └── liva-common/                   # Shared TypeScript type definitions and protocol constants
```

---

## 📊 Quantitative Remake Benchmarks

Comparison between the legacy monolithic prototype and the Remake Architecture:

| Metric | Legacy Prototype | Remake Architecture | Measured Improvement |
|---|---|---|---|
| **Local `target/` Directory Size** | **94.34 GB** (9,772 build artifacts) | **$< 12.0\text{ GB}$** | **87.3% disk reduction** |
| **Executable Binaries Linked** | **106 binaries** (78 tests + 26 bins + 2) | **2 binaries** (1 harness + 1 tools CLI) | **98.1% fewer link targets** |
| **Local Link Time on Touch** | 20–60s (default MSVC `link.exe`) | **$< 3\text{ s}$** (MSVC `lld-link` & single harness) | **10x–20x faster linking** |
| **CI Pipeline Execution Time** | $> 60\text{ min}$ (sequential Windows VM) | **$\sim 5.5\text{ min}$** (4-Job Parallel DAG) | **91% CI turnaround cut** |
| **SQLite Page Cache RAM Footprint** | **312.5 MB** (5 conns $\times$ 62.5 MB) | **$\sim 11.7\text{ MB}$** (calibrated pragmas) | **300.8 MB RAM saved (96.2%)** |
| **Active Recall Retrieval Latency** | 50–250ms (full table scan + AES) | **$< 2\text{ ms}$** (in-memory Radix Trie) | **50x–100x lower latency** |
| **Database Write Throughput** | 20–50 tx/s (unbatched disk sync) | **1,500–3,000 ops/s** (5ms micro-batching) | **50x write throughput boost** |
| **Health Check Latency During LLM Gen** | 3,000–15,000ms (locked Mutex) | **$< 5\text{ ms}$** (Priority::High actor channel) | **Zero UI freeze / HoL blocking** |
| **3D Avatar Rendering Framerate** | 20–35 FPS (UI thread starvation) | **Locked 60 FPS** (OffscreenCanvas Worker) | **Fluid visual rendering** |
| **Windows Installer Package Size** | **291.2 MB** (unoptimized) | **$< 85.0\text{ MB}$** (ThinLTO + symbol strip) | **70.8% smaller installer** |

---

## 🛠 Step-by-Step Installation & Usage Guide

### Step 1: Prerequisites
- **Operating System**: Windows 10/11 x64.
- **Rust Toolchain**: 1.85 or newer (Rust 2024 edition support) with `x86_64-pc-windows-msvc` target.
- **Node.js**: v20 or newer (npm workspaces support).
- **C++ Build Tools**: Visual Studio 2022 C++ x64 Build Tools with `lld-link` (LLVM linker component).
- **CMake & LLVM**: CMake $\ge 3.24$ and LLVM/Clang. Ensure `LIBCLANG_PATH` points to your LLVM `bin` directory.
- **Hardware Requirements**: Minimum 16 GB system RAM. NVIDIA GPU (CUDA 12.x) with 8 GB+ VRAM recommended for hardware-accelerated LLM and Vision inference; CPU inference supported via `LIVA_LLM_N_GPU_LAYERS=0`.

### Step 2: Clone and Setup

```powershell
# 1. Clone the repository
git clone https://github.com/DuongNAD/LIVA.git
cd LIVA

# 2. Install Node.js dependencies with clean supply-chain overrides
npm ci

# 3. Fetch required model weights (gitignored)
npm run setup:models          # downloads minimal profile; add -- --profile full for full speech stack

# 4. Run preflight health diagnostics
npm run doctor                # verifies required model weights, ONNX runtimes, and paths
```

### Step 3: Configure Environment Variables

> ⚠️ **Configuration Notice**: The Rust native core does not load `.env` files dynamically. Configure variables in your PowerShell environment prior to launching:

```powershell
# Optional GPU and context window tuning
$env:LIVA_LLM_N_GPU_LAYERS = "99"      # Offload all layers to NVIDIA GPU (set to 0 for CPU-only)
$env:LIVA_LLM_N_CTX = "4096"            # Context window token ceiling (default: 4096)
$env:LIVA_WAKE_MODE = "asr_prefix"      # Voice wake mode
```

Inspect [`.env.example`](.env.example) and [`docs/02-van-hanh/01-cau-hinh-va-bien-moi-truong.md`](docs/02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) for full configuration reference.

### Step 4: Build & Verify Workspace (Sequential Guardrails)

Always execute compiler commands sequentially adhering to the bounded resource flags:

```powershell
# 1. Workspace compilation check (0 errors required)
cargo check --workspace -j 2

# 2. Verify desktop shell test linking
cargo test -p liva-desktop -j 2 --no-run

# 3. Run core integration test suite
cargo test -p liva-native-core -j 2 -- --test-threads 2

# 4. Validate GitHub Actions CI/CD workflows
node scripts/actionlint.mjs
npm run test:actionlint

# 5. Run consolidated diagnostics CLI
cargo run -p liva-tools -j 2 -- --help
```

### Step 5: Launch the Application

```powershell
npm run dev
```

The automated bootstrap script (`scripts/start_all.ps1`):
1. Verifies port availability and checks running instances.
2. Spawns the Vue 3 frontend Vite development server (`liva-ui`, port 5173).
3. Boots the Tauri v2 Desktop Application (`liva-desktop`) hosting the native core directly in-process with unified channel streaming.

---

## 🖼️ System Screenshots & Demo

<p align="center">
  <img src="docs/assets/ghost_mode_widget.png" width="68%" alt="Ghost Mode Overlay Widget">
  <img src="docs/assets/liva_avatar.png" width="30%" alt="LIVA 3D Avatar">
</p>
<p align="center">
  <img src="docs/assets/memory_space.png" width="48%" alt="Memory Space Dashboard">
  <img src="docs/assets/avatar_gallery.png" width="48%" alt="Avatar Gallery">
</p>

### 🎬 Unedited Video Demonstration
▶ **[Watch or download the continuous session recording (MP4, 7.9 MB)](https://github.com/DuongNAD/LIVA/releases/tag/demo-2026-07)**

---

## 📚 Documentation Hub

The project maintains comprehensive, single-source documentation:

- 📑 **[Documentation Index (docs/README.md)](docs/README.md)** — Master index across all architectural, operational, and research tiers.
- 📐 **[Architecture Remake Proposal](architecture_remake_proposal.md)** — Full forensic analysis, bottleneck dissection, and comparative engineering decisions.
- 🎯 **[Milestone & Interface Contracts (PROJECT.md)](PROJECT.md)** — Detailed specification, milestone tracking (M1–M4), and interface contracts.
- ⚙️ **[Environment Variables & Runtime Operations](docs/02-van-hanh/01-cau-hinh-va-bien-moi-truong.md)** — Complete configuration documentation.
- 🧠 **[Obsidian Knowledge Base Vault](teamwork_projects/obsidian_llm_wiki/vault/)** — Source of truth for cognitive rules, domain skills, and architectural designs.

---

## 🏁 Remake Milestones & Delivery Status

| Milestone | Scope | Key Deliverables | Status |
|---|---|---|---|
| **M1: Workspace & CI/CD** | Root Cargo, CI, Test Harness, Tools CLI | Workspace dependencies, `lld-link`, single test harness, `liva-tools` CLI, 4-Job Parallel DAG CI, npm audit fix | **DONE** |
| **M2: Database & Storage** | Calibrated SQLite, Batched Actor, Radix Trie | 4KB pages, cache_size=-2000, 50-op/5ms batching, Radix Trie active recall, ArcSwap CSR graph, scoped vectors | **DONE** |
| **M3: LLM & IPC** | Actor Concurrency, Trust Cache, Tauri Channels | 3-tier Priority MPSC LlmActor, SHA-256 trust caching, WebSocket retirement, native Tauri v2 channels, Worker Avatar | **DONE** |
| **M4: Documentation & Verification** | Full Verification, Packaging, README | Workspace verification (0 errors), actionlint tests passing, installer config (<85MB), updated architecture README | **DONE** |

---

## 🤝 Permitted Use & License

This project is the intellectual property of **Nguyen Anh Duong** and is protected under a **Personal & Internal Use License**.
- You are **PERMITTED** to download, study, compile, upgrade, and modify this project for personal and internal research purposes.
- You are **STRICTLY PROHIBITED** from redistributing original or modified source code, republishing, commercializing, selling, or offering LIVA as a hosted Software-as-a-Service (SaaS).

For complete legal terms, review [`LICENSE`](LICENSE) and [`CONTRIBUTING.md`](CONTRIBUTING.md).

---

## 🙏 Acknowledgments

LIVA is built on the shoulders of giants. Deep gratitude to the open-source communities and researchers:
- **llama.cpp** & **llama-cpp-2** for blazing-fast local GGUF inference on consumer hardware.
- **Tauri** & **Vue 3** for the memory-efficient native desktop shell and reactive UI.
- **SQLite** & **sqlite-vec** for embedded transactional storage and vector search.
- **NVIDIA (Nemotron)**, **rhasspy/Piper**, and **Silero** for local neural speech recognition, synthesis, and voice activity detection.
- **Google DeepMind**, **Qwen Team**, and **Meta AI** for state-of-the-art open weights models.
