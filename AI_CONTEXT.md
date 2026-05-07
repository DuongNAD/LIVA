# 🤖 LIVA System — AI Developer Context & System Guidelines
# Last Updated: 2026-05-05 (P2 Security Hardening — BlueGreenRouter Safe Rollback, SensoryManager Anti-Injection, Geolocation Opt-in) | Maintainer: Dương (System Architect)

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
┌─────────────────────────────────────────────────────────────────────────────┐
│                          LIVA SYSTEM ARCHITECTURE                           │
├──────────────┬──────────────────┬───────────────┬──────────────┬────────────┤
│  Remote Hub  │ liva-ui          │ openclaw-     │ llama-server │ liva-      │
│  (Ingress)   │ (Electron)       │ gateway       │ (C++ Native) │ dashboard  │
│              │                  │ (Node.js/TS)  │ GGUF Runtime │ (Web)      │
│  Telegram ↔  │ Desktop UI       │ Agent Brain   │ GPU Offload  │ Analytics  │
│  CDP Bridge  │ WebSocket ↔      │ FSM + Memory  │ Zero-Python  │            │
│  Approval    │                  │ + Skills      │ CUDA/Vulkan  │            │
└──────────────┴──────────────────┴───────────────┴──────────────┴────────────┘
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
- **Remote Control Hub (Phase 1)**: `openclaw-gateway` acts as the ingress layer handling Telegram long-polling and CDP WebSocket connections to Antigravity IDE. Execution requests go through `SecurityGateway` (Zero-Trust) and `ApprovalEngine` (HITL flow).
- Gateway ↔ Engine communication: gRPC (prod) or OpenAI-compatible HTTP (dev)
- **VirtualManager (Phase 2)**: The 32B manager model is replaced with a zero-VRAM Node.js native router. It performs parallel I/O over LanceDB (episodic) and SQLite (facts) and supports fast-track routing (`chitchat`, `system_command`) with <1ms overhead.
- **LIVA Ngoại Biên (Ingress/Egress Phase)**:
  - Tích hợp `TelegramManager` đẩy notification và hỗ trợ duyệt HITL trực tiếp qua Inline Keyboard.
  - Tích hợp `EmailClientManager` chạy nền giám sát IMAP với cơ chế Sanitization và backoff tự động.
- **Singularity Pipeline (Tự Tiến Hóa)**:
  - Sandbox Docker Zero-Trust cực đoan (`--network none`, `--read-only`, `--pids-limit=64`) chặn Fork-bomb.
  - Thao tác AST bằng `ts-morph` và Atomic Write (thông qua `ASTCodeSurgeon`), loại bỏ sửa file bằng Regex.
  - Hệ thống `GitNexus` chia làm hai: Indexer chạy ngầm (`GitNexusIndexer`) và MCP Tool truy vấn Semantic RAG (`GitNexusQuery`) đảm bảo Zero VRAM Leak.
  - **BlueGreenRouter V8**: Rollback bằng Physical Snapshot (`.src.rollback.bak`) thay vì `git checkout -- src/` + `git clean -fd src/` phá hoại working tree.
  - **SensoryManager Anti-Injection**: Hàm `sanitizeSensoryData()` cắt input 2000 ký tự, strip HTML tag, escape control characters trước khi inject vào LLM prompt.
- **Asynchronous HiGMem (Phase 3)**:
  - **L1 Turn Layer**: Raw conversational turns are persisted directly into `turn_layer_nodes` in `StructuredMemory.sqlite`.
  - **L2 Event Layer**: `ReflectionDaemon` operates fully asynchronously via batched extraction using rigorous Zod Dual Schema (Factual/Relational).
  - **Macro Synthesis**: `ConsolidationCron` processes idle events into L2 `AXIOM` and temporal `ANCHOR` vectors in LanceDB.
- **DevSecOps Self-Healing Pipeline (Phase 4)**:
  - `ModelOrchestrator` implements `startAnomalyDetection` (15s pings) to monitor the LLM backend.
  - If a VRAM hang is detected (3 consecutive failures), the `RollbackManager` auto-kills the zombie process tree and re-warms the AI autonomously.
  - **DevSecOps Security Vault**: `openclaw-gateway/.env` is continuously monitored by `electron.cjs`. Sensitive keys (e.g., `ZALO_OA_ACCESS_TOKEN`, `AI_API_KEY`) are automatically intercepted, encrypted via `electron.safeStorage` into `liva_vault.json`, and removed from plaintext `.env` (Zero-Trust/Shift-Left approach).
- **Frontend Tối Ưu Vue 3 (Trụ cột 4)**:
  - **Reactivity System bypass:** Sử dụng `shallowRef` + `triggerRef` thay vì `ref` cho luồng stream để tránh Event Loop blocking.
  - **Chống Zombie RAM:** Polling timers bên trong Vue components bị cache bởi `<KeepAlive>` bắt buộc phải dùng `onActivated` và `onDeactivated`.
  - **Telemetry Observability:** SystemView captures and displays real-time health-check logs and process anomaly reports emitted directly from the `CoreKernel` to isolate backend failures.
  - **Mobile-Responsive Design:** Implement responsive CSS patterns (e.g. converting Sidebar to Bottom Navigation Tab bar via `@media` max-width 768px) to prepare for future tablet/mobile expansion.
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
| Database | `node:sqlite` (built-in) | Used in StructuredMemory. **Ghi chú:** Bắt buộc áp dụng *Debounced Writes pattern* để tránh block Main Thread. |
| Vector DB | `@lancedb/lancedb` | Used in TurboQuantStore, LanceMemory |
| Embeddings | `@huggingface/transformers` v4 | **Via `EmbeddingService` singleton ONLY** (see §5) |
| Browser | `playwright-core` | API-only, no bundled browsers |
| Search | `flexsearch` | Document indexing in HeraCompass |
| Logger | `pino` + `pino-pretty` | Async worker thread, structured JSON |
| Testing | `vitest` (TS), `pytest` (Python) | `vi.stubGlobal('fetch')` for mocking |
| Validation | `zod` v4+ | Schema validation — use `.issues` not `.errors` on `ZodError` |
| Caching | `lru-cache` | Bounded eviction (Use default export: `import LRUCache from 'lru-cache'`) |
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
| ❌ `sqlite3` / `sqlite` | Native compilation causes ABI mismatch & bloat | Native `node:sqlite` (built-in) |
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
catch (e: unknown) {
  // Native fetch buries the real error in e.cause
  const errMsg = e instanceof Error ? ((e as any).cause?.message || e.message) : String(e);
}
```

</CRITICAL_DIRECTIVE>

### 4.2. Timer & Memory Leak Prevention

<CRITICAL_DIRECTIVE>

> **All long-running or delayed background tasks MUST use true private fields (`#`)**
> ECMAScript `#` ensures strict encapsulation, preventing zombie state modifications from outside the class. (e.g. `ReflectionDaemon.ts`)

</CRITICAL_DIRECTIVE>

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

### 4.8. Frontend Vue 3 Reactivity & Zombie RAM

<CRITICAL_DIRECTIVE>

> **Streaming AI Output MUST use `shallowRef`!**
> Using `ref` on an array that receives 60 tokens/second causes Vue's deep reactivity proxy to traverse the entire object tree constantly, blocking the main thread and freezing 3D engines (VRM/Live2D).

</CRITICAL_DIRECTIVE>

```typescript
// ✅ CORRECT — Bypass deep reactivity, trigger manually
const messages = shallowRef<{ role: string; text: string }[]>([]);
messages.value[messages.value.length - 1].text += chunk;
triggerRef(messages);

// ❌ WRONG — Freezes the UI on rapid updates
const messages = ref<{ role: string; text: string }[]>([]);
messages.value[messages.value.length - 1].text += chunk;
```

**KeepAlive Lifecycle:** Component bị cache bằng `<KeepAlive>` (ví dụ `SystemView.vue`) tuyệt đối không được thiết lập global `setInterval`. Bắt buộc phải start/stop timer trong `onActivated` và `onDeactivated` để ngăn chặn rò rỉ RAM (Zombie RAM).

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
│   ├── PromptBuilder.ts     # System prompt assembly (route-aware 4-tier memory injection + L2 semantic + HeraCompass ICL)
│   ├── ZaloPolling.ts       # Inbound message listener (long-polling)
│   ├── TelemetryProfiler.ts # Performance metrics (debounced write)
│   ├── ASTActuator.ts       # Code modification via AST
│   ├── ASTHealer.ts         # Auto-fix broken code
│   └── UIController.ts      # WebSocket bridge to Electron (token auth)
│
├── memory/                  # 💾 Persistence Layer (LIVA-UHM)
│   ├── EncryptionEngine.ts  # Centralized AES-256-GCM + Atomic Write protocol
│   ├── RamCacheManager.ts   # Bounded FIFO message cache + GDPR purge
│   ├── StructuredMemory.ts  # L1: Key-value facts + Event bricks (node:sqlite, TTL, FIFO)
│   ├── TurboQuantStore.ts   # L0: Quantized vector memory (4-bit KV cache)
│   ├── LanceMemory.ts       # L2: Semantic RAG + consolidated narratives (@lancedb)
│   ├── SemanticRouter.ts    # 🧠 Intent router (cosine similarity, <100ms, 5 routes incl. tool_recall, adaptive threshold)
│   ├── ReflectionDaemon.ts  # 🔄 Dual-Perspective Φ/Ψ extraction (debounced 12s)
│   ├── ConsolidationCron.ts # 💤 Sleep-time consolidation (idle 30min + manual)
│   ├── HeraCompass.ts       # Error insight DB (flexsearch, utility scoring)
│   ├── PersonalKnowledgeExtractor.ts  # Auto-extract user preferences
│   └── SensoryManager.ts    # Multi-modal input aggregation (TTL + GC)
│
├── skills/                  # 🔧 MCP Tools (Domain-driven architecture)
│   ├── agentic/             # AI scientist, code gen, hypothesis, planning
│   ├── core/                # File I/O, execute commands, GitNexus query
│   ├── data/                # Data extraction, markdown, PDF, vision
│   ├── devops/              # Docker, deployment, system metrics
│   ├── docs/                # Report generation, search, writing
│   ├── personal/            # Email, calendar, notes, preferences
│   ├── social/              # Telegram, Zalo, Messenger, Slack
│   └── web/                 # Browser automation, web search, scraping
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
│   ├── MicroVMDaemon.ts     # LocalSandbox with filesystem deny list
│   └── DockerEnvManager.ts  # Ephemeral Docker container với Hardened Zero-Trust profile
│
├── evolution/               # 🧬 Singularity Pipeline (DAG)
│   ├── EvolutionPipeline.ts # Main Orchestrator
│   ├── EngineManager.ts     # Safe VRAM & Hot-swap
│   ├── ASTMutator.ts        # Direct AST surgery (No SkillRegistry)
│   ├── ASTCodeSurgeon.ts    # Phẫu thuật AST với Path Jail và Atomic Write
│   ├── GitNexusIndexer.ts   # Daemon chạy gitnexus analyze ngầm
│   └── RollbackManager.ts   # Safe rollback on failure
│
├── services/                # 🎤 Peripheral Services
│   ├── EmailClientManager.ts# IMAP Daemon lắng nghe email
│   ├── TelegramManager.ts   # Tương tác Telegram Bot API
│   ├── EmbeddingService.ts  # ⭐ Singleton embedding (Promise Lock, all-MiniLM-L6-v2)
│   ├── VoiceEngine.ts       # TTS token streaming
│   ├── KokoroVoiceEngine.ts # Kokoro-JS ONNX TTS (local-first)
│   ├── WhisperNode.ts       # Speech-to-text (safeFetch to Whisper API)
│   └── WhisperJSNode.ts     # Pure JS Whisper (ONNX fallback)
│
├── utils/                   # 🔨 Shared Utilities
│   ├── HttpClient.ts        # ⭐ safeFetch() + withSafeTimeout() — THE fetch/timeout wrappers
│   ├── PlaywrightBrowser.ts # Browser singleton factory (auto-detect Chrome/Edge)
│   ├── logger.ts            # Pino async logger
│   ├── ZaloNotifier.ts      # Fire-and-forget Zalo notifications
│   ├── LivaEngine.ts        # LLM client factory (SecureLivaEngine + Seal Token)
│   ├── NativeIPCClient.ts   # gRPC client to Python engine (GRPCStream async iter)
│   ├── JsonExtractor.ts     # ⭐ safeExtractJSON() — centralized LLM JSON extraction (jsonrepair)
│   ├── VectorMath.ts        # ⭐ cosineSimilarity/F32() — shared vector ops (SIMD-like unrolling)
│   └── DockerSandbox.ts     # Ephemeral Docker container management
│
├── auto_singularity.ts      # 🧬 Entrypoint for EvolutionPipeline (Refactored to DAG)
├── Gateway.ts               # Entry point
├── SkillRegistry.ts         # Dynamic skill loader + MCP fallback
├── MemoryManager.ts         # Memory Facade orchestrator (delegates to Encryption/Cache/Lance)
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
- **Zombie Timer on Recursive setTimeout**: Store the timer ref (`this.#reconnectTimer = setTimeout(fn, ms)`) and `clearTimeout` it before reassignment AND in `stop()`/`dispose()`. Use true private `#field` to prevent external zombie modifications. (Fixed: TelegramBridge, EmailClientManager, useGateway, 2026-05-05)

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
- **Race Timeout in Promise.race**: NEVER use `Promise.race([task, new Promise(setTimeout)])`. The timeout's `setTimeout` leaks on every successful task. Use `withSafeTimeout(promise, ms, label)` from `HttpClient.ts` instead — it clears the timer in `.finally()`. (Fixed: PromptBuilder, WebResearchAgent, 2026-05-05)

### Security Hardening
- **Duplicate Encryption**: Tuyệt đối không copy/paste logic mã hóa giữa các file. Bắt buộc phải import và dùng chung `EncryptionEngine` để tránh mất đồng bộ key và lộ secret.
- **Destructive Git Rollback**: NEVER use `git checkout -- src/` or `git clean -fd src/` in rollback logic. These commands nuke ALL uncommitted work in the entire `src/` tree. Use physical folder snapshot (`.src.rollback.bak`) via `fs.cpSync` instead. (Fixed: BlueGreenRouter V8, 2026-05-05)
- **Unsanitized External Data in LLM Prompts**: NEVER inject clipboard/window title data directly into system prompts. Always run through `sanitizeSensoryData()` (max 2000 chars, HTML strip, control char escape). Attacker can manipulate LLM via clipboard poisoning. (Fixed: SensoryManager, 2026-05-05)
- **Auto-leaking IP Geolocation**: NEVER call external IP lookup APIs unconditionally on boot. Geolocation must be OPT-IN via `LIVA_GEOLOCATION_ENABLED=true`. (Fixed: CoreKernel, 2026-05-05)

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
# Security
LIVA_ENCRYPTION_KEY=   # [BẮT BUỘC] Chuỗi 32 bytes AES-256 dùng để vận hành EncryptionEngine
LIVA_KERNEL_SECRET=    # [TÙY CHỌN] Chuỗi dự phòng (fallback UUID) dùng cho hệ thống kernel internal

# AI Provider: "local" (GGUF via llama-server) or "cloud" (OpenAI-compatible API)
AI_PROVIDER=local
AI_BASE_URL=           # Cloud API endpoint (only when AI_PROVIDER=cloud)
AI_API_KEY=            # Cloud API key
AI_MODEL=              # Cloud model name
AI_MODELS_DIR=         # Local model directory (default: ~/.liva/models)
ROUTER_MODEL_NAME=     # Light model for routing (default: gemma-4-E4B-it-Q4_K_M.gguf)
EXPERT_MODEL_NAME=     # Heavy model for deep tasks

# Integrations
ZALO_OA_ACCESS_TOKEN=  # Zalo Bot Creator token (contains ":")
ZALO_USER_ID=          # Auto-detected on first message
TAVILY_API_KEY=        # Web search (free 1000/month, falls back to DDG)
LIVA_GEOLOCATION_ENABLED= # "true" to enable IP geolocation lookup on boot (opt-in, default OFF)
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
- Current baseline: **109 test files, 1150+ tests** (Updated 2026-04-30)

**Test File Map:**
```
tests/
├── core/
│   ├── AgentLoop.test.ts          # Sub-agents: CKA, DualPort, TEO, LTC, TaskLane
│   ├── ASTActuator.test.ts        # AST mutations, source transforms
│   ├── ASTHealer.test.ts          # Self-healing code patches
│   ├── ApprovalEngine.test.ts     # Multi-step approval workflows
│   ├── CoreKernel.test.ts         # Full bootstrap, peripherals, shutdown
│   ├── CoreKernelAuthority.test.ts # Token issuance, phase verification
│   ├── DualPortController.test.ts # Expert model swap, VRAM management
│   ├── HeartbeatManager.test.ts   # Interval start/stop, heartbeat trigger
│   ├── IsolatedAgentTurn.test.ts  # Background turn, XML tool parsing
│   ├── LTCOrchestrator.test.ts    # Long-term concept extraction
│   ├── ModelOrchestrator.test.ts  # TaskToken, health check, VRAM
│   ├── NLCommandTranslator.test.ts # Natural language → command mapping
│   ├── NativeIPCClient.test.ts    # gRPC unary + streaming + health
│   ├── PromptBuilder.test.ts      # Context assembly, tool RAG, skill filter
│   ├── SessionOrchestrator.test.ts # Session lifecycle, state persistence
│   ├── StressTest.test.ts         # High-load concurrency testing
│   ├── TaskQueue.test.ts          # Sequential processing, singleton
│   ├── TelemetryProfiler.test.ts  # Perf tracking, timing accuracy
│   ├── ToolExecutionOrchestrator.test.ts # Reflection, loop prevention
│   ├── UIController.test.ts       # WebSocket pool, broadcast, config SSOT
│   ├── VirtualManager.test.ts     # Zero-VRAM context orchestration
│   └── ZaloPolling.test.ts        # Token validation, message emit, offset
├── bridges/
│   ├── CDPBridge.test.ts          # Chrome DevTools Protocol bridge
│   └── VSCodeBridge.test.ts       # VS Code extension bridge
├── memory/
│   ├── ConsolidationCron.test.ts  # Sleep-time consolidation, sessions, L2+L3
│   ├── HeraCompass.test.ts        # RAG insight, utility score, GC
│   ├── LanceMemory.test.ts        # LanceDB vector add, search, connect
│   ├── PersonalKnowledgeExtractor.test.ts # Fact extraction, JSON safety
│   ├── ReflectionDaemon.test.ts   # Debounced Φ/Ψ extraction, batch, flush
│   ├── SemanticRouter.test.ts     # Route classification, fallback, confidence
│   ├── SensoryManager.test.ts     # Capture, TTL, prompt injection
│   ├── StructuredMemory.test.ts   # SQLite CRUD, TTL, eviction, events table
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
├── skills/                         # 30 skill test files (domain-organized)
│   ├── AIScientist.test.ts        # Research agent skill
│   ├── AppendGoogleDoc.test.ts    # Google Docs integration
│   ├── BrowserHarness.test.ts     # Browser automation
│   ├── CheckImportantEmailsToday.test.ts  # Email priority filtering
│   ├── CreateGoogleDoc.test.ts    # Document creation
│   ├── DeleteLocalFile.test.ts    # Path guardrails, boot file protection
│   ├── ExecuteCommand.test.ts     # Whitelist security, HITL approval
│   ├── GetSystemInfo.test.ts      # OS/CPU/RAM info retrieval
│   ├── GetWeather.test.ts         # Weather API integration
│   ├── HashChecksum.test.ts       # Stream-based file integrity (MD5/SHA256)
│   ├── JsonYamlConverter.test.ts  # Bidirectional format conversion
│   ├── ListDirectory.test.ts      # Directory listing, error handling
│   ├── ProcessManager.test.ts     # Process monitoring with HITL guard
│   ├── ReadLocalFile.test.ts      # File read, Unicode, error cases
│   ├── ScreenshotCapture.test.ts  # Desktop capture via PowerShell/.NET
│   ├── SendZaloBot.test.ts        # Zalo bot API integration
│   ├── UpdateMemory.test.ts       # Category routing, TTL
│   ├── WebSearch.test.ts          # Tavily + DDG fallback, error paths
│   ├── WriteLocalFile.test.ts     # Atomic write, path guardrails
│   └── ... (30 total)
├── evolution/
│   ├── ASTCodeSurgeon.test.ts     # AST surgical code modifications
│   └── GitNexusIndexer.test.ts    # Code graph indexing
├── utils/
│   ├── HttpClient.test.ts         # safeFetch, timeout, 4xx/5xx
│   └── ZaloNotifier.test.ts       # Bot Creator vs OA API, fire-and-forget
├── mcp/
│   └── MCPClientManager.test.ts   # Singleton, method surface
└── SkillRegistry.test.ts          # Built-in skills, MCP fallback, semantic topK
```

---

## 9. 📋 Commands Quick Reference

```bash
# Development
npx tsx src/Gateway.ts          # Start gateway (dev CLI — electron.cjs uses local binary)
npx vitest run                  # Run tests
npx vitest watch                # Watch mode

# Self-Evolution (AI research pipeline)
cross-env NODE_OPTIONS="--expose-gc --max-old-space-size=8192" npx tsx src/auto_singularity.ts

# Full system startup (Windows)
start_all.bat                   # Starts: Engine → Voice → Gateway → UI

# GitNexus (code intelligence)
# NOTE: electron.cjs & GitNexusIndexer resolve binary locally (node_modules/.bin/gitnexus)
# --embeddings is OPT-IN only; boot-time indexing skips it to avoid blocking startup
npx gitnexus analyze            # Rebuild code graph (CLI shorthand)
npx gitnexus analyze --embeddings  # With semantic embeddings (heavy, opt-in)
```

---

## 10. 🗺️ Key Data Flows

### User Message → AI Response
```
User Input (Electron WebSocket)
  → UIController.ts
  → AgentLoop.ts (FSM: IDLE → THINKING)
  → SemanticRouter.route() — intent classification (<100ms)
  → PromptBuilder.ts (route-aware context + token budget)
     chitchat → minimal (profile only)
     system_command → skip RAG (profile + sensory)
     factual_recall/deep_reasoning → full L1+L2+L3 pipeline
  → ModelOrchestrator.ts (Router:8000 or Expert:8001)
  → LLM generates response + optional tool calls
  → SkillRegistry.ts → skill.execute()
  → ZMAS_Guard.ts (filter output)
  → ReflectionDaemon.queueTurn() — debounced Φ/Ψ extraction
  → AgentLoop.ts (REFLECTING → IDLE)
  → UIController.ts → Electron
```

### Memory Architecture (LIVA-UHM — 4-tier Hierarchical)
```
L0: TurboQuantStore (VRAM)    — Working memory, quantized KV cache
L1: StructuredMemory (SQLite) — Event bricks (Φ Factual + Ψ Relational) + KV facts
L2: LanceMemory (LanceDB)     — Consolidated narratives, semantic vector search
L3: PersonalKnowledge (KV)    — Core insights, user preferences, strategic memory

SemanticRouter → routes queries to appropriate tier (<100ms, cosine similarity)
ReflectionDaemon → extracts Φ/Ψ after each turn (debounced 12s micro-batch)
ConsolidationCron → synthesizes L1→L2+L3 (idle 30min / manual / cold-start)
```

### Error Self-Healing
```
Tool fails → HeraCompass.learnFromError() → LLM generates rule
Next attempt → HeraCompass.getRelatedInsight() → Injects past lesson
Success/Failure → updateUtilityScore() → Verified or Garbage-Collected
```

---

## 11. 🔒 Shutdown Chain (`CoreKernel.shutdown()`)

Every resource with cleanup requirements is called in order via **asynchronous execution** to guarantee database writes:

```typescript
async CoreKernel.shutdown()
  ├── clearInterval(gcIntervalId)     // Own GC timer
  ├── fileWatcher.close()             // FSWatcher file handles
  ├── zalo.stop()                     // ZaloPolling timer
  ├── voiceEngine.destroy()           // TTS timers/buffers
  ├── whisperNode.destroy()           // STT model + listeners
  ├── memory.dispose()                // [LIVA-UHM] flushPending → ReflectionDaemon
  │   ├── reflectionDaemon.flushPending() // Flush pending Φ/Ψ extractions
  │   ├── reflectionDaemon.dispose()      // Clear debounce timer
  │   ├── consolidationCron.dispose()     // Clear idle-check interval
  │   ├── quantStore.dispose()            // QuantStore GC + tensor cache
  │   └── structuredMemory.close()        // SQLite connection
  ├── SensoryManager.dispose()        // 5s GC interval
  ├── EmbeddingService.dispose()      // 140MB ONNX model
  ├── emailManager.dispose()          // Dừng IMAP timer và ngắt kết nối
  ├── voiceSpeaker.dispose()          // Dọn dẹp tiến trình ngầm phát âm thanh (PowerShell TTS)
  └── gitNexusIndexer.dispose()       // Dừng Background Indexer debounce timer
```

> [!IMPORTANT]
> The `Gateway.ts` handles graceful shutdown asynchronously. It strictly blocks the exit (`process.exit(0)`) for **1.5 seconds** after calling `await kernel.shutdown()` to ensure the SQLite Write-Behind Cache (WAL) has enough time to flush to disk safely, preventing data loss.
> When adding a new service with timers, intervals, or ML models, you **MUST** add its cleanup call here. This is enforced by Write Protocol.

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

