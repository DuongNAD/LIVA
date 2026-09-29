---
title: "shutdown_chain"
tags:
  - liva/rule
  - liva/architecture
  - liva/rust-native
author: "LIVA Core Architecture Team"
last_update: "2026-09-29T11:20:00+07:00"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Shutdown Chain

## Rule Statement

The system teardown MUST execute deterministically and sequentially through native Rust graceful shutdown (`AppState::shutdown` and Tauri lifecycle management). The shutdown sequence must signal Tokio `CancellationToken`s, cancel in-flight LLM generation, drain the `DbActor` MPSC queue, execute a safe SQLite WAL checkpoint, and terminate all child sub-processes via Windows NT Job Objects (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`). No hardcoded sleeps (`thread::sleep` / `tokio::time::sleep`) or abrupt uncoordinated process aborts (`std::process::exit`) are permitted.

## Rationale

- **GPU & VRAM Protection**: Local LLM and ONNX runtime contexts must release GPU buffers and device handles cleanly to prevent device memory fragmentation or VRAM locks across sessions.
- **Zero WAL Corruption**: SQLite WAL transactions must be micro-batched and committed before connection pools drop. Closing the database while the `DbActor` MPSC queue contains pending mutations leads to lost writes or unfinished WAL rollbacks.
- **Zombie Process Elimination**: External tool executions or side utilities must be attached to an atomic Windows NT Job Object at creation (`StartupInfoExW` with `PROC_THREAD_ATTRIBUTE_JOB_LIST`). If the host process exits unexpectedly, the Windows kernel terminates all descendants atomically.

## The Native Rust Shutdown Sequence

```
┌────────────────────────────────────────────────────────────────────────┐
│                      LIVA Graceful Shutdown Chain                      │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ 1. Trigger CancellationToken
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 1: Inbound Halt & Task Interruption                              │
│   - Cancel active LLM generation: cancel_token.store(true)             │
│   - Halt WebRTC audio duplex: stop WASAPI capture, AEC3, Silero VAD    │
│   - Disarm CUA automation: release synthetic Win32 inputs              │
│   - Stop accepting new Tauri IPC commands                              │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ 2. Await In-Flight Tasks
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 2: MPSC Queue & Background Task Drain                            │
│   - Await Tokio JoinSet task termination (governor, watchers, audio)   │
│   - Send DbWriteCommand::Flush through DbActorHandle                   │
│   - Await DbActor acknowledgmentoneshot channel                        │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ 3. Checkpoint WAL
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 3: SQLite WAL Finalization & Connection Drop                     │
│   - Execute PRAGMA wal_checkpoint(TRUNCATE);                           │
│   - Drop r2d2_sqlite reader connection pool                            │
│   - Close and join dedicated DbActor OS thread                         │
└──────────────────────────────────┬─────────────────────────────────────┘
                                   │ 4. Child Process & Vault Cleanup
                                   ▼
┌────────────────────────────────────────────────────────────────────────┐
│ Phase 4: Windows Job Object & Stronghold Vault Teardown                │
│   - Windows NT Job Object: Automatic KILL_ON_JOB_CLOSE terminates child│
│   - Lock & persist IOTA Stronghold vault secrets                       │
│   - Emit Tauri window destroyed events and exit process cleanly        │
└────────────────────────────────────────────────────────────────────────┘
```

## Implementation Reference

```rust
pub async fn execute_graceful_shutdown(
    state: &AppState,
    cancel_token: &tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    tracing::info!("Initiating LIVA native graceful shutdown sequence");

    // Phase 1: Signal cancellation to in-flight tasks and actors
    cancel_token.cancel();
    state.llm.shutdown().await?;

    // Stop voice audio streaming and WASAPI playback
    if let Some(ref tts) = *state.tts.lock().await {
        tts.stop_audio().await;
    }

    // Phase 2: Drain and flush persistent database actor queue
    tracing::info!("Flushing DbActor write queue to SQLite WAL...");
    state.db.writer_actor.flush().await?;

    // Phase 3: SQLite WAL truncation checkpoint
    {
        let conn = state.db.writer_pool.get()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        tracing::info!("SQLite WAL checkpoint TRUNCATE completed successfully");
    }

    // Phase 4: Windows Job Objects automatically terminate sub-processes
    // Child processes spawned via liva-cua or tools attached to JobObject
    // terminate atomically upon handle closure (JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE).

    tracing::info!("LIVA native shutdown complete. Clean exit.");
    Ok(())
}
```

## Anti-Patterns & Prohibited Actions

- ❌ **Abrupt Process Exit**: Calling `std::process::exit(0)` without awaiting `DbActor::flush()` and `PRAGMA wal_checkpoint(TRUNCATE);`.
- ❌ **Unbounded Sleep**: Using `thread::sleep(Duration::from_secs(2))` to "wait" for background operations instead of deterministic Tokio channels.
- ❌ **Detached Child Processes**: Spawning child utilities via `std::process::Command::spawn()` without assigning them to the Windows NT Job Object.
- ❌ **Drop-Order Deadlocks**: Holding a mutex guard across an `.await` boundary in the shutdown handler.

## Verification & Independent Audit

- **WAL Cleanliness**: Verify that `db.sqlite-wal` size drops to 0 bytes or is cleanly truncated upon shutdown.
- **Process Inspection**: Inspect Windows Task Manager or run `Get-Process -Name liva*` in PowerShell to confirm zero orphaned child processes or zombie threads remain.
- **Test Suite**: Run `cargo test -p liva-llm -j 2 -- --test-threads 2` to verify actor preemption and shutdown signal handling.

## Related Notes

- [[Knowledge/liva_architecture|LIVA Architecture]] — Overall system topology and Tauri v2 IPC.
- [[Knowledge/memory_architecture|Memory Architecture]] — Three-tier memory engine, Radix Trie, and SQLite WAL.
- [[Rules/coding_standards|Coding Standards]] — Rust development standards and concurrency invariants.
- [[Rules/tech_stack|Tech Stack]] — Multi-crate dependencies and build profiles.
- [[Knowledge/anti_patterns|Anti-Patterns]] — Prohibited design patterns in the native Rust architecture.
