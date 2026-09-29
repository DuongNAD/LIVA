---
title: "memory_architecture"
tags:
  - liva/knowledge
  - liva/memory
  - liva/rust-native
author: "LIVA Core Architecture Team"
last_update: "2026-09-29T11:20:00+07:00"
---

# Knowledge: Memory Architecture

## Executive Summary

LIVA's Unified Hybrid Memory (UHM) engine is implemented entirely in native Rust across `crates/liva-storage` and `liva-native-core`. It eliminates all legacy Node.js/V8/Zod constructs (`Math.exp` in V8, `EmbeddingWorker.ts`, `setImmediate`, `fs.promises.cp`), replacing them with high-throughput native primitives: an in-memory Radix Trie for active recall, a single-writer micro-batched SQLite WAL actor, statically compiled `sqlite-vec` (384-dimensional quantized embeddings), a lock-free double-buffered HippoRAG graph (`ArcSwap<CsrGraph>`), and an exact Context Token Budget allocator.

## Three-Tier Memory Subsystem

```
┌────────────────────────────────────────────────────────────────────────┐
│                      LIVA Multi-Tier Memory Engine                     │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 1: In-Memory Radix Trie (crates/liva-storage)                     │
│   - FactTrie & ActiveRecallManager                                     │
│   - Substring sliding-window matching in < 0.1 ms (0 token cost)       │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 2: Persistent SQLite WAL (crates/liva-storage & liva-native-core) │
│   - Bounded MPSC DbActor (1024 queue) on dedicated OS thread           │
│   - Micro-batched BEGIN IMMEDIATE transactions (zero SQLITE_BUSY)      │
│   - PRAGMA cache_size = -2000 (2 MB), mmap_size = 256 MB               │
│   - Tables: facts, events, agent_checkpoints, action_audit_ledger      │
├────────────────────────────────────────────────────────────────────────┤
│ Tier 3: Hybrid Vector & Knowledge Graph (crates/liva-storage)          │
│   - sqlite-vec: Static C FFI (vec_idx USING vec0(embedding int8[384])) │
│   - Decoupled EmbeddingEngine: multilingual-e5-small via ORT &self CPU │
│   - FTS5 vectors_fts table + vectors_meta table with Ebbinghaus decay  │
│   - GraphRAG: l3_nodes, l3_edges, lock-free ArcSwap<CsrGraph> in RAM   │
│   - HippoRAG: 3-round SpMV Personalized PageRank (PPR) in 1.12 ms      │
└────────────────────────────────────────────────────────────────────────┘
```

### Tier 1: In-Memory Radix Trie (Active Recall)

- **Source**: `crates/liva-storage/src/lib.rs` (`FactTrie`, `ActiveRecallManager`).
- **Data Structure**: Radix Trie rooted at `TrieNode { children: HashMap<char, TrieNode>, is_end_of_word: bool, fact_keys: Vec<String> }`.
- **Search Mechanics**:
  - Sliding-window prefix matching over user conversational turns.
  - Operates entirely in RAM with sub-millisecond latency ($<0.1\text{ ms}$).
  - Intercepts turns before LLM execution via `try_intercept_turn`. If a fact match is found, an active recall prompt is issued with **zero LLM token consumption**.
  - Prefix exploration (`search_prefix`) enables fast substring discovery and query autocompletion.

### Tier 2: Persistent SQLite WAL & Single-Writer DbActor

- **Source**: `crates/liva-storage` and `liva-native-core/src/db_actor.rs`.
- **Concurrency & Serialization**:
  - SQLite Write-Ahead Logging (`PRAGMA journal_mode = WAL;`) allows multiple concurrent read connections (`r2d2_sqlite`) without blocking.
  - Dedicated background OS thread runs the single-writer `DbActor`, bounded to a 1024-command MPSC queue (`tokio::sync::mpsc::channel`).
  - Mutations are micro-batched up to 25 writes within a 20 ms window using atomic `BEGIN IMMEDIATE; ... COMMIT;` blocks, completely eliminating `SQLITE_BUSY` errors.
- **Tuned PRAGMA Invariants (`crates/liva-storage/src/pragmas.rs`)**:
  - `PRAGMA busy_timeout = 5000;`
  - `PRAGMA cache_size = -2000;` (~2 MB memory footprint instead of default bloated buffers).
  - `PRAGMA page_size = 4096;`
  - `PRAGMA mmap_size = 268435456;` (256 MB memory-mapped I/O).
  - `PRAGMA temp_store = MEMORY;`
  - `PRAGMA synchronous = NORMAL;`
  - `PRAGMA journal_size_limit = 67108864;` (64 MB WAL boundary with autocheckpoint at 1000 pages).
- **Security & Privacy**:
  - Sensitive conversation contents in `vectors_meta` and agent states in `agent_checkpoints` are encrypted at rest with AES-GCM v2.
  - Privacy deletion audit records deterministic scope hashes and row counts without storing plaintext user prompts.

### Tier 3: Static sqlite-vec & HippoRAG In-Memory Graph

- **Static C-FFI Vector Engine**:
  - Compiled directly from `c/sqlite-vec.c` via `cc` in `crates/liva-storage/build.rs` with `/O2`, `/fp:fast`, `/arch:AVX2` on MSVC x64.
  - Statically linked through `sqlite3_vec_init` C FFI; no dynamic `vec0.dll` dependency.
  - Table: `vec_idx USING vec0(embedding int8[384])` holding 384-dimensional quantized embeddings.
- **Lock-Free Embedding Engine**:
  - `Arc<EmbeddingEngine>` wraps the ONNX Runtime session (`ort`) for `multilingual-e5-small`.
  - Uses `&self` inference methods, completely eliminating global mutex lock contention during embedding generation.
- **Lock-Free HippoRAG Graph (`ArcSwap<CsrGraph>`)**:
  - Compressed Sparse Row graph (`CsrGraph`) stored in `liva-native-core/src/db/csr_graph.rs` and `src/agent/graph.rs`.
  - Read path accesses `ArcSwap<CsrGraph>.load()` with zero lock contention.
  - Mutation path is serialized via `DbActor`, writing to `l3_nodes` and `l3_edges` in SQLite and updating the in-memory snapshot atomically.
  - Personalized PageRank (PPR) uses a 3-round Sparse Matrix-Vector (SpMV) power iteration, resolving multi-hop associative queries across nodes in $\approx 1.12\text{ ms}$.

## Context Token Budget Allocator

- **Source**: `liva-native-core/src/llm/prompt/dynamic_prompt.rs` (`ContextTokenBudget`).
- **Priority Gating**:
  - `P0 (System + Current User Prompt + Suffix)`: Guaranteed non-evictable invariant.
  - `P1 (Active Recall / Direct Memory Facts)`: Budgeted facts retrieved via `FactTrie`.
  - `P2 (Hybrid RAG & HippoRAG PPR Candidates)`: Scored via Reciprocal Rank Fusion (RRF) with dynamic Ebbinghaus decay.
  - `P3 (Historical Conversation Turns)`: Evicted in reverse chronological order when approaching context limits.
- **Exact Measurement**:
  - Replaces heuristic character multipliers with exact compile-measure-evict loops.
  - Guarantees zero context window overflow while maximizing KV cache reuse across turns.

## Native Rust Memory Invariants

| Invariant | Implementation | Architectural Guarantee |
|---|---|---|
| **Lock-Free Reads** | `ArcSwap<CsrGraph>.load()` | Zero lock contention on RAG search threads |
| **Single-Writer WAL** | `DbActor` MPSC + `BEGIN IMMEDIATE;` | Zero `SQLITE_BUSY`, atomic batch commits |
| **Static C Vector** | `cc::Build` of `sqlite-vec.c` | No runtime DLL loading, AVX2 SIMD acceleration |
| **Sub-0.1ms Recall** | `FactTrie` sliding-window in RAM | Zero LLM token consumption on known facts |
| **Bounded Channels** | `tokio::sync::mpsc(1024)` | Strict backpressure, prevents unbounded RAM growth |
| **Safe Backup** | `VACUUM INTO` and WAL truncation | Guarantees zero database corruption during backups |
| **Encrypted Rest** | AES-GCM v2 in `vectors_meta` | Encryption at rest, key separation |

## Related Notes

- [[Knowledge/liva_architecture|LIVA Architecture]] — Overall system topology and Tauri v2 IPC.
- [[Rules/coding_standards|Coding Standards]] — Rust development standards and concurrency invariants.
- [[Rules/tech_stack|Tech Stack]] — Multi-crate dependencies and build profiles.
- [[Rules/shutdown_chain|Shutdown Chain]] — Deterministic teardown and MPSC queue flushing.
- [[Knowledge/anti_patterns|Anti-Patterns]] — Prohibited memory patterns in the native Rust architecture.
