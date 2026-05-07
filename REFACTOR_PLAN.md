# 🏗️ LIVA MICRO-CORE REFACTORING — MASTER BLUEPRINT

> **MANDATORY SYSTEM PROMPT FOR AI ASSISTANT:**
> Act as a Staff/Principal Systems Engineer. Your primary directive is **Zero-Regression Refactoring**. We are migrating from a Monolithic architecture (God Objects) to an Event-Driven, Micro-Core architecture.
> **READ ALL CONSTRAINTS BEFORE WRITING ANY CODE.**

## 🚨 GLOBAL CONSTRAINTS & RULES (TUYỆT ĐỐI TUÂN THỦ)

1. **The Prime Directive (Zero Test Breakage):** There are 1649 passing tests. YOU MUST NOT BREAK THEM. At the end of every task, `vitest` must pass.
2. **The Strangler Fig Pattern:** When refactoring `CoreKernel`, `AgentLoop`, or `MemoryManager`, **DO NOT DELETE** their existing public properties or methods. Instead, extract the logic into new manager classes, inject them, and turn the old methods into "Delegates/Facades". The external API signature must remain 100% identical.
3. **Banned Patterns (from AI_CONTEXT.md):**
   - ❌ NO `console.log` -> MUST use `logger` from `../utils/logger`.
   - ❌ NO raw `fetch` -> MUST use `safeFetch` from `../utils/HttpClient`.
   - ❌ NO synchronous I/O (`fs.readFileSync`) -> MUST use `fs.promises`.
   - ❌ NO `any` -> Use `unknown` and strict Zod parsing at boundaries.
4. **Required Patterns:**
   - ✅ MUST use native `#` for true private fields (e.g., `#timer`, `#config`).
   - ✅ MUST implement `dispose(): void` for any class holding intervals or event listeners to prevent Memory Leaks.
   - ✅ File writing MUST use Atomic Write pattern (`.tmp` + `rename`).

---

## 🚀 SPRINT 1: THE NERVOUS SYSTEM (Hạ tầng Event & DI)
**Goal:** Establish type-safe communication and feature-flag configuration. DO NOT touch existing monolithic classes yet.

*   **Task 1.1: `KernelConfig` (`src/core/config/KernelConfig.ts`)**
    *   Create a strict TS Interface for system feature flags (`enableVoice`, `enableTelegram`, `enableGitNexus`, `ttsEngine`). Implement a default config factory.
*   **Task 1.2: `EventCatalog` (`src/core/events/EventCatalog.ts`)**
    *   Define a central schema interface mapping event names to their payload types (e.g., `'ai:stream_chunk': { text: string, id: string }`).
*   **Task 1.3: `TypedEventBus` (`src/core/events/TypedEventBus.ts`)**
    *   Implement a generic, type-safe wrapper around `EventEmitter`. Must support typed `on()`, `emit()`, `off()`, and a `dispose()` method.
*   **Task 1.4: Unit Tests**
    *   Write `TypedEventBus.test.ts` to ensure memory leaks don't happen and typing is strictly enforced.

---

## 🧠 SPRINT 2: AGENTLOOP SURGERY (Pipeline Pattern)
**Goal:** Break down the 370-line `handleUserInput()` monolithic function into a sequential pipeline.

*   **Task 2.1: `StreamGenerator` (`src/core/ai/StreamGenerator.ts`)**
    *   Move LLM text generation and chunk-streaming logic here. It must emit chunks via `TypedEventBus` instead of callbacks.
*   **Task 2.2: `ToolParser` (`src/core/ai/ToolParser.ts`)**
    *   Move XML and JSON fallback parsing logic here. Pure functions preferred. Input: Raw LLM string -> Output: Parsed Tool objects.
*   **Task 2.3: `ResponseRouter` (`src/core/ai/ResponseRouter.ts`)**
    *   Handle logic that decides where the response goes (Zalo queue, UI, Voice TTS).
*   **Task 2.4: Strangler `AgentLoop.handleUserInput()`**
    *   Refactor `AgentLoop` to instantiate the 3 classes above. Convert `handleUserInput()` into a clean Orchestrator that chains them sequentially. **Do not change its input/output signature.**

---

## 🫀 SPRINT 3: CORE KERNEL STRANGLER (The God Object Facade)
**Goal:** Extract 700+ lines of wiring from `CoreKernel` WITHOUT breaking its public API or the 944 lines of tests depending on it.

*   ✅ **Task 3.1: Create Sub-systems**
    *   Create `src/core/bootstrap/BootstrapManager.ts` (Handles TTS/STT init, VAD boot).
    *   Create `src/core/hubs/RemoteControlHub.ts` (Wires Telegram, Meta, CDP listeners).
    *   Create `src/core/events/EventPipeline.ts` (Wires internal UI, Audio, and Camera events).
*   ✅ **Task 3.2: Create DI Container**
    *   Create `src/core/DependencyContainer.ts` to hold singleton instances (Config, EventBus, Hubs).
*   ✅ **Task 3.3: The Delegation Pattern (CRITICAL)**
    *   Modify `CoreKernel` constructor to initialize the 3 sub-systems via the DI Container. 
    *   Keep old public methods like `initTelegram()`. Rewrite their body to delegate:
    ```typescript
    public async initTelegram() {
        // Old 50 lines of logic are now moved to RemoteControlHub
        await this.#remoteHub.startTelegram();
    }
    ```

---

## 💾 SPRINT 4: MEMORY MANAGER DECOMPOSITION
**Goal:** Separate Crypto and Cache concerns from Memory Orchestration.

*   ✅ **Task 4.1: `EncryptionEngine` (`src/memory/EncryptionEngine.ts`)**
    *   Extracted AES-256-GCM encrypt/decrypt + Atomic Write helpers (.tmp → rename).
    *   Single Source of Truth: eliminated duplicate crypto code from `MemoryManager.ts` AND `StructuredMemory.ts`.
    *   Static-only class: `EncryptionEngine.encrypt()`, `.decrypt()`, `.writeFileEncrypted()`, `.readFileDecrypted()`, `.initFileEncrypted()`.
*   ✅ **Task 4.2: `RamCacheManager` (`src/memory/RamCacheManager.ts`)**
    *   Extracted bounded FIFO cache (MAX=200, evict to 100) from `MemoryManager.memCache`.
    *   True-private `#cache` field. Methods: `push()`, `getAll()`, `hydrate()`, `injectWarmup()`, `purge()`.
    *   No timers/intervals — no `dispose()` needed (GC-safe by design).
*   ✅ **Task 4.3: Refactor `MemoryManager.ts`**
    *   Facade delegates to `EncryptionEngine` and `RamCacheManager`. Public API 100% unchanged.
    *   `StructuredMemory.ts` also updated to use `EncryptionEngine` (no more inline crypto).
    *   Dedicated tests: `EncryptionEngine.test.ts` (12 tests) + `RamCacheManager.test.ts` (11 tests).
    *   All existing tests pass: 107/107 memory tests, full suite green.

---

## 🧪 SPRINT 5: TEST DEBT & CLEANUP
**Goal:** Replace old monolithic tests with isolated unit tests ONLY AFTER everything works.

*   ✅ **Task 5.1:** Write dedicated, isolated, mock-free tests for `BootstrapManager`, `RemoteControlHub`, `EventPipeline`, and `EncryptionEngine`.
*   ✅ **Task 5.2:** Safely remove outdated mega-tests like `CoreKernel.test.ts` ONLY AFTER the new tests have achieved 100% coverage on the new classes.