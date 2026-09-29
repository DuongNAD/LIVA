---
title: "liva_architecture"
tags:
  - liva/knowledge
  - liva/architecture
  - liva/rust-native
author: "LIVA Core Architecture Team"
last_update: "2026-09-29T11:20:00+07:00"
---

# Knowledge: LIVA System Architecture

## Runtime Boundary

LIVA is built as a 100% Rust Native Multi-Crate Workspace. The Tauri v2 desktop shell connects in-process to the native Rust engine without intermediate loopback WebSockets or external sidecar processes. The legacy Node.js gateway (`liva-gateway`) and Python AI engine (`liva-ai-engine`) have been completely eliminated. The standalone `liva-voice/` folder remains strictly an offline utility for voice cloning training and dataset synthesis.

## Workspace Topology (7 Native Crates)

The Cargo workspace manages 7 member crates with strict dependency boundaries:

1. **`liva-desktop/src-tauri`**: Desktop shell built on Tauri v2. Owns window lifecycle, system tray, transparent overlay (`widget`), multi-monitor coordinate transforms (`ghost_mode`), IOTA Stronghold vault secrets, and native IPC command dispatching.
2. **`liva-native-core`**: Core system orchestrator. Implements `AppState`, domain command routing (`commands::*`), WebRTC audio pipeline, Sonora AEC3, GTCRN speech denoiser, Silero VAD, Smart Turn v3.2 active turn gate, vision manager, CUA integration, and in-memory knowledge graph (`CsrGraph`).
3. **`crates/liva-core-types`**: Pure, lightweight shared domain contracts, JSON schemas (`schemars`), turn taking verdicts (`TurnVerdict`, `AdaptiveTurnDecision`), and tool argument definitions across crates with zero heavy dependencies.
4. **`crates/liva-storage`**: SQLite WAL connection pool (`r2d2`), statically compiled C-FFI `sqlite-vec` (AVX2/FMA hardware-optimized vector search), in-memory Radix Trie (`FactTrie` / `ActiveRecallManager`), and the asynchronous `DbActor` single-writer transaction serializer (`BEGIN IMMEDIATE`).
5. **`crates/liva-llm`**: Multi-priority actor-based LLM engine (`LlmActorHandle`). Drains prioritized requests (`High`, `Normal`, `Low`), dispatches blocking inference to dedicated OS threads (`spawn_blocking`), and supports token streaming with real-time preemption cancellation via `AtomicBool`.
6. **`crates/liva-tools`**: Unified diagnostic CLI utility exposing subcommands `probe` (DB, ONNX, STT, TTS, Router, OS), `bench`, `doctor`, and `eval`.
7. **`crates/liva-cua`**: Native Computer-Use Agent (CUA) automation engine for Windows. Implements `Win32CuaDriver`, DPI-aware coordinate transformations, emergency hardware kill switch (`VK_ESCAPE` poller), UIPI privilege isolation, and SQLite action auditing.

Supporting frontend and tooling workspaces:
- **`liva-ui`**: Vue 3 + TypeScript desktop frontend (Three.js 3D avatar rendering via `OffscreenCanvas`).
- **`teamwork_projects/obsidian_llm_wiki`**: Local MCP server and Obsidian knowledge vault acting as the Single Source of Truth.

## Data and Control Flow

```
┌────────────────────────────────────────────────────────────────────────┐
│               liva-ui (Vue 3 / WebView2 / Three.js)                   │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ Tauri v2 IPC (Direct In-Process)
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│                       liva-desktop/src-tauri                           │
│   - Window Principal Authorization (widget / dashboard / setup)        │
│   - Event & Channel Streaming (tauri::ipc::Channel / window.emit)      │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ Direct Rust In-Process API
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│                         liva-native-core                               │
│   - Domain Routing (chat, voice, vision, cua, system)                  │
│   - Active Recall Interception (0 token cost FactTrie prefix search)   │
│   - Hybrid Retrieval (FTS5 + static sqlite-vec + ArcSwap<CsrGraph>)    │
│   - Audio Duplex Pipeline (AEC3 + GTCRN + Silero VAD + Smart Turn v3.2)│
└──────────────┬───────────────────┬───────────────────┬─────────────────┘
               │ MPSC              │ MPSC              │ C-FFI
               ▼                   ▼                   ▼
┌───────────────────────┐ ┌─────────────────┐ ┌──────────────────────────┐
│   crates/liva-llm     │ │crates/liva-storage│ │ crates/liva-cua        │
│ - LlmActor (Priority) │ │ - DbActor (WAL) │ │ - Win32CuaDriver         │
│ - spawn_blocking      │ │ - sqlite-vec (C)│ │ - Emergency Kill Switch  │
│ - Preemption Cancel   │ │ - Radix FactTrie│ │ - Windows Job Objects    │
└───────────────────────┘ └─────────────────┘ └──────────────────────────┘
```

1. **User Interaction**: User input originates from voice (`voice_mic_chunk`), hotkeys, or Vue 3 text inputs.
2. **In-Process IPC**: Commands flow across Tauri v2 IPC (`native_ipc_call` for unary calls, `native_ipc_call_stream` for chunked streaming, and `tauri::ipc::Channel` for real-time visemes and transcripts). Zero loopback TCP sockets or WebSocket servers are used.
3. **Authorization Gating**: `authorize_tauri_principal` verifies the calling window's label (`widget` -> `TauriWidget`, `dashboard` -> `TauriDashboard`, `setup` -> `TauriSetup`). Fail-closed enforcement rejects unauthorized command execution.
4. **Active Recall**: The conversational turn is intercepted by `ActiveRecallManager`. In-memory `FactTrie` searches for known facts in $<0.1\text{ ms}$; if a match is found, an active recall prompt is issued with 0 LLM token cost.
5. **Context Assembly**: `recall_context_scoped` executes two-stage hybrid search (static `sqlite-vec` embedding similarity + SQLite FTS5) and traverses `ArcSwap<CsrGraph>` with Personalized PageRank (SpMV in $<2\text{ ms}$). Dynamic context token budgeting strictly trims prompt segments to prevent context window overflow.
6. **LLM Inference**: The request is enqueued into `crates/liva-llm`'s `PriorityQueue`. Dedicated OS threads run inference without blocking Tokio async workers. If a higher-priority interruption occurs, `cancel_token` immediately preempts generation.

## Persistence & Static Vector Extension

- Database engine is SQLite with Write-Ahead Logging (`PRAGMA journal_mode = WAL;`) and `PRAGMA synchronous = NORMAL;`.
- Concurrency model: Single-writer actor (`DbActor`) on a dedicated OS thread micro-batches mutations using `BEGIN IMMEDIATE;`, completely eliminating `SQLITE_BUSY` errors. Multiple read connections (`r2d2_sqlite`) execute concurrent reads.
- **Static `sqlite-vec`**: The vector similarity search engine is statically compiled into the Rust binary from C source (`c/sqlite-vec.c`) via `cc` in `crates/liva-storage/build.rs` with AVX2/FMA optimizations. No dynamic runtime loading of `vec0.dll` or external C++ shared libraries occurs.

## Architectural Invariants

- **Security & Integrity First**: Principle of Least Privilege (`CommandPrincipal`), Windows Job Objects (`STARTUPINFOEXW` / `PROC_THREAD_ATTRIBUTE_JOB_LIST` with 512MB RAM cap and `KILL_ON_JOB_CLOSE`), and fail-closed security.
- **Memory Ceiling**: System RAM $\le 4.0\text{ GB}$ (typical runtime $\approx 2.3\text{ GB}$) and VRAM $\le 5.1\text{ GB}$ managed by `VisualGovernor`.
- **Zero Blocking on Tokio**: All heavy compute (LLM inference, ONNX embedding generation, SQLite disk writes) runs on dedicated background threads or through `spawn_blocking`.
- **No Loopback Ports**: Zero internal HTTP/WebSocket listening sockets; all inter-thread communication uses Rust channels and Tauri IPC channels.
- **Obsidian Single Source of Truth**: System state and rules documented here in `teamwork_projects/obsidian_llm_wiki/vault` govern runtime behavior.

## Related Notes

- [[Knowledge/memory_architecture|Memory Architecture]] — Three-tier memory engine, Radix Trie, SQLite WAL, and HippoRAG PPR graph.
- [[Knowledge/voice_pipeline|Voice Pipeline]] — WebRTC full-duplex pipeline, AEC3, GTCRN denoiser, and Smart Turn v3.2 active gate.
- [[Rules/tech_stack|Tech Stack]] — Multi-crate workspace dependencies and Tauri v2 configurations.
- [[Rules/coding_standards|Coding Standards]] — Rust development standards, concurrency patterns, and safety constraints.
- [[Rules/shutdown_chain|Shutdown Chain]] — Deterministic teardown sequence and sub-process cleanup.
- [[Knowledge/anti_patterns|Anti-Patterns]] — Prohibited design patterns in the native Rust architecture.
