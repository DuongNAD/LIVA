# Project: LIVA Intelligent Assistant Ecosystem Upgrade & Optimization

## Architecture
- **Native Backend (`liva-native-core`)**: Pure Rust engine running in-process via Tauri v2. Houses SQLite WAL bifurcated pool (1-writer/4-readers), llama.cpp inference, ONNX multilingual-e5-small embeddings (384-dim), sqlite-vec (`vec0`), FTS5 hybrid search with RRF ($K=60.0$), Swarm DAG StateGraph orchestration, and AES-256-GCM encrypted transcript storage.
- **Desktop UI (`liva-ui` & `liva-desktop`)**: Vue 3.5 + TypeScript + Vite 8 + UnoCSS + Tauri v2. Dual-window architecture (`widget.html` overlay + `dashboard.html` control center). Native IPC streaming bridge (`useGateway.ts`), 150ms hit-testing zone sync, BI analytics visualizer, Obsidian PKM explorer, global toast system, and skeleton loaders.
- **Security & Compliance**: Windows DPAPI sealed master keys, `CommandPrincipal` RBAC, `SecretScrubber` audit log redaction, `PRAGMA secure_delete = ON` cryptographic wipe, and Vietnamese Decree 13 / GDPR compliance.

## Feature Inventory
| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| F1 | SQLite WAL Pool Optimization | Bifurcated 1-writer/4-reader pool, async helpers, lock contention prevention | M1 | R1 / Backend Survey |
| F2 | IPC Streaming Optimization | Elimination of double JSON serialization, pre-allocated event strings, <5ms latency | M1 | R1 / Backend Survey |
| F3 | Build Profile & Memory Tuning | Root Cargo.toml profile optimizations for argon2/dev/test, memory allocation tuning | M1 | R1 / Backend Survey |
| F4 | Global Toast Notification System | Unified `useToast()` composable and `<ToastContainer />` across all dashboard views | M2 | R2 / Frontend Survey |
| F5 | Standardized Skeleton Loaders | Shimmer `<SkeletonLoader />` components for async memory, tasks, and system views | M2 | R2 / Frontend Survey |
| F6 | Native Tauri Dialog Integration | Replace legacy Electron path properties with `@tauri-apps/plugin-dialog` in `AISettings.vue` | M2 | R2 / Frontend Survey |
| F7 | BI Analytics Visualizer View | Interactive dashboard view for business metrics, SQL query execution, and chart rendering | M2 | R2 / Frontend Survey |
| F8 | Obsidian PKM Vault Explorer | Interactive knowledge note explorer, frontmatter viewer, and vault search interface | M2 | R2 / Frontend Survey |
| F9 | Frontend Code Cleanup | Remove 0-byte `VisionSensor.vue`, clean Vitest exclusions, refine streaming cursor | M2 | R2 / Frontend Survey |
| F10 | AI Router & Context Guardrails | Dynamic model family detection, hard token bounds (`check_prompt_fits`), KV cache pruner | M3 | R3 / AISec Survey |
| F11 | Hybrid RAG & Vector Search | Decoupled ONNX embedder (384-dim), sqlite-vec, FTS5 unicode61, RRF fusion ($K=60.0$) | M3 | R3 / AISec Survey |
| F12 | Swarm DAG StateGraph Engine | Async StateGraph pipeline, multi-agent dispatcher, self-correction loop, scoped tools | M3 | R3 / AISec Survey |
| F13 | Security Redaction & Cryptography | DPAPI keystore, AES-256-GCM transcript encryption, `SecretScrubber` redaction | M4 | R4 / AISec Survey |
| F14 | Compliance & Right-to-be-Forgotten | Atomic deletion with `PRAGMA secure_delete = ON`, Vietnamese Decree 13 / GDPR compliance | M4 | R4 / AISec Survey |
| F15 | Tech-Debt & Skill Governance | 100/100 health score verification in `tech-debt-ledger.json`, 42 skills / 53 vault notes audit | M4 | R4 / AISec Survey |
| F16 | E2E Opaque-Box & Adversarial Test Suite | Comprehensive Tiers 1-4 test verification + Tier 5 adversarial stress testing | M5 | Acceptance Criteria |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Native Core Optimization & Performance | SQLite WAL pool, IPC stream optimization, Cargo profile tuning | none | DONE (625/625 tests passed) |
| M2 | Desktop UI & Realtime User Experience | Global toasts, skeleton loaders, native dialogs, BI & PKM views, cleanup | M1 | DONE (444/444 tests passed) |
| M3 | AI Router, Multi-Agent Swarm & Vector RAG | Dynamic template detection, ONNX embeddings, RRF RAG, Swarm DAG engine | M1 | DONE (625/625 tests passed) |
| M4 | Comprehensive Security & Quality Hardening | SecretScrubber, DPAPI keystore, atomic deletion, tech-debt ledger verification | M1, M3 | DONE (100/100 score, audit clean) |
| M5 | E2E Test Suite & Multi-Agent Verification | Pass 100% E2E tests (Tiers 1-4), adversarial Tier 5 hardening, Forensic Audit | M1, M2, M3, M4 | **NOT ACCEPTED** — status reverted 16/08/2026. The "177/177 passed" figure came from `scripts/e2e-test-suite.mjs`, which asserts against JavaScript reimplementations in `scripts/e2e/helpers.mjs` and never reaches the Rust core, the UI, the gateway socket, or an on-disk DB. Retraction and evidence: [`TEST_READY.md`](TEST_READY.md). Real E2E signal today: `e2e-gateway-ci.mjs` 8/8, `e2e-memory.mjs` 6/6. |

## Interface Contracts

### IPC Bridge Contract: `liva-desktop/src-tauri` ↔ `liva-ui`
- `native_ipc_call(command: String, payload: Value) -> Result<Value, String>`
- `native_ipc_call_stream(command: String, payload: Value, req_id: String) -> Result<Receiver<IpcResponse>, String>`
- Stream chunks emitted over Tauri window event `ipc-stream:{req_id}` with payloads:
  - `{"type": "chunk", "text": string}`
  - `{"type": "done", "total_tokens": number}`
  - `{"type": "error", "message": string}`

### Storage & Cryptography Contract: `liva-native-core::db`
- `DatabasePool`: Bifurcated into `writer: Arc<r2d2::Pool<SqliteConnectionManager>>` (max 1) and `readers: Arc<r2d2::Pool<SqliteConnectionManager>>` (max 4).
- PRAGMAs: `journal_mode = WAL`, `synchronous = NORMAL`, `busy_timeout = 5000`, `cache_size = -8192`, `foreign_keys = ON`.
- AES-256-GCM key derived from Windows DPAPI `.device_key` via HKDF-SHA256.

### AI Engine & Swarm Contract: `liva-native-core::agent`
- `StateGraph::invoke(initial_state: AgentState) -> Result<AgentState, AgentError>`
- Scoped tools enforce `ExecPolicy::Auto` for read-only actions and `ExecPolicy::ProposeOnly` for side-effect operations.

## Code Layout
- `liva-native-core/`: Rust native engine (LLM inference, ONNX embedder, SQLite WAL, Swarm DAG, Security).
- `liva-desktop/src-tauri/`: Tauri v2 desktop application wrapper, window management, native IPC gateway.
- `liva-ui/`: Vue 3.5 + TypeScript frontend, widget overlay, dashboard control center, components, composables.
- `packages/liva-common/`: Shared TypeScript data models, interfaces, and protocol definitions.
- `teamwork_projects/obsidian_llm_wiki/`: Obsidian PKM knowledge vault and wiki server.
- `.agents/`: Agent orchestration state, plans, handoffs, and verification logs.
