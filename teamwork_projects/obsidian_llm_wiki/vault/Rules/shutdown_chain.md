---
title: "shutdown_chain"
tags:
  - liva/rule
author: "worker"
last_update: "2026-06-21T02:21:19Z"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Shutdown Chain

## Rule Statement
The system teardown MUST execute sequentially and asynchronously through `CoreKernel.shutdown()` to prevent VRAM hangs, database corruption, or zombie worker processes. No hardcoded sleeps (`setTimeout`) are allowed.

## Rationale
- If `llama-server` is not terminated first, it locks GPU VRAM, preventing other applications from accessing the GPU.
- If database connections are closed before pending memory transactions (Reflection/Consolidation) flush, writes are lost, and SQLite WAL may corrupt.

## The Shutdown Sequence

```typescript
async CoreKernel.shutdown()
  ├── modelOrchestrator.killLlamaServer() // 🚨 STEP 1 (IMMEDIATE): Kill llama-server to release VRAM (local mode only)
  ├── modelOrchestrator.clearExpertCooldown() // Clear TTL to avoid zombie swap attempts
  ├── workerThreadPool.terminateAll()     // Kill toàn bộ node:worker_threads
  ├── clearInterval(gcIntervalId)     // Own GC timer
  ├── fileWatcher.close()             // FSWatcher file handles
  ├── zalo.stop()                     // ZaloPolling timer
  ├── voiceEngine.destroy()           // TTS timers/buffers
  ├── whisperNode.destroy()           // STT engine (NemotronSTTService) + worker thread + listeners
  ├── memory.dispose()                // 🚨 MUST await `unifiedMemory.close()` — no sleep.
  │   ├── reflectionDaemon.flushPending() // Flush pending Φ/Ψ extractions
  │   ├── reflectionDaemon.dispose()      // Clear debounce timer
  │   ├── consolidationCron.dispose()     // Clear idle-check interval
  │   ├── dreamingPipeline.dispose()      // [Dreaming] Clear indexCache
  │   ├── quantStore.dispose()            // QuantStore GC + tensor cache
  │   └── structuredMemory.close()        // SQLite connection
  ├── SensoryManager.dispose()        // 5s GC interval
  ├── EmbeddingService.dispose()      // GPU API client cleanup
  ├── emailManager.dispose()          // Dừng IMAP timer và ngắt kết nối
  ├── voiceSpeaker.dispose()          // Dọn dẹp tiến trình ngầm phát âm thanh (PowerShell TTS)
  ├── gitNexusIndexer.dispose()       // Dừng Background Indexer debounce timer
  ├── proactiveInterestsDaemon.dispose() // [v24] Dừng Shadow Digest Interests cron timer
  ├── proactiveFocusDaemon.dispose()     // [v24] Dừng Shadow Digest Focus cron timer
  └── vramGuard.dispose()               // [v24] Dừng GPU monitor polling interval
```

## Examples

### Compliant Behavior
- Register cleanups in the sequential async chain and await database closing natively:
```typescript
// inside Gateway graceful shutdown
await CoreKernel.shutdown();
console.log("Shutdown complete.");
process.exit(0);
```

### Non-Compliant Behavior
- Using hardcoded timeouts to wait for database writes, or shutting down in random order:
```typescript
// Non-compliant: db connection is closed before llama-server is killed,
// or using setTimeout instead of awaiting Native close.
setTimeout(() => {
  process.exit(0);
}, 2000);
```

## Exceptions
None. Graceful shutdown sequence must be respected under all circumstances.

## Verification & Enforcement
- The shutdown handler logs each step to standard error or logs files.
- Integration tests confirm all processes exited without remaining zombie handles.
