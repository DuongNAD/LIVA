# LIVA Strategic Upgrade & Refactoring Plan

## 1. Executive Summary & Codebase Quality Status

Following the Phase 1 Codebase Audit, the system has been scanned programmatically to assess TypeScript compilation errors, linter violations, dependency bloat, file size limits, and banned packages. 

Based on the audit, the current quality status is **CODE RED**. Under the system's Architectural Review guidelines, a score below 70 triggers a CODE RED status, which blocks further feature expansion until codebase stability and standard compliance are restored.

### Key Quality Metrics (Cited from `logs/audit_scan_results.json`)

| Metric | Scanned Value | Health Score Impact | Rationale & Thresholds |
|---|---|---|---|
| **Architecture Health Score** | **0 / 100** | Critical Failure | Capped at 0 (Threshold for Normal: $\ge$ 70) |
| **System Status** | **CODE RED** | Blocking | Blocks feature addition; requires refactoring first |
| **TypeScript Compiler Errors** | **186** | -930 points | Blocker bugs (Deduct 5 points per error) |
| **ESLint Errors** | **2** | -4 points | Severe linter issues (Deduct 2 points per error) |
| **ESLint Warnings** | **467** | -934 points | Minor standard violations (Deduct 2 points per warning) |
| **God Components (>1,200 lines)** | **3** | -15 points | Cognitive complexity risk (Deduct 5 points per component) |
| **Banned Dependencies** | **1** | -2 points | Architecture policy violation (Deduct 2 points per package) |
| **Total TypeScript Files** | **341** | Metric | Total files scanned across the workspace |
| **Total Workspace Dependencies** | **113** | Metric | Total dependencies across all package.json files |

---

## 2. Banned Package Remediation Plan

### 2.1. Identified Violation
- **Banned Package:** `@huggingface/transformers` (version `^4.2.0`)
- **Location:** `liva-gateway/package.json` under `dependencies`

### 2.2. Rationale & Impact
Running CPU-bound tensor models inside the main Node.js event loop blocks execution for upwards of 10-15ms per chunk, freezing the Gateway and violating **AI_CONTEXT Directive 3.1** (Event Loop Protection). The linter rule in `eslint.config.mjs` explicitly bans this library:
> *"BANNED: CPU Tensor blocks Event Loop. Use llama-server /v1/embeddings"*

### 2.3. Upgrade Proposals
1. **Remove Dependency:** Uninstall `@huggingface/transformers` from `liva-gateway/package.json`.
2. **Transition Embeddings Pipeline:** Ensure all embedding lookups are routed through either:
   - The decoupled `/v1/embeddings` GPU endpoint via `llama-server`.
   - The allowed `onnxruntime-node` package running inside a separate Worker Thread (`EmbeddingWorker.ts`), preventing the event loop from being blocked.

---

## 3. God Component Decoupling Plan

The audit identified three major "God Components" exceeding the strict 1,200-line limit. These files represent high maintenance risks, violating the Single Responsibility Principle and memory isolation boundaries.

```
liva-gateway
├── src/
│   ├── core/
│   │   ├── CoreKernel.ts      (2,458 lines — 110.88 KB)
│   │   └── AgentLoop.ts       (1,813 lines — 103.47 KB)
│   └── memory/
│       └── StructuredMemory.ts (1,292 lines — 54.42 KB)
```

### 3.1. CoreKernel.ts (2,458 lines)
- **Problem:** Handles system lifecycle scheduler, event dispatching, IPC routing, and sandbox controls in a single monolithic coordinator.
- **Proposed Optimization:**
  - Decouple the system event lifecycle loop into a standalone `KernelLifecycleManager.ts`.
  - Extract the scheduling logic to `kernel/Scheduler.ts` completely, removing scheduling state from the core kernel.
  - Move IPC sidecar handshake coordination to a dedicated `IPCGatewayServer.ts`.

### 3.2. AgentLoop.ts (1,813 lines)
- **Problem:** Orchestrates user inputs, speculative decoding cache validation, VRAM cost checks, and fallback mechanisms.
- **Proposed Optimization:**
  - Extract speculative decoding validation logic into a dedicated helper class `SpeculativeCacheManager.ts`.
  - Move the 90s engine warm-up wait loop to `EngineWarmUpOrchestrator.ts`.
  - Decouple the user interface console response handling into `UIResponseRouter.ts`.

### 3.3. StructuredMemory.ts (1,292 lines)
- **Problem:** Manages all memory representations including L0 (QuantStore), L1 (Turn Layer), L2 (Vector Repository), and L3 (Graph/Facts).
- **Proposed Optimization:**
  - Split StructuredMemory into distinct files corresponding to the Unified Hybrid Memory (UHM v2) layer design.
  - Delegate vector queries exclusively to `VectorRepository.ts`.
  - Migrate knowledge facts extraction logic into a separate `KnowledgeGraphWeaver.ts`.

---

## 4. TypeScript Type Safety & Blocker Bug Cleanup Plan

With **186 compiler errors** reported by `tsc --noEmit -p liva-gateway/tests/tsconfig.json`, restoring compilation integrity is a high priority.

### 4.1. Major Error Patterns Detected
1. **Implicit Any Types:**
   - E.g., `HITLGuard.ts` line 111-112: `Element implicitly has an 'any' type because type 'typeof globalThis' has no index signature.`
   - E.g., `PresenceDetectorChallenger.test.ts` lines 34, 60, 153: Parameter implicitly has an 'any' type.
2. **Missing Declaration Files:**
   - E.g., `nodemailer` in `SendEmail.ts` and `qrcode` in `QRCodeTool.ts` lack typescript type definitions.
3. **Type Incompatibilities in Test Mocks:**
   - E.g., `ConsolidationCron.test.ts` lines 74, 83, 90: Passing synchronous arrays instead of `Promise<EventBrick[]>`.
   - E.g., `ModelOrchestrator.test.ts` lines 358, 383, 408: Missing required properties (like `errorMessage` or `loadedModel`) in simulated `SwapModelResult` objects.

### 4.2. Refactoring Steps
1. **Install Missing Types:** Add `@types/nodemailer` and `@types/qrcode` as devDependencies.
2. **Resolve Implicit Any:** Enforce strict type signatures on all function parameters and use type assertions (`as any` only when absolutely necessary, prioritizing custom interface types or branded types).
3. **Align Mock Architectures:** Refactor vitest/jest mock structures to ensure async methods return Promises and complete interface contracts (e.g. including both `success` and `errorMessage` on mock returns).

---

## 5. Linter Warnings & Standards Compliance Plan

ESLint reported **2 errors** and **467 warnings**, primarily in `liva-gateway/src`.

### 5.1. Target Violations
- **no-console:** Source files contain `console.log` and `console.error` calls.
- **no-explicit-any:** Extensive use of `any` types which dilutes type safety and violates the zero-any policy.
- **no-unused-vars:** Variables initialized but never consumed.

### 5.2. Remediation Proposals
1. **Logger Integration:** Replace all `console.log` statements in source files with the allowed `pino` logger (`import { logger } from "../utils/Logger"`). 
   - *Note:* Keep Tauri sidecar stdout handshake intact as required by the interlock guards.
2. **Unused Code Cleanup:** Remove all unused imports and variables, or prefix them with `_` to suppress warnings where they are required for future extension/conformance.
3. **Zero-Any Policy:** Replace `any` typings with union types, interfaces, or generics.

---

## 6. Verification & Implementation Schedule

Refactoring will be carried out in three distinct sprints to systematically restore the system health score to $\ge 90$.

| Phase | Sprint Focus | Verification Command | Exit Criteria |
|---|---|---|---|
| **Phase A** | Banned dependency removal & strict type checking fixes | `npx tsc --noEmit -p liva-gateway/tests/tsconfig.json` | 0 TypeScript errors |
| **Phase B** | ESLint warning remediation & logger migration | `npx eslint liva-gateway/src` | 0 ESLint errors & warnings |
| **Phase C** | God component modularization & UHM v2 split | `npx tsx tests/audit_profiler.ts` | God component count = 0; Score $\ge 90$ |
