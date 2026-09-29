---
title: "anti_patterns"
tags:
  - liva/knowledge
  - liva/anti-patterns
  - liva/rust-native
author: "LIVA Core Architecture Team"
last_update: "2026-09-29T11:20:00+07:00"
---

# Knowledge: Anti-Patterns

## Executive Summary

This document serves as the authoritative catalogue of prohibited patterns, concurrency hazards, and architectural anti-patterns in LIVA's 100% Rust Native Multi-Crate Architecture. All legacy Node.js/V8/Zod constraints (`Sidecar WS`, `build-sea.js`, `isolated-vm`, `ASTWorker.ts`, `node:sqlite`) have been purged. Violating these native Rust invariants will trigger build failures, async deadlocks, memory leaks, or crashes.

## 1. Rust Concurrency & Async Hazards

### ❌ Holding MutexGuard Across `.await`
- **Violation**: Holding `std::sync::MutexGuard` or `parking_lot::MutexGuard` across a Tokio `.await` boundary.
- **Hazard**: Causes thread pool starvation, executor deadlocks, or compiler `!Send` errors. Tokio worker threads cannot yield while holding synchronous locks.
- **Resolution**: Minimize lock scope using a local block `{ let val = guard.get(); }` before `.await`, or use `tokio::sync::Mutex` only when long-lived state must be held across await points.

### ❌ Unbounded MPSC Channels
- **Violation**: Allocating `tokio::sync::mpsc::unbounded_channel()` for IPC messages, token streaming, or DB writes.
- **Hazard**: Fast producers (e.g., streaming LLM tokens or high-frequency audio frames) flood the channel faster than consumers can process, violating the strict RAM ceiling ($\le 4.0\text{ GB}$).
- **Resolution**: Always use bounded channels with explicit backpressure, e.g. `tokio::sync::mpsc::channel(1024)`.

### ❌ Direct `Command::spawn` without Windows Job Object
- **Violation**: Spawning sub-processes or external CLI tools via `std::process::Command::spawn()` directly without OS sandbox containment.
- **Hazard**: If LIVA crashes or is terminated, child processes remain orphaned, leaking GPU VRAM, ports, and file locks.
- **Resolution**: Attach all child processes to an atomic Windows NT Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) via `STARTUPINFOEXW` / `PROC_THREAD_ATTRIBUTE_JOB_LIST` so child processes terminate automatically with the host.

### ❌ `unwrap()` / `expect()` on IPC, SQLite, or Network Paths
- **Violation**: Invoking `.unwrap()` or `.expect()` in Tauri command handlers, DB queries, or external tool execution paths.
- **Hazard**: Unhandled panics abort the entire native engine or poison shared synchronization locks (`RwLock` / `Mutex`).
- **Resolution**: Use `anyhow::Result`, `thiserror`, or return structured errors with `?`. Recover gracefully from poisoned locks via `.unwrap_or_else(|e| e.into_inner())`.

### ❌ Blocking Tokio Async Workers with Heavy Compute
- **Violation**: Executing synchronous disk I/O, heavy ONNX model inference, or LLM token generation directly inside `tokio::task`.
- **Hazard**: Starves the Tokio async reactor, introducing jitter and lag to UI streaming and audio duplex loops.
- **Resolution**: Offload all blocking compute to `tokio::task::spawn_blocking` or dedicated OS worker threads (e.g., `DbActor` and `LlmActor`).

## 2. Memory & Persistence Hazards

### ❌ Bypassing the DbActor Single-Writer
- **Violation**: Executing ad-hoc SQL mutations (`INSERT`, `UPDATE`, `DELETE`) directly on reader connections or without routing through `DbActor`.
- **Hazard**: Triggers `SQLITE_BUSY` contention on SQLite WAL and creates desynchronization with the in-memory `ArcSwap<CsrGraph>`.
- **Resolution**: Route all write mutations through `state.db.writer_actor`, which batches writes using atomic `BEGIN IMMEDIATE; ... COMMIT;` transactions.

### ❌ Global Mutex on EmbeddingEngine
- **Violation**: Wrapping `EmbeddingEngine` in a `tokio::sync::Mutex` or `parking_lot::Mutex`.
- **Hazard**: Chokes concurrent RAG and active recall operations behind a single global lock.
- **Resolution**: ONNX Runtime sessions (`ort`) support thread-safe `&self` inference. Wrap the engine in `Arc<EmbeddingEngine>` and invoke methods concurrently without locking.

### ❌ Direct SQLite File Copying for Backup
- **Violation**: Copying live SQLite database files via `std::fs::copy` while connections are open.
- **Hazard**: Results in corrupt database images due to uncommitted WAL pages and incomplete frame headers.
- **Resolution**: Always use `PRAGMA wal_checkpoint(TRUNCATE);` followed by `VACUUM INTO 'backup.sqlite';`.

### ❌ Raw UTF-8 Byte Slicing
- **Violation**: Truncating text using raw byte indices: `&text[..max_len]`.
- **Hazard**: Splitting a multi-byte UTF-8 sequence (common in Vietnamese diacritics) causes an immediate thread panic.
- **Resolution**: Always slice using character boundaries: `text.chars().take(max_len).collect::<String>()` or `floor_char_boundary`.

## 3. Desktop IPC & Voice Pipeline Hazards

### ❌ Loopback WebSocket Re-introduction
- **Violation**: Opening TCP loopback servers (e.g., `axum` or `tokio-tungstenite`) to bridge frontend and backend.
- **Hazard**: Creates security exposure (port hijacking, rogue web page access) and violates zero-loopback invariants.
- **Resolution**: Use direct, in-process Tauri v2 IPC (`native_ipc_call`, `native_ipc_call_stream`, and `tauri::ipc::Channel`).

### ❌ Streaming Raw Tokens Directly to TTS
- **Violation**: Forwarding individual token chunks or emojis directly from the LLM stream to the audio synthesizer.
- **Hazard**: Produces unnatural robotic cadence, audio crackling, and extreme synthesis overhead.
- **Resolution**: Buffer tokens using `TTSFormatter` into complete Vietnamese semantic clauses (split on punctuation and conjunctions) before synthesis.

### ❌ Per-Token Reactive DOM Thrashing
- **Violation**: Updating deep reactive Vue stores or triggering layout reflows on every individual streamed token.
- **Hazard**: Degrades UI frame rates down to 20-30 FPS and stutters 3D avatar animations.
- **Resolution**: Use `shallowRef` and batch DOM updates into requestAnimationFrame boundaries in `liva-ui`.

## 4. Hardware Budget & VRAM Hazards

### ❌ Concurrent VLM and LLM Execution
- **Violation**: Loading vision language models (VLM, 750 MB) while the primary LLM is running inference without leasing.
- **Hazard**: Breaches the strict VRAM ceiling ($\le 5.1\text{ GB}$), crashing display drivers or inducing GPU OOM.
- **Resolution**: Enforce mutual exclusion through `VisualGovernor`, ensuring VLM is unloaded or kept dormant during conversational turns.

## Related Notes

- [[Knowledge/liva_architecture|LIVA Architecture]] — Overall system topology and Tauri v2 IPC.
- [[Knowledge/memory_architecture|Memory Architecture]] — Three-tier memory engine, Radix Trie, and SQLite WAL.
- [[Knowledge/voice_pipeline|Voice Pipeline]] — WebRTC full-duplex pipeline, AEC3, GTCRN denoiser.
- [[Rules/coding_standards|Coding Standards]] — Rust development standards and concurrency invariants.
- [[Rules/tech_stack|Tech Stack]] — Multi-crate dependencies and build profiles.
- [[Rules/shutdown_chain|Shutdown Chain]] — Deterministic teardown and MPSC queue flushing.
