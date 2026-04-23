# 🤖 LIVA System — AI Developer Context & System Guidelines
# Last Updated: 2026-04-22 (Post-Audit) | Maintainer: Dương (System Architect)

> [!CAUTION]
> **🤖 MANDATORY AI & DEV INSTRUCTION:**
> 
> **1. READ PROTOCOL (Pre-flight Check):**
> Before you start analyzing, planning, or executing ANY task, you **MUST silently read this `AI_CONTEXT.md`** file.
> This file contains the **Single Source of Truth** for the project architecture, memory flows, and coding conventions.
> Always align your actions with the rules defined here. **Do not skip this step.**
> 
> **2. WRITE PROTOCOL (Continuous Context Sync):**
> `AI_CONTEXT.md` is a **living document**. Whenever you implement a NEW feature, add a new module,
> modify existing architecture, or change dependencies, you **MUST update `AI_CONTEXT.md`** accordingly
> in the **same Pull Request**. The Single Source of Truth must evolve alongside the codebase.
> **Never leave it desynchronized.**

> This file is the **supreme law** for any AI assistant working on this codebase.
> It encodes architecture decisions, banned patterns, and hard-won lessons from production debugging.
> Treat every rule here as a compile-time constraint — violations break the system.

---

## 1. 🎯 AI Persona & Core Directives

- **Role:** You are a Principal Software Engineer and System Architect.
- **Mindset:** Security First → Performance → Clean Code.
- **Mandatory Behaviors:**
  - THINK STEP-BY-STEP before writing code. Analyze blast radius.
  - Go straight to the point. NO apologies, NO filler phrases.
  - NEVER use placeholders inside the specific function/class you are modifying. Write the COMPLETE logic for that block. However, to save output tokens in large files (>300 lines), you MAY omit entirely unchanged functions.
  - If a request is ambiguous or violates architecture, STOP and ASK before coding.

---

## 2. 🏗️ Project Overview

**LIVA** is a **local-first, multi-agent AI desktop assistant** with 4 subsystems:

```
┌──────────────────────────────────────────────────────────────┐
│                      LIVA SYSTEM ARCHITECTURE                │
├──────────────┬───────────────┬──────────────┬────────────────┤
│  liva-ui     │ openclaw-     │ llama-server │ liva-          │
│  (Electron)  │ gateway       │ (C++ Native) │ dashboard      │
│              │ (Node.js/TS)  │ GGUF Runtime │ (Web)          │
│  Desktop UI  │ Agent Brain   │ GPU Offload  │ Analytics      │
│  WebSocket ←→│ FSM + Memory  │ Zero-Python  │                │
│              │ + Skills      │ CUDA/Vulkan  │                │
└──────────────┴───────────────┴──────────────┴────────────────┘
```

### Startup Sequence (`npm run desktop` / `electron.cjs`)
1. `openclaw-gateway` → `tsx src/Gateway.ts` → `AutoGPUSetup` (hardware detection)
2. `openclaw-gateway` → `ModelOrchestrator` → spawns `llama-server.exe` (C++ native, port 8000)
3. `liva-ai-engine` → `voice_engine.py` (Edge-TTS, optional)
4. `liva-ui` → Electron desktop app (connects via WebSocket to port 8082)

### Architectural Boundaries (STRICT)
- `/core` layer NEVER calls database directly — must go through `/memory`
- `/skills` are self-contained MCP tools — each exports `metadata` + `execute()`
- `/security` guards are applied at the AgentLoop level, not per-skill
- Gateway ↔ Engine communication: gRPC (prod) or OpenAI-compatible HTTP (dev)
- **Tauri Sidecar Giao Tiếp**: Gateway chạy nền (Daemon) và giao tiếp với Tauri UI qua kiến trúc **Dynamic WS Handshake**. 
  - `console.log` đã bị khoá (`stdout` Guard) để chỉ in ra đúng 1 dòng JSON `{event: "GATEWAY_READY", port: <dynamic>, token: <uuid>}`.
  - **TUYỆT ĐỐI KHÔNG IN RA STDOUT**. Mọi log khác phải dùng `logger` (Pino) trỏ qua `stderr`.

---

## 3. 🚫 Tech Stack — Allowed vs Banned

### ✅ ALLOWED (Use ONLY these)

| Category | Technology | Notes |
|----------|-----------|-------|
| Runtime | Node.js v22+ | **MUST** use ESM (`"type": "module"` in package.json) |
| Language | TypeScript 5.x (strict) | Python optional (voice_engine only) |
| LLM Runtime | `llama-server.exe` (C++) | Zero-Python, CUDA/Vulkan GPU offload |
| Network | Native `fetch` via `safeFetch()` | Wrapper at `src/utils/HttpClient.ts` |
| Database | `node:sqlite` (built-in) | Used in StructuredMemory |
| Vector DB | `@lancedb/lancedb` | Used in TurboQuantStore, LanceMemory |
| Embeddings | `@huggingface/transformers` v4 | **Via `EmbeddingService` singleton ONLY** (see §5) |
| Browser | `playwright-core` | API-only, no bundled browsers |
| Search | `flexsearch` | Document indexing in HeraCompass |
| Logger | `pino` + `pino-pretty` | Async worker thread, structured JSON |
| Testing | `vitest` (TS), `pytest` (Python) | `vi.stubGlobal('fetch')` for mocking |
| Validation | `zod` | Schema validation |
| Caching | `lru-cache` | Bounded eviction (PromptBuilder, etc.) |
| LLM Client | `openai` SDK | Compatible with local & cloud endpoints |

### ❌ BANNED (NEVER USE — these were deliberately removed)

| Library | Reason | Replacement |
|---------|--------|-------------|
| ❌ `axios` | Removed in Phase 3 hardening | `safeFetch()` from `src/utils/HttpClient.ts` |
| ❌ `puppeteer` | 500MB bloatware, ABI crash with Electron | `playwright-core` (2MB, API only) |
| ❌ `fuse.js` | O(N) per search, memory hog | `flexsearch` Document indexing |
| ❌ `@xenova/transformers` | Deprecated, unmaintained | `@huggingface/transformers` v4 |
| ❌ `request` / `got` / `node-fetch` | Redundant with native fetch | `safeFetch()` |
| ❌ `console.log` / `console.error` | No structure, blocks event loop | `logger.info()` / `logger.error()` from pino |
| ❌ `fs.readFileSync` / `fs.writeFileSync` | Blocking I/O on main thread | `fs.promises.*` or pino async transport |
| ❌ `__dirname` / `__filename` | Not available in ESM | `import.meta.dirname` / `import.meta.filename` |

---

## 4. 📜 Coding Standards

### 4.1. Network — THE CRITICAL RULE

<CRITICAL_DIRECTIVE>

> **Native `fetch` does NOT throw on HTTP 4xx/5xx!**
> It only throws on network failure (ECONNREFUSED, DNS, timeout).
> HTTP 400/500 = Promise RESOLVED. Your code silently succeeds. Data is garbage.

**MANDATORY**: Use `safeFetch()` for ALL network calls:

```typescript
import { safeFetch } from "../utils/HttpClient";

// ✅ CORRECT — throws on 4xx/5xx, auto-timeout, no timer leak
const res = await safeFetch(url, { method: "POST", body: JSON.stringify(data) }, 5000);
const json = await res.json();

// ❌ WRONG — silently swallows HTTP errors
const res = await fetch(url, options);
const json = await res.json(); // crashes if server returns HTML error page
```

**Error handling hierarchy for fetch errors:**
```typescript
catch (e: any) {
  // Native fetch buries the real error in e.cause
  const errMsg = e.cause?.message || e.message || "Unknown error";
}
```

</CRITICAL_DIRECTIVE>

### 4.2. Timer & Memory Leak Prevention

```typescript
// ✅ CORRECT — guaranteed cleanup
const controller = new AbortController();
const timeoutId = setTimeout(() => controller.abort(), 5000);
try {
    await fetch(url, { signal: controller.signal });
} finally {
    clearTimeout(timeoutId); // ALWAYS in finally, never in try
}

// ❌ WRONG — timer leaks on fetch failure
try {
    const timeoutId = setTimeout(...);
    await fetch(url);
    clearTimeout(timeoutId); // never reached if fetch throws!
}
```

### 4.3. File I/O — Atomic Write Pattern (MANDATORY)

```typescript
import { promises as fsp } from "fs";

// ✅ CORRECT — Atomic: write .tmp then rename (prevents corrupt file on crash)
const tmpPath = `${dbPath}.tmp`;
await fsp.writeFile(tmpPath, data, "utf-8");
await fsp.rename(tmpPath, dbPath); // rename is atomic on same filesystem

// ❌ WRONG — Direct write can corrupt file if interrupted
await fsp.writeFile(dbPath, data, "utf-8");

// ❌ WRONG — Blocking I/O on main thread
fs.writeFileSync(dbPath, data);  // Freezes event loop for 50-200ms
```

### 4.4. TypeScript Conventions

- **Branded Types** for security-sensitive IDs:
  ```typescript
  type TaskToken<T extends string> = T & { readonly __brand: unique symbol };
  ```
- **Early Return** — max nesting depth: 3. Flatten with guard clauses.
- **No `any`** — use `unknown` and narrow with type guards. When parsing JSON from external APIs or LLM outputs, it is **MANDATORY** to use `zod` schemas to parse and validate data at the boundary. NEVER blindly cast with `as any`.
- **Private fields** — use `#field` (true private) for process handles, crypto keys.

### 4.5. Logging

```typescript
import { logger } from "../utils/logger";

// ✅ CORRECT — structured, async, searchable
logger.info("Message loaded successfully");
logger.error({ err, context: "ZaloPolling" }, "Failed to parse update");

// ❌ WRONG — blocking, unstructured, no context
console.log("loaded");
console.error(err);
```

### 4.6. JSON Parsing from LLM Output

```typescript
// ✅ CORRECT — defensive extraction with repair
import { jsonrepair } from "jsonrepair";

const first = text.indexOf('{');
const last = text.lastIndexOf('}');
if (first === -1 || last === -1) return null;
const parsed = JSON.parse(jsonrepair(text.substring(first, last + 1)));

// ❌ WRONG — greedy regex that swallows preamble/trailing text
const match = text.match(/{[\s\S]*}/);
JSON.parse(match[0]); // breaks on nested braces, markdown code fences
```

### 4.7. Browser Automation (playwright-core)

```typescript
import { getOrCreateBrowser } from "../utils/PlaywrightBrowser";

// ✅ CORRECT — Use shared browser singleton (auto-detects system Chrome/Edge)
const { browser, context } = await getOrCreateBrowser("my_profile");
const page = await context.newPage();

// ✅ ALSO CORRECT — Direct launch with explicit executable path
import { chromium } from "playwright-core";
const browser = await chromium.launch({ executablePath: "/path/to/chrome" });

// ❌ WRONG — Standard playwright downloads 500MB+ Chromium
import { chromium } from "playwright"; // DO NOT USE
const browser = await chromium.launch(); // Crashes: no bundled browser in -core
```

---

## 5. 📂 Project Structure — openclaw-gateway

```
src/
├── core/                    # 🧠 Agent Brain (SENSITIVE — no side effects)
│   ├── AgentLoop.ts         # Main FSM: IDLE→THINKING→ACTING→REFLECTING
│   ├── CoreKernel.ts        # Authority tokens, phase transitions, shutdown chain
│   ├── CoreKernelAuthority.ts # Extracted sub-agent: authority token validation
│   ├── DualPortController.ts  # Extracted sub-agent: Router↔Expert switching
│   ├── ToolExecutionOrchestrator.ts # Extracted sub-agent: tool output validation
│   ├── LTCOrchestrator.ts   # Extracted sub-agent: long-term concept extraction
│   ├── TaskLaneWorker.ts    # Extracted sub-agent: async task queue (timeout-safe)
│   ├── ModelOrchestrator.ts  # Dual-port LLM management (Router:8000, Expert:8001)
│   ├── PromptBuilder.ts     # System prompt assembly (4-tier memory injection)
│   ├── ZaloPolling.ts       # Inbound message listener (long-polling)
│   ├── TelemetryProfiler.ts # Performance metrics (debounced write)
│   ├── ASTActuator.ts       # Code modification via AST
│   ├── ASTHealer.ts         # Auto-fix broken code
│   └── UIController.ts      # WebSocket bridge to Electron (token auth)
│
├── memory/                  # 💾 Persistence Layer (math-optimized)
│   ├── StructuredMemory.ts  # Key-value facts (node:sqlite, TTL, FIFO eviction)
│   ├── TurboQuantStore.ts   # Quantized vector memory (4-bit KV cache)
│   ├── LanceMemory.ts       # Semantic RAG (@lancedb + @huggingface embeddings)
│   ├── HeraCompass.ts       # Error insight DB (flexsearch, utility scoring)
│   ├── PersonalKnowledgeExtractor.ts  # Auto-extract user preferences
│   └── SensoryManager.ts    # Multi-modal input aggregation (TTL + GC)
│
├── skills/                  # 🔧 MCP Tools (self-contained, each exports metadata+execute)
│   ├── WebSearch.ts         # Tavily API + DuckDuckGo fallback
│   ├── WebBrowser.ts        # playwright-core page interaction
│   ├── ComputerUse.ts       # Desktop automation (screenshots, clicks)
│   ├── SendZaloBot.ts       # Zalo Bot Creator API messaging
│   ├── SendZaloRPA.ts       # Zalo contact messaging (browser automation)
│   ├── SendMessengerRPA.ts  # Facebook Messenger automation
│   ├── ExecuteCommand.ts    # Shell command execution (sandboxed)
│   ├── ReadLocalFile.ts     # File system read
│   ├── WriteLocalFile.ts    # File system write
│   ├── ReportWriter.ts      # Multi-section report generation
│   ├── ResearchIdeation.ts  # AI Scientist ideation pipeline
│   └── ... (29 skills total)
│
├── security/                # 🛡️ Guardrails
│   ├── ZMAS_Guard.ts        # Multi-layer output filter (URL, PII, injection, creds)
│   ├── HITLGuard.ts         # Human-in-the-loop approval gate (60s timeout)
│   └── RPAGuardrails.ts     # RPA action validation
│
├── mcp/                     # 🔌 Model Context Protocol
│   ├── MCPClientManager.ts  # Singleton client for connecting MCP servers
│   ├── MCPHost.ts           # MCP host for exposing LIVA tools
│   ├── LocalMCPServer.ts    # Local MCP server implementation
│   └── LocalAdapterServer.ts # MCP adapter bridge
│
├── sandbox/                 # 📦 Code Isolation
│   └── MicroVMDaemon.ts     # LocalSandbox with filesystem deny list
│
├── evolution/               # 🧬 Singularity Pipeline (DAG)
│   ├── EvolutionPipeline.ts # Main Orchestrator
│   ├── EngineManager.ts     # Safe VRAM & Hot-swap
│   ├── ASTMutator.ts        # Direct AST surgery (No SkillRegistry)
│   └── RollbackManager.ts   # Safe rollback on failure
│
├── services/                # 🎤 Peripheral Services
│   ├── EmbeddingService.ts  # ⭐ Singleton embedding (Promise Lock, all-MiniLM-L6-v2)
│   ├── VoiceEngine.ts       # TTS token streaming
│   ├── KokoroVoiceEngine.ts # Kokoro-JS ONNX TTS (local-first)
│   ├── WhisperNode.ts       # Speech-to-text (safeFetch to Whisper API)
│   └── WhisperJSNode.ts     # Pure JS Whisper (ONNX fallback)
│
├── utils/                   # 🔨 Shared Utilities
│   ├── HttpClient.ts        # ⭐ safeFetch() — THE fetch wrapper (timeout + !res.ok)
│   ├── PlaywrightBrowser.ts # Browser singleton factory (auto-detect Chrome/Edge)
│   ├── logger.ts            # Pino async logger
│   ├── ZaloNotifier.ts      # Fire-and-forget Zalo notifications
│   ├── LivaEngine.ts        # LLM client factory (SecureLivaEngine + Seal Token)
│   ├── NativeIPCClient.ts   # gRPC client to Python engine (GRPCStream async iter)
│   └── DockerSandbox.ts     # Ephemeral Docker container management
│
├── auto_singularity.ts      # 🧬 Entrypoint for EvolutionPipeline (Refactored to DAG)
├── Gateway.ts               # Entry point
├── SkillRegistry.ts         # Dynamic skill loader + MCP fallback
├── MemoryManager.ts         # Memory singleton factory
└── system_prompt.ts         # System prompt template
```

---

## 6. 🛑 Anti-Patterns — Hard-Won Lessons (ADD TO THIS LIST!)

### Networking
- **fetch Silent Failure**: `fetch` resolves on HTTP 400/500. ALWAYS use `safeFetch()`.
- **Timer Leak**: `clearTimeout` MUST be in `finally`, not after `await fetch()`.
- **ECONNREFUSED Location**: Native fetch error message = "fetch failed". Real error is in `e.cause.message`, NOT `e.message`.
- **Axios Ghost Properties**: After migration, `e.response?.data` is DEAD CODE. Native fetch errors don't have `.response`.

### Singleton & Resource Management
- **Duplicate Model Loading**: NEVER instantiate `@huggingface/transformers` pipeline directly. Use `EmbeddingService.getInstance()`. Each pipeline load costs ~140MB RAM.
- **Missing `dispose()`**: Every service with timers (`setInterval`/`setTimeout`) or ML models MUST expose a `dispose()` or `destroy()` method. Call them in `CoreKernel.shutdown()`.
- **Zombie Timer on Recursive setTimeout**: Store the timer ref (`this.pollTimerRef = setTimeout(fn, ms)`) and `clearTimeout` it in `stop()`.

### Database
- **SQLite WAL Mode**: Always enable `PRAGMA journal_mode = WAL` + `PRAGMA synchronous = NORMAL` on init. Without WAL, concurrent reads during writes cause `SQLITE_BUSY`.
- **Duplicate DB Instances**: Never `new StructuredMemory()` in multiple places. Inject via `MemoryManager.getStructuredMemoryInstance()`.

### File I/O
- **Atomic Write**: ALWAYS use `.tmp` + `rename()` pattern for persistent data files. Direct `writeFile` can corrupt data on crash/concurrent write.
- **Sync I/O in Hot Path**: `fs.readFileSync` + `fs.appendFileSync` × 3 = 3 blocking calls per event. Use debounced async writes.

### Cache
- **Unbounded Map Cache**: NEVER use `new Map()` for caching without eviction. Use `lru-cache` with `{ max, ttl }`.
- **O(N) Cache Keys**: Don't use `array.join(",")` for cache keys. Use `Buffer.from(new Float32Array(v).buffer).toString("base64")`.

### Code Generation
- **Greedy Regex JSON**: `/{[\s\S]*}/` swallows multiple JSON blocks. Use `indexOf('{')` + `lastIndexOf('}')` + `jsonrepair`.
- **Duplicate try Blocks**: Multi-edit tools can generate `try { try {` when replacing code inside existing try blocks. Always verify brace nesting after automated edits.
- **Singularity Circular Dependency**: Singularity Pipeline bắt buộc dùng AST nội bộ (`ASTActuator`), không dùng Skill Tool (`SkillRegistry`), và phải có luồng Rollback (`RollbackManager`).

### gRPC Streaming
- **Async Iterator Data Loss**: `GRPCStream.pushChunk()` MUST always queue chunks to the buffer array, then signal the iterator via `resolveNext()`. NEVER pass data through the promise resolution value — the iterator discards it. (Fixed: 2026-04-22)
- **Drain Before Error**: After the iterator wakes from `await`, ALWAYS loop back to drain the chunk queue before checking `this.error`. Otherwise, chunks received before `fail()` are silently dropped.

### Timer Management
- **Race Timeout in Promise.race**: When using `Promise.race([task, timeout])`, ALWAYS store the `setTimeout` ID and call `clearTimeout()` in `.finally()`. Without this, the 5-minute timeout leaks on every successful task. (Fixed: TaskLaneWorker, 2026-04-22)

### Electron / Packaging (Node.js SEA)
- **ABI Mismatch**: Native C++ addons (`isolated-vm`, `better-sqlite3`) crash with `electron-rebuild`. Prefer: `node:sqlite` (built-in) or WASM alternatives.
- **Node.js SEA (Single Executable Application)**: Khi bundle file bằng `esbuild` qua `build-sea.js`, **BẮT BUỘC** phải đưa các thư viện Native C++ (`@lancedb/lancedb`, `sqlite3`) vào mục `external: [...]`. Script hậu kỳ phải copy thủ công các file `.node` từ `node_modules` ra nằm ngang hàng với file `.exe` sinh ra.
- **Bundled Browsers**: `puppeteer` downloads 500MB+ Chromium. Use `playwright-core` (API only, 2MB) + system Chrome via `executablePath`.

### Testing
- **False Green**: 100% pass rate means NOTHING if tests only mock happy paths. Every fetch mock MUST include at least one 4xx/5xx negative test case.
- **Mock fetch Correctly**: Use `vi.stubGlobal('fetch', vi.fn())` — NOT `axios-mock-adapter` or `nock`.
- **UIController Tests**: MUST push `--dev` to `process.argv` before creating UIController instance, and restore in `afterEach`. This bypasses `randomUUID`-based token auth that is inaccessible to test mocks.
- **Fake Timer + Promise Rejection**: When testing timeout behavior with `vi.useFakeTimers()`, attach a `.catch()` handler to the promise BEFORE calling `vi.advanceTimersByTimeAsync()`. Otherwise, the rejected promise becomes an unhandled rejection before `await expect().rejects` can catch it.
- **Module-level Mock Completeness**: When mocking `fs`, include ALL methods used by the target module (`readFile`, `writeFile`, `rename`, `existsSync`, `mkdirSync`). Missing methods cause silent failures in async handlers that swallow errors via try/catch.

### Performance
- **Double Eviction**: Don't call `evictExpired()` then `getAllFacts()` — the latter already calls eviction internally.

---

## 7. 🔑 Environment Variables

```bash
# AI Provider: "local" (GGUF via llama-server) or "cloud" (OpenAI-compatible API)
AI_PROVIDER=local
AI_BASE_URL=           # Cloud API endpoint (only when AI_PROVIDER=cloud)
AI_API_KEY=            # Cloud API key
AI_MODEL=              # Cloud model name
AI_MODELS_DIR=         # Local model directory (default: E:\AI_Models)
ROUTER_MODEL_NAME=     # Light model for routing (default: gemma-4-E4B-it-Q4_K_M.gguf)
EXPERT_MODEL_NAME=     # Heavy model for deep tasks

# Integrations
ZALO_OA_ACCESS_TOKEN=  # Zalo Bot Creator token (contains ":")
ZALO_USER_ID=          # Auto-detected on first message
TAVILY_API_KEY=        # Web search (free 1000/month, falls back to DDG)
EMAIL_HOST=            # IMAP server
EMAIL_USER=            # Email address
EMAIL_PASS=            # App-specific password

# Internal
LIVA_USE_NATIVE=       # "true" to use gRPC native engine (bypass HTTP health check)
```

---

## 8. 🧪 Testing Conventions

```bash
# Run all tests
npx vitest run

# Watch mode
npx vitest watch

# Run specific test file
npx vitest run tests/utils/HttpClient.test.ts
```

**Rules:**
- Test files mirror source structure: `src/memory/X.ts` → `tests/memory/X.test.ts`
- **NEVER** call real APIs in tests. Mock with `vi.stubGlobal('fetch', vi.fn())`
- **ALWAYS** include negative test cases (HTTP 4xx/5xx, timeout, malformed input)
- SQLite tests must clean up: delete `.sqlite` files in `afterEach`
- Current baseline: **36 test files, 401 tests** (Updated 2026-04-23)

**Test File Map:**
```
tests/
├── core/
│   ├── AgentLoop.test.ts          # Sub-agents: CKA, DualPort, TEO, LTC, TaskLane
│   ├── HeartbeatManager.test.ts   # Interval start/stop, heartbeat trigger
│   ├── IsolatedAgentTurn.test.ts  # Background turn, XML tool parsing
│   ├── ModelOrchestrator.test.ts  # TaskToken, health check, VRAM
│   ├── NativeIPCClient.test.ts    # gRPC unary + streaming + health
│   ├── PromptBuilder.test.ts      # Context assembly, tool RAG, skill filter
│   ├── TaskQueue.test.ts          # Sequential processing, singleton
│   ├── TelemetryProfiler.test.ts  # Perf tracking, timing accuracy
│   ├── UIController.test.ts       # WebSocket pool, broadcast, config SSOT
│   └── ZaloPolling.test.ts        # Token validation, message emit, offset
├── memory/
│   ├── HeraCompass.test.ts        # RAG insight, utility score, GC
│   ├── PersonalKnowledgeExtractor.test.ts # Fact extraction, JSON safety
│   ├── SensoryManager.test.ts     # Capture, TTL, prompt injection
│   ├── StructuredMemory.test.ts   # SQLite CRUD, TTL, eviction
│   ├── TurboQuantStore.test.ts    # Quantized vector memory, search
│   └── WorkingBuffer.test.ts      # Token budget, snapshot, compaction
├── security/
│   ├── HITLGuard.test.ts          # Approval flow, timeout, double-response
│   ├── RPAGuardrails.test.ts      # RPA action validation
│   ├── ZMAS_Adversarial.test.ts   # Jailbreak, obfuscation, 1MB payload
│   └── ZMAS_Guard.test.ts         # URL, PII, injection, credential filters
├── sandbox/
│   └── MicroVMDaemon.test.ts      # Command blocklist, filesystem deny
├── services/
│   ├── EmbeddingService.test.ts   # Singleton, Promise Lock, embed, dispose
│   ├── KokoroVoiceEngine.test.ts  # TTS streaming, preempt, destroy
│   └── WhisperJSNode.test.ts      # STT model lifecycle
├── skills/
│   ├── DeleteLocalFile.test.ts    # Path guardrails, boot file protection
│   ├── ExecuteCommand.test.ts     # Whitelist security, HITL approval
│   ├── GetSystemInfo.test.ts      # OS/CPU/RAM info retrieval
│   ├── ListDirectory.test.ts      # Directory listing, error handling
│   ├── ReadLocalFile.test.ts      # File read, Unicode, error cases
│   ├── UpdateMemory.test.ts       # Category routing, TTL
│   ├── WebSearch.test.ts          # Tavily + DDG fallback, error paths
│   └── WriteLocalFile.test.ts     # Atomic write, path guardrails
├── utils/
│   ├── HttpClient.test.ts         # safeFetch, timeout, 4xx/5xx
│   └── ZaloNotifier.test.ts       # Bot Creator vs OA API, fire-and-forget
├── mcp/
│   └── MCPClientManager.test.ts   # Singleton, method surface
└── SkillRegistry.test.ts          # Built-in skills, MCP fallback
```

---

## 9. 📋 Commands Quick Reference

```bash
# Development
npx tsx src/Gateway.ts          # Start gateway
npx vitest run                  # Run tests
npx vitest watch                # Watch mode

# Self-Evolution (AI research pipeline)
cross-env NODE_OPTIONS="--expose-gc --max-old-space-size=8192" npx tsx src/auto_singularity.ts

# Full system startup (Windows)
start_all.bat                   # Starts: Engine → Voice → Gateway → UI

# GitNexus (code intelligence)
npx gitnexus analyze            # Rebuild code graph
npx gitnexus analyze --embeddings  # With semantic embeddings
```

---

## 10. 🗺️ Key Data Flows

### User Message → AI Response
```
User Input (Electron WebSocket)
  → UIController.ts
  → AgentLoop.ts (FSM: IDLE → THINKING)
  → PromptBuilder.ts (injects 4-tier memory + system prompt)
  → ModelOrchestrator.ts (Router:8000 or Expert:8001)
  → LLM generates response + optional tool calls
  → SkillRegistry.ts → skill.execute()
  → ZMAS_Guard.ts (filter output)
  → AgentLoop.ts (REFLECTING → IDLE)
  → UIController.ts → Electron
```

### Memory Architecture (4-tier)
```
Tier 1: StructuredMemory (SQLite)  — Deterministic facts ("user likes X")
Tier 2: LanceMemory (LanceDB)     — Semantic RAG vector search
Tier 3: PersonalKnowledge          — Auto-extracted preferences
Tier 4: TurboQuantStore            — Long-term quantized concepts
```

### Error Self-Healing
```
Tool fails → HeraCompass.learnFromError() → LLM generates rule
Next attempt → HeraCompass.getRelatedInsight() → Injects past lesson
Success/Failure → updateUtilityScore() → Verified or Garbage-Collected
```

---

## 11. 🔒 Shutdown Chain (`CoreKernel.shutdown()`)

Every resource with cleanup requirements is called in order:

```
CoreKernel.shutdown()
  ├── clearInterval(gcIntervalId)     // Own GC timer
  ├── fileWatcher.close()             // FSWatcher file handles
  ├── zalo.stop()                     // ZaloPolling timer
  ├── voiceEngine.destroy()           // TTS timers/buffers
  ├── whisperNode.destroy()           // STT model + listeners
  ├── memory.dispose()                // QuantStore GC + SQLite close
  ├── SensoryManager.dispose()        // 5s GC interval
  └── EmbeddingService.dispose()      // 140MB ONNX model
```

> [!IMPORTANT]
> When adding a new service with timers, intervals, or ML models,
> you **MUST** add its cleanup call here. This is enforced by Write Protocol.

---

## 12. 🚨 ESLint Guards (Recommended CI Rules)

```jsonc
// .eslintrc.json
{
  "rules": {
    "no-restricted-imports": ["error", {
      "paths": [
        { "name": "@xenova/transformers", "message": "BANNED: Use EmbeddingService singleton" }
      ]
    }],
    "no-restricted-globals": ["error",
      { "name": "fetch", "message": "Use safeFetch() from src/utils/HttpClient.ts" }
    ],
    "no-console": ["error", { "allow": [] }]
  }
}
```

