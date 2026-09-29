---
title: "tech_stack"
tags:
  - liva/rule
  - liva/tech-stack
  - liva/rust-native
author: "LIVA Core Architecture Team"
last_update: "2026-09-29T11:20:00+07:00"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Tech Stack

## Rule Statement

LIVA operates exclusively on a 100% Rust Native Multi-Crate Workspace. The Tauri v2 desktop shell embeds and invokes the native engine directly in-process through Tauri IPC Commands and Channels. All legacy loopback WebSocket servers (`tokio-tungstenite`, `axum`), legacy gateway adapters, and Node.js/Python server runtimes are completely eliminated and strictly prohibited.

## The 7-Crate Workspace Topology

| Crate Path | Role & Technology |
|---|---|
| **`liva-desktop/src-tauri`** | Tauri v2 desktop shell, window lifecycle, transparent overlay (`widget`), multi-monitor coordinate math, IOTA Stronghold secure vault, and native IPC dispatching. |
| **`liva-native-core`** | Central system orchestrator, domain command routing, WebRTC full-duplex audio pipeline (Sonora AEC3, GTCRN, Silero VAD, Smart Turn v3.2), `AppState`, and `ArcSwap<CsrGraph>`. |
| **`crates/liva-core-types`** | Shared domain contracts, `schemars` JSON schemas, `TurnVerdict`, `AdaptiveTurnDecision`, and tool argument types with zero heavy dependencies. |
| **`crates/liva-storage`** | SQLite WAL connection pool (`r2d2`), single-writer `DbActor` (`BEGIN IMMEDIATE` micro-batching), in-memory Radix Trie (`FactTrie`), and static C-FFI `sqlite-vec`. |
| **`crates/liva-llm`** | Actor-based LLM engine (`LlmActorHandle`), 3-tier priority queue (`High`, `Normal`, `Low`), `spawn_blocking` inference threads, and `AtomicBool` streaming preemption cancellation. |
| **`crates/liva-tools`** | Unified diagnostic CLI tool binary providing `probe` (DB, ONNX, STT, TTS, Router, OS), `bench`, `doctor`, and `eval` command suites. |
| **`crates/liva-cua`** | Windows Computer-Use Agent (CUA) automation engine: `Win32CuaDriver`, DPI-aware coordinate mapping, emergency kill switch (`VK_ESCAPE`), and action audit logging. |

## Technology Matrix

| Layer | Approved Technology Stack |
|---|---|
| **Language & Concurrency** | Rust 2021/2024, Tokio multi-threaded runtime, bounded MPSC channels |
| **Desktop IPC** | Tauri v2 in-process IPC (`native_ipc_call`, `native_ipc_call_stream`, `tauri::ipc::Channel`). Zero TCP loopback ports. |
| **Persistence & Database** | `rusqlite` + `r2d2`, SQLite WAL mode, FTS5 full-text indexing, single-writer `DbActor` |
| **Vector Search Engine** | Static C-FFI `sqlite-vec` compiled via `cc` in `crates/liva-storage/build.rs` with AVX2 SIMD optimizations (no `vec0.dll`) |
| **Graph & Retrieval** | In-memory `ArcSwap<CsrGraph>`, HippoRAG 3-round SpMV Personalized PageRank ($1.12\text{ ms}$) |
| **Local ML & Inference** | `llama-cpp-2` for GGUF execution; ONNX Runtime (`ort`) on CPU for embeddings (`multilingual-e5-small`), VAD (`Silero VAD v5`), speech denoising (`GTCRN`), and turn taking (`Smart Turn v3.2`) |
| **Audio Duplex** | WebRTC AudioWorklet capture (16 kHz / 32 ms hop), Sonora AEC3, native TTS (`tts::kokoro`, `tts::vieneu`, `piper`) |
| **Desktop UI** | Vue 3, TypeScript, Vite, Three.js 3D avatar rendering via Web Worker `OffscreenCanvas` |
| **Knowledge Base** | Local Obsidian Vault (`teamwork_projects/obsidian_llm_wiki`) with local TypeScript MCP server |
| **Offline Tooling** | `liva-voice/` Python utility strictly for offline custom voice cloning dataset training |

## Architectural Invariants & Bans

- ❌ **No Loopback WebSockets**: Do not reintroduce `tokio-tungstenite`, `axum`, or internal TCP loopback servers. All desktop-to-core streaming must use `tauri::ipc::Channel` or `window.emit`.
- ❌ **No External Vector DLLs**: Vector search must use statically compiled `sqlite-vec` via `crates/liva-storage/build.rs`. Never attempt dynamic loading of `vec0.dll`.
- ❌ **No Node.js/Python Backend Servers**: All backend orchestration, persistence, and AI routing must reside inside the compiled Rust workspace.
- ❌ **No Live WAL Copying**: Never use filesystem copies (`std::fs::copy`) on active SQLite files. Use `PRAGMA wal_checkpoint(TRUNCATE);` followed by `VACUUM INTO`.
- ❌ **No Unbounded MPSC Channels**: Channel allocations must specify explicit buffer limits (e.g. 1024) to maintain strict hardware ceilings ($\le 4.0\text{ GB}$ RAM).
- ❌ **No Direct Command Spawning**: Child processes must be attached to an atomic Windows NT Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).

## Verification Commands

- **Workspace Check**: `cargo check --workspace -j 2`
- **Static Vector Test**: `cargo test -p liva-storage --test sqlite_vec_static_tests -j 2 -- --test-threads 2`
- **LLM Actor Preemption Test**: `cargo test -p liva-llm -j 2 -- --test-threads 2`
- **Full Workspace Test Suite**: `cargo test --workspace -j 2 -- --test-threads 2`
- **Vault Validation**: `npm run validate -w obsidian-llm-wiki`

## Related Notes

- [[Knowledge/liva_architecture|LIVA Architecture]] — Overall system topology and Tauri v2 IPC.
- [[Knowledge/memory_architecture|Memory Architecture]] — Three-tier memory engine, Radix Trie, and SQLite WAL.
- [[Knowledge/voice_pipeline|Voice Pipeline]] — WebRTC full-duplex pipeline, AEC3, GTCRN denoiser.
- [[Rules/coding_standards|Coding Standards]] — Rust development standards and concurrency invariants.
- [[Rules/shutdown_chain|Shutdown Chain]] — Deterministic teardown sequence and sub-process cleanup.
- [[Knowledge/anti_patterns|Anti-Patterns]] — Prohibited design patterns in the native Rust architecture.
