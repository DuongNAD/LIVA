# Project: LIVA Mac-to-Main Port & Optimization

## Architecture
LIVA is a hybrid-intelligence AI desktop assistant. This project ports optimizations from the `origin/mac` branch (77 files, ~15,684 additions) to `main` (Windows), updates documentation, ports test suites, and implements new optimizations.

Key modules being ported:
- **ModelOrchestrator** (core/): Speculative decoding, dynamic threads, startup timeout, warm-up tracking
- **VirtualManager** (core/): Graph search, route-based parallel I/O, Vietnamese word boundary detection
- **AgentLoop** (core/): Engine warm-up wait loop, speculative cache validation
- **ConsolidationCron** (memory/): True # private fields, VRAM guards
- **SemanticRouter** (memory/): Route classification improvements
- **AutoGPUSetup** (scripts/): Cross-platform hardware detection
- **SystemHealth** (skills/): Cross-platform system health

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Port Core Services | ModelOrchestrator, VirtualManager, AgentLoop, ConsolidationCron — main feature ports | none | DONE |
| 2 | Port Secondary Files | SemanticRouter, AutoGPUSetup, SystemHealth, ConfigManager, EngineManager, misc | none | DONE |
| 3 | Update consolidated_docs.md | Port llama.cpp wiki docs from origin/mac | none | DONE |
| 4 | Port Upgrade Test Suites | 4 test files: caching, speculative, routing, consolidation — adapt for Windows | M1, M2 | DONE |
| 5 | Research & Implement Optimizations | 3+ proposals, implement 1 with benchmarks | M1 | DONE |
| 6 | Compliance & Regression Testing | tsc --noEmit zero errors, vitest all pass, zero console.log, safeFetch check | M1-M5 | DONE |
| 7 | Memory & Audio Upgrades | Eliminate any-type violations, VAD FIFO queue, integrate WriteValidationGate, enable tests | M6 | DONE |
| 8 | Nemotron STT Fix | Fix dimension mismatch, shape errors, and chunking/overlap/preemphasis in NemotronWorker.ts | M7 | DONE |



## Interface Contracts
### ModelOrchestrator — Speculative Decoding
- New env vars: `LIVA_ENABLE_SPECULATIVE`, `LIVA_DRAFT_MODEL_NAME`
- `#buildLlamaArgs()` adds `--model-draft` and `--draft-max` when enabled
- Graceful fallback when draft model doesn't exist

### VirtualManager — Graph Search
- `#searchGraph(query, maxHops)` method with multi-hop graph traversal
- Route-based parallel I/O: `kg_recall` → graph search, `deep_reasoning` → vector + graph
- `isWholeWordMatch()` for Vietnamese word boundaries

### AgentLoop — Warm-up Wait
- `handleUserInput()` returns `Promise<void>` with 90s max warm-up wait loop
- Checks `modelOrchestrator.isReady()` before proceeding

### ConsolidationCron — Private Fields
- All instance fields migrated to true `#` private syntax
- `isRunning` getter for external status checks
- VRAM guard check in `consolidateNow()`

## Code Layout
- `liva-gateway/src/core/ModelOrchestrator.ts`
- `liva-gateway/src/core/VirtualManager.ts`
- `liva-gateway/src/core/AgentLoop.ts`
- `liva-gateway/src/memory/ConsolidationCron.ts`
- `liva-gateway/src/memory/SemanticRouter.ts`
- `liva-gateway/src/scripts/AutoGPUSetup.ts`
- `liva-gateway/src/skills/personal/SystemHealth.ts`
- `liva-gateway/src/core/config/ConfigManager.ts`
- `liva-gateway/src/evolution/EngineManager.ts`
- `liva-gateway/src/workers/VADWorker.ts`
- `liva-gateway/src/services/WhisperNode.ts`
- `liva-gateway/src/memory/VectorRepository.ts`
- `liva-gateway/src/memory/WriteValidationGate.ts`
- `liva-gateway/src/memory/ConsolidationSteps.ts`
- `liva-gateway/tests/memory/WriteValidationGate.test.ts`
- `liva-gateway/tests/upgrades/` (4 new test files)
- `consolidated_docs.md` (project root)
