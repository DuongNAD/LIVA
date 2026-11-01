# Project: LIVA Production-Ready System Upgrade

## Architecture Overview
LIVA (**Local Intelligent Virtual Assistant**) is a privacy-first, desktop-native personal AI assistant operating on Windows 10/11 x64. The architecture is centered around a unified high-performance Rust core (`liva-native-core`), connected via native in-process IPC (`tauri::ipc::Channel`) to a Tauri v2 desktop shell (`liva-desktop`) and a Vue 3 / Three.js user interface (`liva-ui`).

### Core Architectural Pillars
1. **Pillar 1: Multi-Agent Orchestration & Planning**: Asynchronous DAG planning (`petgraph`), hierarchical complexity routing, Reflexion self-healing, and execution integrity.
2. **Pillar 2: Local Memory & Hybrid Retrieval**: Hierarchical memory (Working Core, Recall SQLite, Archival Vector), Mem0 bitemporal reconciliation, lock-free HippoRAG CSR matrix, dynamic Ebbinghaus decay, and two-stage hybrid search (`sqlite-vec` + FTS5 Vietnamese normalization + Cross-Encoder).
3. **Pillar 3: Multimodal & Real-time Duplex**: Edge-optimized visual GUI grounding with SIMD diffing, two-stage adaptive voice turn-taking (`Smart Turn v3.2` active gate < 225ms), Sonora AEC3 + GTCRN + Silero VAD + VieNeu/Piper TTS, and OffscreenCanvas Three-VRM avatar kinematics.
4. **Pillar 4: Sandboxed System Automation & Security**: Dual-layer process containment (Windows Job Objects with memory caps; fail-closed capability attenuation with BLAKE3 HMAC tokens), prompt delimiter injection sanitization, and tiered authorization with two-phase human-in-the-loop confirmation.

---

## Feature Inventory
Every capability, hardening requirement, and bug fix from the Survey phase is indexed below with its assigned milestone.

| # | Feature Key | Description | Milestone | Source |
|---|---|---|---|---|
| F01 | `desktop-ts-build-fix` | Fix TS6133 unused `toggleSkill` in `SkillsView.vue:326` to unblock `vue-tsc -b` and packaging | M1 | Desktop Survey |
| F02 | `widget-text-ipc-routing` | Remap widget message input from rejected `user_voice_command` to authorized `chat:completion` | M1 | Desktop Survey |
| F03 | `widget-interrupt-ipc` | Remap widget voice stop button from failing generic IPC to dedicated `voice_interrupt` Tauri command | M1 | Desktop Survey |
| F04 | `script-test-path-sync` | Fix documentation path drifts in maintenance tests (`docs-citations`, `check-installer-config`, `wake-ux-copy`) | M1 | Test Survey |
| F05 | `desktop-packaging-verify` | Verify `npm test -w liva-ui`, `npm run build:ui`, `npm run check:installer`, `npm run test:installer` 100% pass | M1 | Desktop/Test Survey |
| F06 | `jobobject-security-hardening`| Harden `WindowsProcessGuard` in `sidecar.rs` with `CREATE_SUSPENDED` and memory caps | M2 | Core Survey |
| F07 | `prompt-delimiter-sanitizer` | Sanitize ChatML/Gemma control tokens in prompt compilation to prevent delimiter injection | M2 | Core Survey |
| F08 | `agent-stub-cleanup` | Eliminate mock stub in `agent/dispatcher.rs` and cleanly integrate production pipeline | M2 | Core Survey |
| F09 | `session-actor-boxing` | Box `SessionTurnRequest` in `SessionMessage::ExecuteTurn` to reduce enum variant footprint and channel churn | M2 | Core/Test Survey |
| F10 | `abandoned-crate-cleanup` | Remove abandoned empty directory `crates/liva-voice/` and clean unlinked artifacts | M2 | Core Survey |
| F11 | `core-types-clippy-polish` | Fix `clippy::derivable_impls` and add `# Safety` doc comment to `unwrap_unchecked` in `liva-core-types` | M3 | Test Survey |
| F12 | `native-core-clippy-polish`| Fix `clippy::new_without_default`, `too_many_arguments`, and `needless_bool` across `liva-native-core` | M3 | Test Survey |
| F13 | `storage-deprecation-cleanup`| Update deprecated `ActiveRecallManager` imports in `crates/liva-storage/tests` to use canonical path | M3 | Core/Test Survey |
| F14 | `zero-warning-clippy-gate` | Ensure `cargo clippy -j 2` exits with 0 warnings across the entire workspace | M3 | System Requirement R3 |
| F15 | `doc-architecture-sync` | Synchronize `docs/01-kien-truc/`, `docs/02-crates/`, and `docs/03-phat-trien-van-hanh/` with production code | M4 | System Requirement R4 |
| F16 | `obsidian-vault-sync` | Update Obsidian Vault (`teamwork_projects/obsidian_llm_wiki/vault/`) architecture blueprints and testing guide | M4 | System Requirement R4 |
| F17 | `tech-debt-ledger-sync` | Update `tech-debt-ledger.json` and historical logs reflecting resolved debt items | M4 | System Requirement R4 |
| F18 | `full-workspace-check` | Comprehensive verification: `cargo check -j 2` and `cargo test -j 2 -- --test-threads 2` across all 7 crates | M5 | Acceptance Criteria |
| F19 | `e2e-quality-assurance` | Independent QA and empirical challenge across desktop shell, IPC, memory, voice, and sandbox | M5 | Acceptance Criteria |
| F20 | `forensic-integrity-audit` | Independent Forensic Auditor verification enforcing zero-cheating benchmark integrity | M5 | Acceptance Criteria |

---

## Milestones

| # | Name | Scope & Deliverable Focus | Dependencies | Status |
|---|---|---|---|---|
| M1 | Desktop Shell, UI & IPC Production Readiness | Fix build blockers (`SkillsView.vue`), fix Widget IPC routing (`chat:completion`, `voice_interrupt`), fix script tests, unblock desktop packaging (F01–F05) | Survey complete | DONE |
| M2 | Rust Native Core & Security Hardening | Harden Windows JobObject, sanitize prompt delimiters, eliminate agent stubs, box session actor enum, clean abandoned directories (F06–F10) | Survey complete | DONE |
| M3 | Code Quality, Clippy Linters & Tech Debt | Fix Clippy warnings in `liva-core-types` and `liva-native-core`, fix storage test deprecation warnings, achieve 0 warnings (F11–F14) | M1, M2 | DONE |
| M4 | Documentation, Architecture & Vault Sync | Synchronize technical documentation in `docs/` and Obsidian Vault, update tech debt ledger and testing guidelines (F15–F17) | M1, M2, M3 | DONE |
| M5 | Final Acceptance Verification & Victory Gate | Comprehensive workspace check, test execution across all crates, independent review, empirical challenge, and forensic audit (F18–F20) | M1, M2, M3, M4 | DONE |

---

## Interface Contracts

### 1. Desktop UI (Widget) ↔ Tauri Native IPC Channel
- **Contract**:
  - Text Prompt Input: Must invoke `chat:completion` with payload `{ text: String, session_id: String }` instead of unmapped `user_voice_command`.
  - Voice Interrupt: Must invoke `voice_interrupt` with payload `{ epoch_id: u32 }` instead of generic `native_ipc_call('interrupt')`.
- **Security**: Must pass `authorize_tauri_principal` under `CommandPrincipal::TauriWidget`.

### 2. LLM Engine ↔ Process Sandbox Boundary
- **Contract**:
  - `WindowsProcessGuard`: Must spawn sub-processes with `CREATE_SUSPENDED`, assign to Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and `JobMemoryLimit = 512MB`, before resuming main thread.
  - Prompt Template Compiler: Must escape or strip delimiter tokens (`<|im_start|>`, `<|im_end|>`, `<start_of_turn>`, `<end_of_turn>`) from user-supplied message strings before interpolating into prompt strings.

### 3. Session Actor Channel Contract
- **Contract**:
  - Channel: `tokio::sync::mpsc::channel<SessionMessage>(32)`
  - Message: `SessionMessage::ExecuteTurn(Box<SessionTurnRequest>)` ensuring size of variant $\le 32$ bytes.

---

## Code Layout
- `liva-native-core/`: Core Rust engine (daemon, pipeline, db, webrtc, stt, tts, cua, authorization)
  - `src/agent/`: Swarm DAG orchestrator, state machine, reflection, session actor
  - `src/db/`: Micro-batched SQLite WAL connection pool, `CsrGraph` HippoRAG, hybrid search
  - `src/webrtc/`: Sonora AEC3, GTCRN denoiser, Silero VAD, Smart Turn v3.2
  - `src/llm/`: Llama/sidecar process runner, tool insurance, prompt compilation
- `crates/liva-core-types/`: Core domain types, capabilities, error definitions
- `crates/liva-storage/`: Unified transactional storage, schema migrations, and embedded vector virtual tables
- `crates/liva-cua/`: Computer-Use Agent Win32 driver, SIMD frame diffing, kill-switch
- `crates/liva-tools/`: Operating system tools, web tools, and sandboxed runner
- `crates/liva-llm/`: LLM actor and prompt engineering utilities
- `liva-desktop/src-tauri/`: Tauri v2 desktop application, window management, native IPC channels
- `liva-ui/`: Vue 3 + Three.js frontend, OffscreenCanvas 3D avatar worker, audio capture worklet
- `docs/`: Master architectural specifications, benchmarking reports, and implementation roadmaps
- `teamwork_projects/obsidian_llm_wiki/vault/`: Obsidian knowledge vault integration and guidelines
