# 🧪 Incubating Modules

This directory contains modules that are **not yet production-ready** but are being preserved
for future integration when their dependencies become available.

## WriteValidationGate.ts

**Status**: ⏸️ Paused — Waiting for NLI (Natural Language Inference) model endpoint.

**Purpose**: SSGM (Stability and Safety Governed Memory) Truth Maintenance System.
Validates proposed memory updates (ΔM) against Core Facts (M_core) to prevent
semantic drift and memory poisoning.

**Why moved here**:
- Phase 1 placeholder uses only regex-based heuristic negation detection.
- The `TODO: Integrate actual NLI model` was never completed.
- Not wired into any production pipeline (dead code).

**Re-activation plan**:
1. When `ModelOrchestrator` exposes a lightweight NLI classification endpoint (e.g., via `router` model).
2. Wire `validateUpdate()` into `ConsolidationCron.processSession()` before `setFact()`.
3. Move back to `src/memory/` and re-enable tests.

**Original location**: `src/memory/WriteValidationGate.ts`  
**Related test**: `tests/memory/WriteValidationGate.test.ts` (marked as `.skip`)
