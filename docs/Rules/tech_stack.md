---
title: "tech_stack"
tags:
  - liva/rule
author: "worker"
last_update: "2026-06-21T02:21:19Z"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Tech Stack

## Rule Statement
Only allowed packages and runtime features may be used. Banned packages and runtime patterns must be completely avoided. Node.js runtime MUST be v22+ and configured for ESM.

## Rationale
To ensure maximum runtime efficiency, avoid Event Loop locking, protect GPU/VRAM from out-of-memory errors, and prevent package mismatches and security issues.

## Allowed and Banned Packages

### Allowed (Use ONLY these)
- **Runtime**: Node.js v22+ (ESM only, `"type": "module"` in package.json)
- **Language**: TypeScript 5.x (strict), Python (voice_engine only)
- **UI Framework**: Tauri v2 (Rust host + WebView)
- **LLM Runtime**: Native Engine (gRPC) or `llama-server` (C++). Local: GGUF, Cloud: OpenAI-compatible
- **Network**: Native `fetch` via `safeFetch()` wrapper
- **Database**: `node:sqlite` (built-in) + `sqlite-vec` + `FTS5` (with Debounced Writes)
- **Processing**: `node:worker_threads` (for CPU-heavy tasks >10ms)
- **Sandbox**: `isolated-vm` / WASI (MicroVMDaemon)
- **Browser**: `playwright-core` (API-only, no bundled browsers)
- **Logger**: `pino` + `pino-pretty`
- **Testing**: `jest` (TS), `pytest` (Python)
- **Validation**: `zod` v4+
- **Caching**: `lru-cache`
- **LLM Client**: `openai` SDK

### Banned (NEVER USE)
- ❌ **Docker / WSL2**: vmmem consumes 2-4GB RAM. Replace with `isolated-vm` / WASI.
- ❌ **Dual Model Concurrent Load**: Swapping Router and Expert concurrently causes OOM. Replace with `Sequential Hot-Swap Architecture`.
- ❌ **@huggingface/transformers**: CPU Tensor calculation freezes Event Loop. Use GPU `/v1/embeddings` API.
- ❌ **@lancedb/lancedb & flexsearch**: DB bloat. Replace with `sqlite-vec` + `FTS5`.
- ❌ **fs.cpSync**: Blocks Event Loop. Use async `fs.promises.cp`.
- ❌ **axios**: Removed for security/hardening. Use `safeFetch()`.
- ❌ **puppeteer**: Chromium dependency overhead. Use `playwright-core`.
- ❌ **fuse.js**: O(N) search and memory hog. Use `FTS5`.
- ❌ **@xenova/transformers**: Deprecated/unmaintained. Use GPU `/v1/embeddings` API.
- ❌ **request / got / node-fetch**: Redundant. Use `safeFetch()`.
- ❌ **console.log / console.error**: Non-structured, blocking. Use `logger`.
- ❌ **fs.readFileSync / fs.writeFileSync**: Blocks main thread. Use async counterparts.
- ❌ **sqlite3 / sqlite**: Native compilation and ABI bloat issues. Use built-in `node:sqlite`.
- ❌ **__dirname / __filename**: Not available in ESM. Use `import.meta.dirname` / `import.meta.filename`.
- ❌ **Web Speech API**: Sends audio to Google Cloud silently. Use TensorFlow.js (WASM) instead.
- ❌ **fs.promises.cp** on running SQLite: Causes WAL corruption. Use `VACUUM INTO` for atomic backup.

## Examples

### Compliant Behavior
```typescript
import { promises as fsp } from "fs";
import { safeFetch } from "../utils/HttpClient";

// Compliant async reading and fetch wrapper
const data = await fsp.readFile("config.json", "utf-8");
const res = await safeFetch("https://api.liva.local", { method: "GET" });
```

### Non-Compliant Behavior
```typescript
import fs from "fs";
import axios from "axios";

// Non-compliant blocking I/O and banned library
const data = fs.readFileSync("config.json", "utf-8");
const res = await axios.get("https://api.liva.local");
```

## Exceptions
- `@huggingface/transformers` in `EmbeddingWorker.ts` is **ALLOWED** for tokenizer-only usage inside `node:worker_threads` (isolated event loop).
- `fs.readFileSync` / `fs.writeFileSync` in `EncryptionEngine.autoMigrateSensitiveEnvKeys()` and `loadVaultIntoEnv()` are **ALLOWED** as they run synchronously exactly once at boot before the Event Loop starts.

## Verification & Enforcement
- The repository relies on ESLint rule bans (e.g. `no-restricted-imports`, `no-restricted-globals`, `no-console`) to block banned libraries.
