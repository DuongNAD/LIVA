# 🤖 LIVA System — AI Developer Context & System Guidelines
# Last Updated: 2026-05-19 (v26 Enterprise-Ready Cognitive OS — Decoupled Embedding, Zero-Leak Guard) | Maintainer: Dương (System Architect)
#
#> [!IMPORTANT]
#> **ARCHITECTURE NOTE (2026-05-17):**
#> Migration từ Electron sang Tauri v2 **ĐÃ HOÀN TẤT**.
#> - `electron.cjs`, `preload.cjs`, `ElectronAdapter.ts` đã bị xóa.
#> - Toàn bộ UI components đã chuyển sang Tauri API / Gateway WebSocket.
#> - Transparent Widget Window (Ghost Mode) hoạt động trên Tauri v2 Windows.
#> - Secure Credential Vault dùng `EncryptionEngine` (AES-256-GCM), không còn phụ thuộc `electron.safeStorage`.

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

**LIVA** is a **hybrid-intelligence, multi-agent AI desktop assistant** designed for **mid-to-high-end hardware** on **Windows & macOS**. It dynamically routes between local AI inference (GPU) and cloud APIs to maximize performance and hardware efficiency.

### Design Philosophy
- **NOT 100% local-only** — LIVA uses a **Hybrid Intelligence** approach: local AI when hardware allows, cloud fallback when performance demands it.
- **Cross-Platform** — Targets Windows 10/11 and macOS (Apple Silicon & Intel). Platform-specific code uses `process.platform` guards.
- **Hardware-Adaptive** — `AutoGPUSetup` detects GPU VRAM, RAM, and CPU cores at boot to auto-configure model size, context length, and thread count.
- **Lean Footprint** — Gateway < 100MB RAM. UI uses Tauri v2 for Widget/Tray features.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                     LIVA HYBRID SYSTEM ARCHITECTURE                         │
├──────────────┬──────────────────┬───────────────┬──────────────┬────────────┤
│  Remote Hub  │ liva-ui          │ openclaw-     │ AI Engine    │ TTS System │
│  (Ingress)   │ (Tauri v2/Rust)  │ gateway       │ (Adaptive)   │ (Hybrid)   │
│              │ OS WebView Native│ (Node.js/TS)  │              │            │
│  Telegram ↔  │ 30MB-50MB RAM    │ Agent Brain   │ Local: GGUF  │ Edge-TTS + │
│  CDP Bridge  │ WebSocket ↔      │ FSM + SQLite  │ Cloud: API   │ Kokoro-JS  │
│              │ Win + macOS      │ 77 Skills     │ Auto-Switch  │ Fallback   │
└──────────────┴──────────────────┴───────────────┴──────────────┴────────────┘
```

### Startup Sequence (`npm run desktop` / Tauri Sidecar)
1. `openclaw-gateway` → `tsx src/Gateway.ts` → `AutoGPUSetup` (hardware detection: VRAM, RAM, CPU cores)
2. `openclaw-gateway` → `ModelOrchestrator` → **Adaptive Engine Selection:**
   - **Local Mode** (`AI_PROVIDER=local`): Spawns `llama-server` (C++ native, port 8000, auto-selects GPU layers based on VRAM)
   - **Cloud Mode** (`AI_PROVIDER=cloud`): Connects to OpenAI-compatible API (Gemini, GPT, Claude, Groq, etc.)
   - **Hybrid Mode** (`AI_PROVIDER=hybrid`): Local for chat, cloud for complex reasoning
3. `liva-ai-engine` → `voice_engine.py` (Edge-TTS, optional — skipped on macOS if `ffmpeg` unavailable)
4. `liva-ui` → Tauri v2 (desktop app, connects via WebSocket to port 8082). Transparent widget + System tray.

### Platform Support
| Platform | Status | Notes |
|----------|--------|-------|
| Windows 10/11 (x64) | ✅ Primary | Full feature support, CUDA/Vulkan GPU |
| macOS (Apple Silicon) | ✅ Supported | Metal GPU, `llama-server` via Homebrew |
| macOS (Intel) | ⚠️ Limited | CPU-only inference, cloud mode recommended |
| Linux (x64) | 🔜 Planned | Community testing phase |

### Architectural Boundaries (STRICT)
- `/core` layer NEVER calls database directly — must go through `/memory`
- `/skills` are self-contained MCP tools — each exports `metadata` + `execute()`
- `/security` guards are applied at the AgentLoop level, not per-skill
- **Remote Control Hub (Phase 1)**: `openclaw-gateway` acts as the ingress layer handling Telegram long-polling and CDP WebSocket connections to Antigravity IDE. Execution requests go through `SecurityGateway` (Zero-Trust) and `ApprovalEngine` (HITL flow).
- Gateway ↔ Engine communication: gRPC (prod) or OpenAI-compatible HTTP (dev/cloud)
- **Adaptive AI Engine**: `ModelOrchestrator` supports 3 modes — local (GGUF), cloud (API), and hybrid. Hardware detection at boot auto-configures the optimal mode based on available GPU VRAM and RAM.
- **Consolidated Brain**: Xóa bỏ `LanceDB` và `flexsearch`. Gom tất cả về 1 file `node:sqlite` duy nhất dùng C-Extension `sqlite-vec` và `FTS5`.
- **LIVA Ngoại Biên (Ingress/Egress Phase)**:
  - Tích hợp `TelegramManager` đẩy notification và hỗ trợ duyệt HITL trực tiếp qua Inline Keyboard.
  - Tích hợp `EmailClientManager` chạy nền giám sát IMAP với cơ chế Sanitization và backoff tự động.
- **Singularity Pipeline (Tự Tiến Hóa)**:
  - **MicroVMDaemon**: Thay thế Sandbox Docker/WSL2. Dựa trên `isolated-vm` hoặc WASI khởi động <1ms, RAM <15MB.
  - Thao tác AST bằng `ts-morph` và Atomic Write (thông qua `ASTCodeSurgeon`), loại bỏ sửa file bằng Regex.
  - Hệ thống `GitNexus` chia làm hai: Indexer chạy ngầm (`GitNexusIndexer`) và MCP Tool truy vấn Semantic RAG (`GitNexusQuery`) đảm bảo Zero VRAM Leak.
  - **BlueGreenRouter V8**: Rollback bằng Physical Snapshot (`.src.rollback.bak`) thay vì `git checkout -- src/` + `git clean -fd src/` phá hoại working tree.
  - **SensoryManager Anti-Injection**: Hàm `sanitizeSensoryData()` cắt input 2000 ký tự, strip HTML tag, escape control characters trước khi inject vào LLM prompt.
- **Worker Threads**: Bất kỳ tác vụ tốn >10ms CPU-time (AST ts-morph, parse JSON lớn, **Neural VAD inference**) BẮT BUỘC đẩy sang `node:worker_threads`. VAD Silero ONNX runs in `VADWorker.ts` — NEVER on main thread.
- **Asynchronous HiGMem (Phase 3)**:
  - **L1 Turn Layer**: Raw conversational turns are persisted directly into `turn_layer_nodes` in `StructuredMemory.sqlite`.
  - **L2 Event Layer**: `ReflectionDaemon` operates fully asynchronously via batched extraction using rigorous Zod Dual Schema (Factual/Relational).
  - **Macro Synthesis**: `ConsolidationCron` processes idle events into L2 `AXIOM` and temporal `ANCHOR` vectors in SQLite (`sqlite-vec`).
- **DevSecOps Self-Healing Pipeline (Phase 4)**:
  - `ModelOrchestrator` implements `startAnomalyDetection` (15s pings) to monitor the LLM backend.
  - If a VRAM hang is detected (3 consecutive failures), the `RollbackManager` auto-kills the zombie process tree and re-warms the AI autonomously.
  - **DevSecOps Security Vault**: `openclaw-gateway/.env` is continuously monitored by the Tauri host process. Sensitive keys (e.g., `ZALO_OA_ACCESS_TOKEN`, `AI_API_KEY`) are automatically intercepted, encrypted via Node.js `node:crypto` (AES-256-GCM) into `liva_vault.json`, and removed from plaintext `.env` (Zero-Trust/Shift-Left approach).
- **Frontend Tối Ưu Vue 3 (Trụ cột 4)**:
  - **Reactivity System bypass:** Sử dụng `shallowRef` + `triggerRef` thay vì `ref` cho luồng stream để tránh Event Loop blocking.
  - **Chống Zombie RAM:** Polling timers bên trong Vue components bị cache bởi `<KeepAlive>` bắt buộc phải dùng `onActivated` và `onDeactivated`.
  - **Telemetry Observability:** SystemView captures and displays real-time health-check logs and process anomaly reports emitted directly from the `CoreKernel` to isolate backend failures.
  - **Mobile-Responsive Design:** Implement responsive CSS patterns (e.g. converting Sidebar to Bottom Navigation Tab bar via `@media` max-width 768px) to prepare for future tablet/mobile expansion.
- **Tauri Sidecar Giao Tiếp**: Gateway chạy nền (Daemon) và giao tiếp với Tauri UI qua kiến trúc **Dynamic WS Handshake**. 
  - `console.log` đã bị khoá (`stdout` Guard) để chỉ in ra đúng 1 dòng JSON `{event: "GATEWAY_READY", port: <dynamic>, token: <uuid>}`.
  - **TUYỆT ĐỐI KHÔNG IN RA STDOUT**. Mọi log khác phải dùng `logger` (Pino) trỏ qua `stderr`.
- **v24 Ambient Cognitive OS — 4 Hardware Optimization Pillars:**
  - **Pillar 1: Preemptive VRAM Yielding** — `AppWatcherService` upgraded to `VRAMGuard`. Monitors GPU load (via `nvidia-smi`) and heavy app detection (game/render whitelist). When detected → `CoreKernel.yieldVRAM()` kills llama-server, routes all AI traffic to Cloud API. On game exit → `CoreKernel.reclaimVRAM()` re-warms local model.
  - **Pillar 2: Semantic Action Cache L0.5** — `SemanticRouter` embeds a persistent action cache in SQLite. Caches `[query_vector] → [tool_name, tool_args]` pairs. Cosine similarity > 0.95 bypasses LLM entirely → direct SkillRegistry execution (< 5ms). Eviction: LRU with max 200 entries.
  - **Pillar 3: On-Demand Screen Awareness** — SemanticRouter detects deictic keywords ("this", "trên màn hình", "đoạn code này"). Triggers Tauri `screenshot` command → single WebP frame → Cloud Vision API. No continuous screen streaming. Zero local VLM cost.
  - **Pillar 4: Wake-Word Edge Offloading** — [v25 IMPLEMENTED] Frontend ONNX WASM wake-word model (~5KB, hey_liva.onnx). Mic always-on at Frontend but audio data NEVER sent to Backend until wake-word detected via ONNX inference. Global Hotkey (Alt+Space) as fallback. Backend CPU/GPU usage = 0% during silence. Zero external dependencies (Picovoice-free).
- **⚠️ v24 CRITICAL GUARD: VRAM ↔ Cache L0.5 Interlock** — `SemanticActionCache` MUST check `VRAMGuard.isYielded` before calling `EmbeddingService.embedWithTimeout()`. Both share the same `llama-server` process — if VRAM is yielded (llama killed), embedding will timeout/fail. The `SemanticRouter.setVramGuardCheck()` method injects this dependency via CoreKernel bootstrap.
- **[v25 Autonomous Ecosystem — Hardware & UX Maximization] (ROADMAP):**
  - **Pillar 1: Energy-Aware Eco Mode** — `PowerMonitorService` reads OS battery API. When unplugged OR battery < 30%: Gateway sets `ECO_MODE` flag → freezes `ProactiveDaemon` (stops background scraping), forces `VRAMGuard.yield()` (kills local LLM), routes 100% traffic to Cloud API (Groq/Gemini), reduces UI avatar FPS to 5. LIVA proactively announces: "Sếp vừa rút sạc, em đã tự động vào chế độ siêu tiết kiệm pin nhé."
  - **Pillar 2: Seamless Local↔Cloud Handoff (StateSynchronizer)** — When `yield_vram` or `reclaim_vram` fires, `MemoryManager` snapshots `WorkingBuffer` (L0 RAM) and compresses into a message history summary. The new engine receives this snapshot injected into the first prompt. Brain swap between Local ↔ Cloud is invisible — LIVA continues mid-conversation without context loss.
  - **Pillar 3: Spatial Cross-Device Handoff (PresenceDetector)** — Monitors OS idle time (mouse/keyboard). If idle > 3 minutes → `PRESENCE = AWAY`. All Desktop output (TTS, UI Toast) is muted. LIVA auto-reroutes responses to `TelegramManager` → sends text to user's phone. When user returns (mouse move), `PRESENCE = ACTIVE` → resumes Desktop output seamlessly.
  - **Pillar 4: Zero-Trust Deictic Vision** — SemanticRouter only activates screen capture when deictic keywords detected ("cái này", "trên màn hình", "đoạn code này"). Hard HITL Guard: LIVA MUST ask "Em chụp màn hình hiện tại nhé?" before capturing. Local Redaction: Tauri captures 1 frame → lightweight local algorithm blurs password fields/credit card inputs → compressed WebP → Cloud Vision API. 0% local VRAM, 100% data safety.
- **[v26 Enterprise-Ready Stability — Core Architecture Upgrade]:**
  - **Decoupled CPU Embedding:** Semantic embeddings are no longer dependent on the GPU-constrained `llama-server`. `EmbeddingService` now relies exclusively on an isolated `onnxruntime-node` CPU worker (`EmbeddingWorker.ts`). This guarantees zero VRAM overhead and ensures memory storage works independently of the main LLM.
  - **Event Loop Protection (Async I/O):** Synchronous file system calls (e.g., `fs.readFileSync`) are STRICTLY BANNED in the main Gateway event loop (e.g., `CoreKernel.ts`, `VoiceEngine.ts`). All configuration loaders must use `fs.promises.readFile`.
  - **Zero-Leak Memory Guards:** Unbounded `Map` caches (like `taskPlanHistories`) MUST be replaced with `LRUCache` to prevent memory bloat over time. Raw `Promise.race` usage for background timeouts is dangerous and strictly prohibited; you MUST use the leak-free `withSafeTimeout` utility to prevent zombie timers.
  - **Single Source of Truth (SSOT):** Configuration states and profiles (like `user_profile.json`) must be synchronized across UI and Gateway controllers, eliminating race conditions during websocket handshakes.

---

## 3. 🚫 Tech Stack — Allowed vs Banned

### ✅ ALLOWED (Use ONLY these)

| Category | Technology | Notes |
|----------|-----------|-------|
| Runtime | Node.js v22+ | **MUST** use ESM (`"type": "module"` in package.json) |
| Language | TypeScript 5.x (strict) | Python optional (voice_engine only) |
| UI Framework | Tauri v2 (Rust host + WebView) | Transparent Widget + System Tray + Stronghold Vault |
| LLM Runtime | `llama-server` (C++) or Cloud API | Local: GGUF models (CUDA/Metal/Vulkan), Cloud: OpenAI-compatible |
| Network | Native `fetch` via `safeFetch()` | Wrapper at `src/utils/HttpClient.ts` |
| Database | `node:sqlite` (built-in) + `sqlite-vec` + `FTS5` | One file, vector & full-text search. Bắt buộc *Debounced Writes*. |
| Processing | `node:worker_threads` | Offload CPU-heavy tasks (>10ms) |
| Sandbox | `isolated-vm` / WASI | MicroVMDaemon (<1ms boot, <15MB RAM) |
| Browser | `playwright-core` | API-only, no bundled browsers |
| Logger | `pino` + `pino-pretty` | Async worker thread, structured JSON |
| Testing | `vitest` (TS), `pytest` (Python) | `vi.stubGlobal('fetch')` for mocking |
| Validation | `zod` v4+ | Schema validation — use `.issues` not `.errors` on `ZodError` |
| Caching | `lru-cache` | Bounded eviction (Use default export: `import LRUCache from 'lru-cache'`) |
| LLM Client | `openai` SDK | Compatible with local & cloud endpoints |

### ❌ BANNED (NEVER USE — these were deliberately removed)

| Library | Reason | Replacement |
|---------|--------|-------------|
| ❌ `Docker / WSL2` | vmmem tốn 2-4GB RAM | `isolated-vm` / WASI |
| ❌ `Dual-Port LLM` | Kiến trúc cũ gây OOM crash | `Single Expert Model` (100% VRAM) |
| ❌ `@huggingface/transformers` | Chạy Tensor CPU làm đơ Event Loop | Gọi API `/v1/embeddings` GPU |
| ❌ `@lancedb/lancedb` & `flexsearch` | Phình DB, removed in v19 | `sqlite-vec` + `FTS5` (fully migrated) |
| ❌ `fs.cpSync` | Khóa cứng Event Loop | Dùng async `fs.promises.cp` |
| ❌ `axios` | Removed in Phase 3 hardening | `safeFetch()` from `src/utils/HttpClient.ts` |
| ❌ `puppeteer` | ABI crash with Electron | `playwright-core` (2MB, API only) |
| ❌ `fuse.js` | O(N) per search, memory hog | `FTS5` (SQLite) |
| ❌ `@xenova/transformers` | Deprecated, unmaintained | API `/v1/embeddings` GPU |
| ❌ `request` / `got` / `node-fetch` | Redundant with native fetch | `safeFetch()` |
| ❌ `console.log` / `console.error` | No structure, blocks event loop | `logger.info()` / `logger.error()` from pino |
| ❌ `fs.readFileSync` / `fs.writeFileSync` | Blocking I/O on main thread | `fs.promises.*` or pino async transport |
| ❌ `sqlite3` / `sqlite` | Native compilation causes ABI mismatch & bloat | Native `node:sqlite` (built-in) |
| ❌ `__dirname` / `__filename` | Not available in ESM | `import.meta.dirname` / `import.meta.filename` |
| ❌ `Web Speech API` (`SpeechRecognition`) | Sends audio to Google Cloud silently, crashes on Tauri | TensorFlow.js Teachable Machine (WASM) |
| ❌ `fs.promises.cp` on running SQLite | WAL corruption — OS page cache unflushed | `VACUUM INTO` for atomic snapshot backup |

---

## 4. 📜 Coding Standards

<CRITICAL_DIRECTIVE>
**Event Loop Protection (Worker Threads):** Never block the Event Loop. Node.js is single-threaded. Any operation taking > 10ms of synchronous CPU time MUST be offloaded to a worker thread:
- Heavy AST mutations (`ts-morph`) inside `ASTCodeSurgeon`.
- Repairing 100KB+ JSON files.
- Intensive chunking of large Markdown/PDFs.
</CRITICAL_DIRECTIVE>

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

Mọi thao tác rename trên Windows BẮT BUỘC đi qua hàm wrapper safeRename() có cơ chế Retry để chống OS lock file.

```typescript
import { promises as fsp } from "fs";

// ✅ CORRECT — Atomic: write .tmp then safeRename (prevents corrupt file on crash & retries on EBUSY)
const tmpPath = `${dbPath}.tmp`;
await fsp.writeFile(tmpPath, data, "utf-8");
await safeRename(tmpPath, dbPath);

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
│   ├── ModelOrchestrator.ts  # Adaptive AI Engine management (local GGUF / cloud API / hybrid)
│   ├── PromptBuilder.ts     # System prompt assembly (route-aware 4-tier memory injection + L2 semantic + HeraCompass ICL)
│   ├── SessionOrchestrator.ts # Session lifecycle, state persistence
│   ├── IsolatedAgentTurn.ts # Background isolated turn execution (XML tool parsing)
│   ├── NLCommandTranslator.ts # Natural language → command mapping
│   ├── DependencyContainer.ts # Service DI container
│   ├── HeartbeatManager.ts  # Interval-based health check trigger
│   ├── VirtualManager.ts    # Zero-VRAM context orchestration
│   ├── TaskQueue.ts         # Sequential async task processing (singleton)
│   ├── ZaloPolling.ts       # Inbound message listener (long-polling)
│   ├── TelemetryProfiler.ts # Performance metrics (debounced write)
│   ├── ApprovalEngine.ts    # Multi-step HITL approval workflows
│   ├── ASTActuator.ts       # Code modification via AST
│   ├── ASTHealer.ts         # Auto-fix broken code
│   ├── UIController.ts      # WebSocket bridge to Electron UI (token auth)
│   ├── stream/              # 🔊 LLM Stream Processing (extracted from AgentLoop)
│   │   ├── StreamSanitizer.ts    # State machine: thinking block muting, stop-sequence stripping, tool call detection
│   │   └── ToolCallExtractor.ts  # XML <tool_call> + raw JSON parsing (jsonrepair)
│   ├── events/              # 🔗 Event Wiring (extracted from CoreKernel)
│   │   └── ReactiveSync.ts       # AgentLoop ↔ CoreKernel lifecycle callbacks (thinking, TTS, approval, anomaly, latency masking)
│   ├── queue/               # 📦 Persistent Message Queue
│   │   └── PersistentQueue.ts    # SQLite-backed crash-resilient queue (lazy init, replaces volatile RAM array)
│   ├── ai/                  # 🤖 AI sub-modules
│   ├── bootstrap/           # 🚀 Boot sequence modules
│   ├── config/              # ⚙️ Configuration loaders
│   ├── hubs/                # 📡 Remote control hubs
│   └── orchestrators/       # 🎭 Sub-agent orchestrators
│
├── memory/                  # 💾 Persistence Layer (LIVA-UHM)
│   ├── EncryptionEngine.ts  # Centralized AES-256-GCM + Atomic Write protocol
│   ├── StructuredMemory.ts  # Facade: KV facts + orchestration (delegates vec/events to repos below)
│   ├── VectorRepository.ts  # [Phase 3.3] sqlite-vec CRUD: upsert, KNN search, DLQ (extracted from StructuredMemory)
│   ├── EventRepository.ts   # [Phase 3.3] Event bricks + Turn Layer + Memory Touch (extracted from StructuredMemory)
│   ├── SemanticRouter.ts    # 🧠 Intent router (cosine similarity, <100ms, 6 routes incl. tool_recall + news_briefing, adaptive threshold)
│   ├── ReflectionDaemon.ts  # 🔄 Dual-Perspective Φ/Ψ extraction (debounced 12s)
│   ├── ConsolidationCron.ts # 💤 Sleep-time consolidation (idle 30min + manual)
│   ├── HeraCompass.ts       # Error insight DB (FTS5 full-text search, utility scoring)
│   ├── PersonalKnowledgeExtractor.ts  # Auto-extract user preferences
│   └── SensoryManager.ts    # Multi-modal input aggregation (TTL + GC)
│
├── skills/                  # 🔧 MCP Tools (77 skills, Domain-driven architecture)
│   ├── agentic/             # AI scientist, code gen, hypothesis, planning
│   ├── core/                # File I/O, weather, translate, execute commands, GitNexus query, browser harness
│   ├── data/                # Data extraction, charts, QR code, image manipulation, DB, ZIP, vision
│   ├── devops/              # Docker, deployment, code runner, network diagnostics, system metrics, log analyzer
│   ├── docs/                # Report generation, PDF export, Google Docs/Sheets
│   ├── personal/            # Calendar, notes, expense tracker, auto-backup, media control, clipboard
│   ├── social/              # Email (unified read + detail), Telegram, Zalo, Messenger
│   ├── system/              # System audit, health check, hardware control, window arranger
│   └── web/                 # Browser automation, web search, summarizer, YouTube downloader, computer use
│
├── security/                # 🛡️ Guardrails
│   ├── SecurityGateway.ts   # Zero-Trust ingress security layer
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
│   └── MicroVMDaemon.ts     # LocalSandbox with filesystem deny list (isolated-vm/WASI)
│
├── evolution/               # 🧬 Singularity Pipeline (DAG)
│   ├── EvolutionPipeline.ts # Main Orchestrator
│   ├── EngineManager.ts     # Safe VRAM & Hot-swap
│   ├── ASTMutator.ts        # Direct AST surgery (No SkillRegistry)
│   ├── ASTCodeSurgeon.ts    # Phẫu thuật AST với Path Jail và Atomic Write
│   ├── GitNexusIndexer.ts   # Daemon chạy gitnexus analyze ngầm
│   ├── LivaHarnessOrchestrator.ts # Docker harness orchestration
│   └── RollbackManager.ts   # Safe rollback on failure
│
├── services/                # 🎤 Peripheral Services
│   ├── EmailClientManager.ts# IMAP Daemon lắng nghe email
│   ├── TelegramManager.ts   # Tương tác Telegram Bot API
│   ├── EmbeddingService.ts  # ⭐ Singleton embedding proxy → delegates to llama-server `/v1/embeddings` GPU API
│   ├── VoiceEngine.ts       # TTS token streaming (Edge-TTS primary)
│   ├── KokoroVoiceEngine.ts # Kokoro-JS ONNX TTS (local-first offline fallback)
│   ├── IVoiceEngine.ts      # VoiceEngine interface contract
│   ├── WhisperNode.ts       # Speech-to-text (safeFetch to Whisper API)
│   ├── VADWorkerBridge.ts   # ⭐ [v22] Worker-offloaded VAD event bridge (speech_start/end → CoreKernel)
│   ├── SmartTurnVAD.ts      # Legacy turn-based VAD (fallback)
│   ├── AppWatcherService.ts # Active app monitoring (foreground window detection)
│   ├── ProactiveDaemon.ts   # ⭐ [v24] Shadow Digest Pipeline (background news aggregation + VRAM-gated synthesis)
│   └── voice/               # 🎙️ Voice sub-modules
│
├── workers/                 # 🧵 Worker Threads (offloaded CPU-heavy tasks)
│   ├── VADWorker.ts         # ⭐ [v22] Silero ONNX VAD inference (NEVER on main thread)
│   └── WhisperWorkerWrapper.cjs # Whisper CJS bridge for worker context
│
├── utils/                   # 🔨 Shared Utilities
│   ├── HttpClient.ts        # ⭐ safeFetch() + withSafeTimeout() — THE fetch/timeout wrappers
│   ├── PlaywrightBrowser.ts # Browser singleton factory (auto-detect Chrome/Edge)
│   ├── logger.ts            # Pino async logger + TraceContext mixin
│   ├── TraceContext.ts      # ⭐ AsyncLocalStorage trace IDs — auto-injects into pino logs
│   ├── ErrorBoundary.ts     # Global process error safety net (unhandledRejection + uncaughtException)
│   ├── TTSFormatter.ts      # ⭐ [v22] Semantic Clause Chunking for TTS (Vietnamese conjunctions + 8-word overflow)
│   ├── BackchannelDetector.ts # ⭐ [v23] Vietnamese/English filler classifier for Two-Stage Barge-in
│   ├── FeatureFlags.ts      # Feature flag management (runtime toggles)
│   ├── ShieldGuard.ts       # Security guard utility
│   ├── MemoryTelemetry.ts   # Memory usage telemetry
│   ├── AuditLogger.ts       # Structured audit logging
│   ├── ZaloNotifier.ts      # Fire-and-forget Zalo notifications
│   ├── LivaEngine.ts        # LLM client factory (SecureLivaEngine + Seal Token)
│   ├── NativeIPCClient.ts   # gRPC client to Python engine (GRPCStream async iter)
│   ├── JsonExtractor.ts     # ⭐ safeExtractJSON() — centralized LLM JSON extraction (jsonrepair)
│   ├── VectorMath.ts        # ⭐ cosineSimilarity/F32() — shared vector ops (SIMD-like unrolling)
│   ├── CDPClient.ts         # Chrome DevTools Protocol low-level client
│   ├── CDPHelpers.ts        # CDP utility functions (DOM, screenshots, navigation)
│   ├── ChromeLauncher.ts    # Chrome/Edge process launcher (auto-detect paths)
│   ├── AxTreeParser.ts      # Accessibility tree parser (for BrowserHarness)
│   └── BrowserDetector.ts   # System browser detection utility
│
├── auto_singularity.ts      # 🧬 Entrypoint for EvolutionPipeline (Refactored to DAG)
├── Gateway.ts               # Entry point
├── SkillRegistry.ts         # Dynamic skill loader + MCP fallback
├── MemoryManager.ts         # Memory Facade orchestrator (delegates to StructuredMemory + sqlite-vec + EmbeddingService)
└── system_prompt.ts         # System prompt template
```

---

## 6. 🛑 Anti-Patterns — Hard-Won Lessons (ADD TO THIS LIST!)

### Resource & VRAM Management
- **VRAM Zombie Process**: Quên kill tiến trình `llama-server.exe` khi tắt app sẽ khóa cứng 8GB VRAM vĩnh viễn. Phải kill ĐẦU TIÊN khi shutdown.
- **Hardcoded Sleep Database**: Dùng `setTimeout` chờ DB xả WAL là sai lầm. Bắt buộc dùng event native `await db.close()`.
- **Main Thread Vector Search**: TUYỆT ĐỐI không gọi vector search của node:sqlite trên Main Thread. Các tác vụ FTS5/Vector phải chạy qua DatabaseWorker để bảo vệ Event Loop.
- **LLM GPU for Embeddings**: KHÔNG dùng chung GPU LLM cho việc tạo Vector Embeddings. Tách Embedding sang CPU ONNX Model để Router sống độc lập khỏi VRAMGuard và bảo toàn LLM KV Cache.

### Networking
- **fetch Silent Failure**: `fetch` resolves on HTTP 400/500. ALWAYS use `safeFetch()`.
- **Timer Leak**: `clearTimeout` MUST be in `finally`, not after `await fetch()`.
- **ECONNREFUSED Location**: Native fetch error message = "fetch failed". Real error is in `e.cause.message`, NOT `e.message`.
- **Axios Ghost Properties**: After migration, `e.response?.data` is DEAD CODE. Native fetch errors don't have `.response`.

### Singleton & Resource Management
- **Duplicate Model Loading**: NEVER compute embeddings on CPU (blocks Event Loop). Use `EmbeddingService.getInstance()` which delegates to `llama-server` GPU API (`/v1/embeddings`).
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
- **Destructive Git Rollback**: NEVER use `git checkout -- src/` or `git clean -fd src/` in rollback logic. These commands nuke ALL uncommitted work in the entire `src/` tree. Use physical folder snapshot (`.src.rollback.bak`) via async `fs.promises.cp` instead (NEVER `fs.cpSync`!). (Fixed: BlueGreenRouter V8, 2026-05-05)
- **Unsanitized External Data in LLM Prompts**: NEVER inject clipboard/window title data directly into system prompts. Always run through `sanitizeSensoryData()` (max 2000 chars, HTML strip, control char escape). Attacker can manipulate LLM via clipboard poisoning. (Fixed: SensoryManager, 2026-05-05)
- **Auto-leaking IP Geolocation**: NEVER call external IP lookup APIs unconditionally on boot. Geolocation must be OPT-IN via `LIVA_GEOLOCATION_ENABLED=true`. (Fixed: CoreKernel, 2026-05-05)

### Tauri v2 / Packaging
- **Tauri Architecture**: liva-ui sử dụng Tauri v2 (Rust host + OS WebView) cho:
  - Transparent Widget Window (always on top, mouse passthrough)
  - System Tray với context menu
  - Secure Credential Vault (tauri-plugin-stronghold)
- **Sidecar Pattern**: Gateway chạy như sidecar process, giao tiếp qua Dynamic WS Handshake.
- **ABI Mismatch**: Native C++ addons (`isolated-vm`) crash with stale ABI. Prefer: `node:sqlite` (built-in) or WASM alternatives.
- **Node.js SEA (Single Executable Application)**: Khi bundle file bằng `esbuild` qua `build-sea.js`, **BẮT BUỘC** phải đưa các thư viện Native C++ (`sqlite-vec`) vào mục `external: [...]`. Script hậu kỳ phải copy thủ công các file `.node` từ `node_modules` ra nằm ngang hàng với file `.exe` sinh ra.
- **Bundled Browsers**: `puppeteer` downloads 500MB+ Chromium. Use `playwright-core` (API only, 2MB) + system Chrome via `executablePath`.

### Testing
- **False Green**: 100% pass rate means NOTHING if tests only mock happy paths. Every fetch mock MUST include at least one 4xx/5xx negative test case.
- **Mock fetch Correctly**: Use `vi.stubGlobal('fetch', vi.fn())` — NOT `axios-mock-adapter` or `nock`.
- **UIController Tests**: MUST push `--dev` to `process.argv` before creating UIController instance, and restore in `afterEach`. This bypasses `randomUUID`-based token auth that is inaccessible to test mocks.
- **Fake Timer + Promise Rejection**: When testing timeout behavior with `vi.useFakeTimers()`, attach a `.catch()` handler to the promise BEFORE calling `vi.advanceTimersByTimeAsync()`. Otherwise, the rejected promise becomes an unhandled rejection before `await expect().rejects` can catch it.
- **Module-level Mock Completeness**: When mocking `fs`, include ALL methods used by the target module (`readFile`, `writeFile`, `rename`, `existsSync`, `mkdirSync`). Missing methods cause silent failures in async handlers that swallow errors via try/catch.

### Performance
- **Double Eviction**: Don't call `evictExpired()` then `getAllFacts()` — the latter already calls eviction internally.
- **Health-Check Timer Leaks**: NEVER use `setInterval` for service health checks if it can be reactive. We rely on `safeFetch` timeout failures (boolean return) to trigger TTS engine hot-swaps, avoiding zombie timers completely. (Updated: 2026-05-09)
- **TTS Word-by-Word Stuttering**: NEVER stream raw tokens directly to TTS engines. Emojis, Markdown, and single tokens cause robotic stuttering. ALWAYS buffer tokens into complete, sanitized **clauses** via `TTSFormatter` (Semantic Clause Chunking) before network transmission. TTSFormatter splits on Vietnamese conjunctions (và, thì, mà, nhưng...), clause punctuation (, : ; —), and 8-word overflow for <300ms TTFS. (Updated: 2026-05-12)
- **~~Wake Word Feedback Loop~~ [DEPRECATED v22]**: ~~ALWAYS pause the Always-On microphone while TTS is playing~~ → **REPLACED BY**: Frontend `getUserMedia()` MUST enable `{ echoCancellation: true, noiseSuppression: true }`. Backend NEVER sends `mic_stop`/`mic_start`. Mic is **Always On** for True Full-Duplex. WebRTC AEC handles echo cancellation at the hardware/OS level. (Updated: 2026-05-12)
- **Context-Aware Barge-in (v23 Two-Stage)**: NEVER abort LLM on `speech_start` — causes false positives from coughs/fillers, wasting VRAM. Instead: Stage 1 (speech_start) → Audio Ducking (TTS volume → 20%, LLM keeps running). Stage 2 (transcription_ready) → `BackchannelDetector.isBackchannel()` classifies text: backchannel ("ừm", "ok", <3 words) → restore volume; real speech → `agentLoop.bargeIn()` (AbortController + XML-safe memory truncation with `<interrupted>` tag). (Updated: 2026-05-12)
- **VRAM Territorial Integrity**: When `AI_PROVIDER=local`, GPU VRAM is reserved exclusively for LLM. STT (Whisper) MUST route to Cloud (`WHISPER_CLOUD_URL`) or CPU-only mode. NEVER load Whisper GPU model alongside local LLM — causes CUDA OOM crash. (Added: 2026-05-12)
- **Latency Masking**: For heavy routes (`deep_reasoning`, `system_command`), AgentLoop MUST emit `onLatencyMask` filler audio ("Dạ vâng...", "Hmm...") BEFORE LLM starts generating. This masks the 1.5-3s TTFT behind natural Vietnamese conversational fillers. Perceived latency = 0ms. (Added: 2026-05-12)
- **KV Cache Shifting**: llama-server MUST run with `--cache-reuse 256` to preserve system prompt KV cache across barge-in turns. On interrupt, LLM only recomputes the new user text — saves 40-60% GPU energy. (Added: 2026-05-12)

### Asynchronous & Evolution Anti-Patterns (v25 Hardening)
- **Active Skill Probing:** NEVER run scheduled dry-runs on external APIs (wastes Quota/Rate Limit). ALWAYS use a Passive Circuit Breaker wrapping `SkillRegistry.execute()` to detect failures dynamically. 3 consecutive errors → OPEN_CIRCUIT → `PromptBuilder` prunes the dead tool from `<tools>` XML → LLM won't hallucinate calls to broken skills. (Added: 2026-05-12)
- **Singularity Fork-Bomb:** NEVER use unbounded `while(true)` in `EvolutionPipeline`. MUST implement `MAX_EPOCHS`, Failure Circuit Breakers (`MAX_CONSECUTIVE_FAILURES`), Hypothesis Deduplication (`Set<string>`), and OS Hardware Budgeting (Battery, RAM, CPU load) before starting an epoch. (Fixed: 2026-05-12)
- **Silent Worker Deadlocks:** NEVER assume `worker_threads` (like VAD) only fail via `"error"` events. ONNX Runtime C++/WASM can deadlock silently. ALWAYS implement a Ping/Pong Watchdog Heartbeat to detect and auto-recover from silent deadlocks. `VADWorkerBridge` includes exponential backoff recovery (max 3 attempts). (Added: 2026-05-12)
- **FIFO VRAM Locks:** NEVER use basic FIFO locks for VRAM. Background tasks (Consolidation, Shadow Digest) MUST use Preemptive Locks (`AbortController`) so `AgentLoop` can instantly abort them and steal the GPU when the user speaks. Voice Full-Duplex latency = 0ms. (Added: 2026-05-12)

---

## 7. 🔑 Environment Variables

```bash
# Security
LIVA_ENCRYPTION_KEY=   # [BẮT BUỘC] Chuỗi 32 bytes AES-256 dùng để vận hành EncryptionEngine
LIVA_KERNEL_SECRET=    # [TÙY CHỌN] Chuỗi dự phòng (fallback UUID) dùng cho hệ thống kernel internal

# AI Provider: "local" (GGUF via llama-server), "cloud" (OpenAI-compatible API), or "hybrid"
AI_PROVIDER=local             # "local" | "cloud" | "hybrid"
AI_BASE_URL=                  # Cloud API endpoint (required when AI_PROVIDER=cloud/hybrid)
AI_API_KEY=                   # Cloud API key
AI_MODEL=                     # Cloud model name (e.g. gemini-2.5-flash, gpt-4o-mini)
AI_MODELS_DIR=                # Local model directory (default: ~/.liva/models)
EXPERT_MODEL_NAME=            # Local GGUF model filename
LLM_ENDPOINT=                 # Override LLM API base (default: http://localhost:8000/v1/chat/completions)

# Integrations
ZALO_OA_ACCESS_TOKEN=         # Zalo Bot Creator token (contains ":")
ZALO_USER_ID=                 # Auto-detected on first message
TAVILY_API_KEY=               # Web search (free 1000/month, falls back to DDG)
LIVA_GEOLOCATION_ENABLED=     # "true" to enable IP geolocation lookup on boot (opt-in, default OFF)
EMAIL_HOST=                   # IMAP server (e.g. imap.gmail.com)
EMAIL_PORT=                   # IMAP port (default: 993)
EMAIL_USER=                   # Email address
EMAIL_PASS=                   # App-specific password

# Internal
LIVA_USE_NATIVE=              # "true" to use gRPC native engine (bypass HTTP health check)
LIVA_TTS_ENGINE=              # "python" (Default: Edge-TTS) or "kokoro" (Offline fallback)

# Voice Pipeline (v23 Sentient Omni-Duplex)
WHISPER_URL=                  # Explicit STT endpoint override (highest priority)
WHISPER_CLOUD_URL=            # Cloud STT for local-LLM mode (e.g. https://api.groq.com/openai/v1/audio/transcriptions)
FF_DISABLE_L2_INJECTION=      # "true" to disable L2 semantic memory injection (default: enabled)
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
- Current baseline: **174 test files, 1794+ tests** (Updated 2026-05-12)

**Test File Map:**
```
tests/
├── core/
│   ├── AgentLoop.test.ts          # Sub-agents: CKA, TEO, LTC, TaskLane
│   ├── ASTActuator.test.ts        # AST mutations, source transforms
│   ├── ASTHealer.test.ts          # Self-healing code patches
│   ├── ApprovalEngine.test.ts     # Multi-step approval workflows
│   ├── CoreKernel.test.ts         # Full bootstrap, peripherals, shutdown
│   ├── CoreKernelAuthority.test.ts # Token issuance, phase verification
│   ├── DependencyContainer.test.ts # Service DI container tests
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
│   ├── ZaloPolling.test.ts        # Token validation, message emit, offset
│   ├── stream/
│   │   ├── StreamSanitizer.test.ts    # Token filtering, thinking block muting, tool call detection
│   │   └── ToolCallExtractor.test.ts  # XML + JSON parsing, argument deserialization
│   └── queue/
│       └── PersistentQueue.test.ts    # FIFO, channel isolation, crash persistence
├── bridges/
│   ├── CDPBridge.test.ts          # Chrome DevTools Protocol bridge
│   └── VSCodeBridge.test.ts       # VS Code extension bridge
├── memory/
│   ├── ConsolidationCron.test.ts  # Sleep-time consolidation, sessions, L2+L3
│   ├── MemoryManager.test.ts      # Full lifecycle, GDPR purge, UHM init, hybrid context
│   ├── PersonalKnowledgeExtractor.test.ts # Fact extraction, JSON safety
│   ├── ReflectionDaemon.test.ts   # Debounced Φ/Ψ extraction, batch, flush
│   ├── SemanticRouter.test.ts     # Route classification, fallback, confidence
│   ├── SensoryManager.test.ts     # Capture, TTL, prompt injection
│   ├── StructuredMemory.test.ts   # Core KV, Vector, Events tests
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
│   ├── WhisperNode.test.ts        # STT model lifecycle
│   ├── VADWorkerBridge.test.ts    # Worker-offloaded VAD event bridge
│   └── ProactiveDaemon.test.ts    # [v24] Shadow Digest: topics, VRAM guard, synthesis, delivery
├── skills/                         # 30 skill test files (domain-organized)
│   ├── AIScientist.test.ts        # Research agent skill
│   ├── AppendGoogleDoc.test.ts    # Google Docs integration
│   ├── BrowserHarness.test.ts     # Browser automation
│   ├── AutoBackup.test.ts           # File/folder backup
│   ├── CodeRunner.test.ts           # JS/Python/TS snippet execution
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
│   ├── ImageManipulator.test.ts     # Image resize/compress/convert
│   ├── NetworkDiagnostics.test.ts   # Ping, DNS, speed test
│   ├── PDFGenerator.test.ts         # PDF creation from markdown
│   ├── QRCodeTool.test.ts           # QR code generation
│   ├── ReadEmails.test.ts           # Unified email interface (UID, filter, topic)
│   ├── ReadEmailDetail.test.ts      # Full email by UID
│   ├── SummarizeContent.test.ts     # URL/text AI summary
│   ├── TranslateText.test.ts        # Multi-language translation
│   ├── UpdateMemory.test.ts       # Category routing, TTL
│   ├── WebSearch.test.ts          # Tavily + DDG fallback, error paths
│   ├── WriteLocalFile.test.ts     # Atomic write, path guardrails
│   ├── ExpenseTracker.test.ts       # Personal finance tracking
│   ├── YouTubeDownloader.test.ts    # YouTube video/audio download
│   └── ... (50+ total)
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
npx tsx src/Gateway.ts          # Start gateway (dev CLI)
npm run desktop                   # Start Electron desktop app
npx vitest run                  # Run tests
npx vitest watch                # Watch mode

# Self-Evolution (AI research pipeline)
cross-env NODE_OPTIONS="--expose-gc --max-old-space-size=8192" npx tsx src/auto_singularity.ts

# Full system startup (Windows)
start_all.bat                   # Starts: Engine → Voice → Gateway → UI

# GitNexus (code intelligence)
# NOTE: GitNexusIndexer resolves binary locally (node_modules/.bin/gitnexus)
# --embeddings is OPT-IN only; boot-time indexing skips it to avoid blocking startup
npx gitnexus analyze            # Rebuild code graph (CLI shorthand)
npx gitnexus analyze --embeddings  # With semantic embeddings (heavy, opt-in)
```

---

## 10. 🗺️ Key Data Flows

### User Message → AI Response
```
User Input (Tauri WebView WebSocket)
  → UIController.ts
  → AgentLoop.ts (FSM: IDLE → THINKING)
  → SemanticRouter.route() — intent classification (<100ms, sqlite-vec cosine)
  → PromptBuilder.ts (route-aware context + token budget)
     chitchat → minimal (profile only)
     system_command → skip RAG (profile + sensory)
     factual_recall/deep_reasoning → full L1+L2+L3 pipeline
  → ModelOrchestrator.ts (Adaptive: local GGUF on port 8000 or cloud API)
  → LLM generates response + optional tool calls
  → SkillRegistry.ts → skill.execute()
  → ZMAS_Guard.ts (filter output)
  → ReflectionDaemon.queueTurn() — debounced Φ/Ψ extraction
  → AgentLoop.ts (REFLECTING → IDLE)
  → UIController.ts → Electron UI
```

### Memory Architecture (LIVA-UHM v2 — Consolidated Brain)
```text
L0: Local Context (RAM)       — In-memory cache in MemoryManager
L1: StructuredMemory (SQLite) — Event bricks (Φ Factual + Ψ Relational) + KV facts
L2: VectorMemory (sqlite-vec) — Consolidated narratives. Tích hợp **H-MEM Positional Index** trỏ ngược về L1 qua `source_event_ids` (O(1) `json_each` Drill-down).
L3: PersonalKnowledge (KV)    — Insights người dùng. Áp dụng **Ebbinghaus Forgetting Curve** (V8 Math.exp decay + chunking), Strength < 0.2 bị loại khỏi prompt.

[Orchestration Pipeline]
- SemanticRouter → routes queries (<100ms, sqlite-vec cosine + FTS5 Drill-down).
- ReflectionDaemon → extracts Φ/Ψ ngầm. Emits passive signals via MemoryEventBus (0 extra LLM calls).
- ConsolidationCron → Hợp nhất L1→L2+L3. 
  Triggered by: 30min Idle HOẶC Passive Signal Burst (topicShiftCount >= 3 OR unconsolidatedCount >= 20, 15s debounce). 
  🚨 Strict Guardrail: Kích hoạt CHỈ KHI `agentLoop.getState() === 'IDLE'` để bảo vệ VRAM.
```

### Agentic Memory Management (AgeMem)
```text
ManageMemory Skill → Agent CRUD trực tiếp lên L1 KV Facts (add/update/delete/search).
  - Namespace Isolation: Chỉ categories whitelisted (user_preferences, relationships, facts, work_context, personal_info).
  - HITL Guard: delete action BẮT BUỘC human approval.
  - Rate Limit: Max 5 mutations/turn.
  - Ebbinghaus Sync: update → memory_strength reset to 1.0.
  - Audit: source='agent_explicit' (phân biệt vs 'auto_extract').
```

### DLQ 3-Strike Schema
```text
events.consolidation_status: 'pending' (new) | 'consolidated' (done) | 'dlq' (failed 3x)
events.retry_count: 0-3 (auto-increment on Zod fail)

⚠️ Backward Compat: ALTER TABLE DEFAULT 'consolidated' — old data KHÔNG bị re-process.
⚠️ Partial Index: idx_events_pending ON events(eventId) WHERE consolidation_status = 'pending'.
```

### UHM Guardrails (MUST follow)
| ID | Rule | Rationale |
|---|---|---|
| G1 | No SQLite math functions | `Math.exp()` in V8 only — SQLite lacks `EXP()` |
| G2 | RAM-buffered fact touches | Prevents write amplification on hot paths |
| G3 | `json_each()` for variable binding | Bypasses SQLite 999-param limit safely |
| G4 | Cap `sourceEventIds` at 50 | Prevents VRAM/RAM overflow on vector meta |
| G5 | Zod-validated `source_event_ids` | `EventIdsSchema.safeParse()` — prevents LLM garbage crashing `json_each` |
| G6 | 15s affective debounce | Prevents event loop flooding |
| G7 | EventBus decoupling | `MemoryEventBus` — zero import coupling between ReflectionDaemon ↔ ConsolidationCron |
| G8 | Atomic transactions | BEGIN/COMMIT/ROLLBACK for batch writes |
| G9 | Dual VRAM guard | `isRunning` + `agentLoopStateGetter() === 'IDLE'` — blocks concurrent LLM ops |
| G10 | Shutdown flush guarantee | `close()` → `flushFactTouches()` before `db.close()` |
| G11 | Chunked decay + `setImmediate` yield | 500-row chunks prevent Event Loop blocking >10ms |
| G12 | `VACUUM INTO` for backup | NEVER `fs.promises.cp` on running SQLite — guaranteed WAL corruption |
| G13 | DLQ 3-Strike | Events failing Zod 3x → `consolidation_status='dlq'`, excluded from retry |
| G14 | AgeMem namespace isolation | Only whitelisted categories accessible via ManageMemory skill |

### Error Self-Healing
```
Tool fails → HeraCompass.learnFromError() → LLM generates rule
Next attempt → HeraCompass.getRelatedInsight() → Injects past lesson
Success/Failure → updateUtilityScore() → Verified or Garbage-Collected
```

### Hybrid TTS & Reactive Hot-Swap Pipeline
```
Gateway CoreKernel → Default: VoiceEngine (Python Edge-TTS via safeFetch, Zero Event Loop Block).
[Timeout/Network Error] → safeFetch returns false → Trigger Circuit Breaker.
Hot-Swap: CoreKernel destroys Python Engine → Lazy Loads KokoroVoiceEngine (100% Offline, yields Event Loop via setTimeout).
```

### STT → Agent Pipeline (v23 Sentient Omni-Duplex)

> **VAD chạy WASM trên Frontend. Backend TTS stream buffer về Electron để phát qua Web Audio API (Đảm bảo WebRTC AEC hoạt động).**

```
[Sentient Omni-Duplex Pipeline]
Microphone (Electron AEC Enabled, Always On) → VAD WASM (Frontend)
  → WS Binary (Chỉ mở khi có speech_start)
    → emit("speech_start") → Stage 1 Barge-in: Audio Ducking (TTS Volume → 20%, LLM STILL RUNNING)
    → emit("transcription_partial") → Speculative RAG Warming (Pre-fetch L2/L3 Vectors → RAM Cache)
    → emit("speech_end") → WhisperNode (Asymmetric Routing: CPU-Whisper/Cloud if LLM owns GPU)
  
  → emit("transcription_ready") (Text = T)
  → Stage 2 Barge-in (BackchannelDetector):
      IF T is Backchannel ("ừm", "ok", cough, <3 words) → Restore TTS Volume 100% (Zero VRAM waste)
      ELSE → AgentLoop.bargeIn():
         1. voiceEngine.preempt()     → Kill TTS audio instantly
         2. AbortController.abort()   → Kill LLM stream, free VRAM
         3. XML-Safe Memory Truncation → Strip dangling <tool_call>/<thinking> + append <interrupted>
  
  → SemanticRouter (uses Speculative Cache if available, <50ms)
       IF route = deep_reasoning | system_command:
         → Latency Masking: Play filler audio ("Dạ vâng...", "Hmm...") via TTS
  → LLM Inference (--cache-reuse 256: KV Cache Shifting, reuses System Prompt KV)
  → Semantic Clause Chunking TTS (<300ms TTFS)
      Split on: . ? ! \n | Vietnamese conjunctions (và, thì, mà, nhưng...)
      | , : ; — | 8-word overflow
```

Frontend getUserMedia MUST enable: { echoCancellation: true, noiseSuppression: true }
Backend NEVER sends mic_stop/mic_start — mic is Always On.

### L2 Semantic Memory Injection (Activated v22)
```
PromptBuilder.buildContextPrompt()
  → IF route = factual_recall | deep_reasoning
  → IF remainingBudget > 500 chars
  → EmbeddingService.embed(userText) [1500ms circuit breaker]
  → StructuredMemory.searchAnchors(queryVec, top_k=3)
  → Inject into <context_memory> XML sandbox (max 30% budget)
Opt-out: FF_DISABLE_L2_INJECTION=true
```

### Proactive Shadow Digest Pipeline (LIVA v24)
```
[Asynchronous Pre-computation — "Cook in the dark, serve at the speed of light"]
Timer (Cron .unref()) → Check `agentLoop.isBusy === false` (VRAM Guard)
  → Read L3 PersonalKnowledge (Extract topics with `memory_strength > 0.2`)
  → safeFetch Tavily API (News Mode, max 5 articles) — Zero Event-Loop Block
  → Asymmetric LLM Synthesis (Cloud API preferred to preserve Local VRAM)
  → Cache Markdown summary to SQLite `daily_briefings` (TTL: 24h, is_read: 0)

[Sentient Omni-Delivery]
- Push Mode (Scheduled): ProactiveDaemon checks Presence.
  IF Active → Silent UI Toast + Soft Ding (Wait for user voice confirm to play TTS).
  IF Offline → Egress via TelegramManager/ZaloNotifier.
- Pull Mode (User Intent): SemanticRouter detects `news_briefing`.
  PromptBuilder injects SQLite cache into <daily_briefing> XML sandbox.
  Zero Web-Search Delay, Response TTFS < 300ms.
```

---

## 11. 🔒 Shutdown Chain (`CoreKernel.shutdown()`)

Every resource with cleanup requirements is called in order via **asynchronous execution** to guarantee database writes:

```typescript
async CoreKernel.shutdown()
  ├── modelOrchestrator.killLlamaServer() // 🚨 STEP 1 (IMMEDIATE): Kill llama-server to release VRAM (local mode only)
  ├── workerThreadPool.terminateAll()     // Kill toàn bộ node:worker_threads
  ├── clearInterval(gcIntervalId)     // Own GC timer
  ├── fileWatcher.close()             // FSWatcher file handles
  ├── zalo.stop()                     // ZaloPolling timer
  ├── voiceEngine.destroy()           // TTS timers/buffers
  ├── whisperNode.destroy()           // STT model + listeners
  ├── memory.dispose()                // 🚨 MUST await `unifiedMemory.close()` — no sleep.
  │   ├── reflectionDaemon.flushPending() // Flush pending Φ/Ψ extractions
  │   ├── reflectionDaemon.dispose()      // Clear debounce timer
  │   ├── consolidationCron.dispose()     // Clear idle-check interval
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

> [!IMPORTANT]
> The `Gateway.ts` handles graceful shutdown asynchronously. Absolutely NO hardcoded sleeps (`setTimeout`)! Use `await db.close()` to natively wait for SQLite WAL flush.
> When adding a new service with timers, intervals, or ML models, you **MUST** add its cleanup call here. This is enforced by Write Protocol.

---

## 12. 🚨 ESLint Guards (Recommended CI Rules)

```jsonc
// .eslintrc.json
{
  "rules": {
    "no-restricted-imports": ["error", {
      "paths": [
        { "name": "@xenova/transformers", "message": "BANNED: Use EmbeddingService → GPU /v1/embeddings" },
        { "name": "@huggingface/transformers", "message": "BANNED: CPU Tensor blocks Event Loop. Use llama-server /v1/embeddings" },
        { "name": "@lancedb/lancedb", "message": "BANNED: Use sqlite-vec within node:sqlite" },
      ]
    }],
    "no-restricted-globals": ["error",
      { "name": "fetch", "message": "Use safeFetch() from src/utils/HttpClient.ts" }
    ],
    "no-console": ["error", { "allow": [] }]
  }
}
```

