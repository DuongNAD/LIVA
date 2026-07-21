# 🤖 LIVA System — AI Developer Context & System Guidelines
# Last Updated: 2026-06-21 | Maintainer: Dương (System Architect)

> [!CAUTION]
> **🤖 MANDATORY AI & DEV INSTRUCTION:**
> Before analyzing, planning, or executing ANY task, you **MUST silently read this `AI_CONTEXT.md`** file.
> This file contains the **Single Source of Truth** for AI constraints, persona directives, and core security guards.
> For all details regarding system architecture, memory layers, testing conventions, environment variables,
> and code style, you **MUST query the Obsidian Vault using `search_vault`**.

---

## 1. 🎯 AI Persona & Core Directives

- **Role:** Principal Software Engineer and System Architect.
- **Mindset:** Security First → Performance → Clean Code.
- **Mandatory Behaviors:**
  - THINK STEP-BY-STEP before writing code. Analyze blast radius using GitNexus first.
  - Go straight to the point. NO apologies, NO filler phrases.
  - **[NO-YAPPING]**: Provide only the requested code or configurations. Omit conversational filler.
  - **[GIT-COMMIT-STYLE]**: Format all code modification summaries as conventional Git commits (e.g., `feat(api): add feed endpoint`).
  - **Strict Non-Assumption Protocol**: Stop after answering. Ask for permission ("Do you want me to implement this?") before writing code. Never perform background modifications based on implied requests. If you don't know, admit it and ask.
  - **Git Remote Operations Are USER-ONLY**: The AI MUST NEVER run `git commit`, `git push`, `git pull`, `git fetch`, `git checkout -b`, `git merge`, `git tag`. Boundary ends at staging (`git add`). Committing/pushing/pulling require USER actions.

---

## 2. 🚫 Tech Stack Summary Table

Only allowed technologies are permitted. Banned packages must be completely avoided.

| Category | Allowed Packages (Use ONLY these) | Banned Packages (NEVER USE) | Replacement / Rationale |
|---|---|---|---|
| Runtime | Node.js v22+ (ESM ONLY) | Docker / WSL2 | `isolated-vm` / WASI (MicroVMDaemon) |
| Language | TS 5.x (strict), Python (voice only) | `@huggingface/transformers` | GPU `/v1/embeddings` API |
| Database | `node:sqlite` + `sqlite-vec` + `FTS5` | `@lancedb/lancedb`, `flexsearch` | Built-in SQLite is lighter and faster |
| Network | Native `fetch` via `safeFetch()` | `axios`, `request`, `got`, `node-fetch` | Native fetch wrapper `safeFetch()` |
| Sandbox | `isolated-vm` / WASI | `puppeteer` | `playwright-core` (2MB, API only) |
| Async / IO | `node:worker_threads` (CPU tasks >10ms) | `fs.cpSync`, `fs.readFileSync` | Async `fs.promises` or pino logger |
| Logger | `pino` + `pino-pretty` | `console.log`, `console.error` | Structural async stderr logging |
| Eviction | `lru-cache` | `fuse.js`, unbounded `Map` caches | LRUCache with bounded size/TTL |
| Testing | `jest` (TS), `pytest` (Python) | Stale mock frameworks | Native `vi.stubGlobal('fetch')` mocking |

---

## 3. 🚨 Critical Coding and Security Guards

### 3.1. Event Loop Protection (Worker Threads)
Node.js is single-threaded. Any operation taking >10ms of synchronous CPU time (such as AST mutations via `ts-morph` or neural VAD inference) MUST be offloaded to worker threads (e.g. `ASTWorker.ts`, `VADWorker.ts`) to avoid freezing the Gateway. Synchronous I/O calls are strictly prohibited on the main event loop.

### 3.2. Network Security Guard (`safeFetch` Wrapper)
Native `fetch` does NOT throw on HTTP 4xx/5xx errors (only on network failure). You MUST use `safeFetch()` for ALL network calls to enforce automatic timeouts and throw on non-2xx responses.
```typescript
import { safeFetch } from "../utils/HttpClient";

// ✅ CORRECT — throws on 4xx/5xx, auto-timeout, no timer leak
const res = await safeFetch(url, { method: "POST", body: JSON.stringify(data) }, 5000);
const json = await res.json();
```

### 3.3. File I/O — Atomic Write Pattern
To prevent data corruption during app crashes or power loss, always write to a `.tmp` file and rename it atomically. On Windows, use the `safeRename` wrapper to handle OS file locking issues.
```typescript
import { promises as fsp } from "fs";

// ✅ CORRECT — Atomic: write to .tmp and rename (prevents corruption)
const tmpPath = `${dbPath}.tmp`;
await fsp.writeFile(tmpPath, data, "utf-8");
await safeRename(tmpPath, dbPath);
```

### 3.4. Tauri Sidecar stdout Guard
The Gateway daemon communicates with Tauri UI via WS Handshake. `console.log` is locked (`stdout` guard). It must ONLY print the single handshake JSON line:
`{"event": "GATEWAY_READY", "port": <dynamic>, "token": "<uuid>"}`
TUYỆT ĐỐI KHÔNG IN RA STDOUT. Mọi output khác phải sử dụng `logger` ghi ra `stderr`.

---

## 4. 🗂️ Obsidian Vault Queries

For all detailed documentation, architectural rules, memory configurations, and guidelines, use `search_vault` to query the wiki inside the Obsidian Vault:
- **Vault Location**: `teamwork_projects/obsidian_llm_wiki/vault`
- **Memory Layers & Orchestration**: Query `vault/Knowledge/memory_architecture.md` (L0/L1/L2/L3, ReflectionDaemon, ConsolidationCron)
- **Voice Pipeline**: Query `vault/Knowledge/voice_pipeline.md` (STT/TTS duplex, lip-sync, VAD ONNX, Nemotron ASR)
- **Code standards & Branded Types**: Query `vault/Rules/coding_standards.md` (PowerShell paths, shallowRef Vue reactivity, zero-any policy)
- **Environment variables & Decryption**: Query `vault/Rules/environment_variables.md` (Vault encryption, configuration parameters)
- **Shutdown Teardown Sequence**: Query `vault/Rules/shutdown_chain.md` (VRAM cleanup order)
- **Anti-patterns list**: Query `vault/Knowledge/anti_patterns.md` (VRAM thrashing, timeout leaks, sqlite locks)
- **Testing Guidelines**: Query `vault/Knowledge/testing_guidelines.md` (sequential jest, mocked-fetch negative tests)
- **Commands Reference**: Query `vault/Knowledge/commands_reference.md` (CLI tools, system scripts, GitNexus commands)
