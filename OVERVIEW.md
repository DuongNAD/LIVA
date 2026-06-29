# LIVA — Project Overview

> Generated quick-orientation summary (2026-06-29). This is a fast on-ramp, **not** the source of truth.
> - **`AI_CONTEXT.md`** is the authoritative SSOT for architecture, conventions, and hard-won rules — if it diverges from this file, it wins.
> - **`PROJECT.md`** tracks the macOS / Gemma 4 upgrade milestones (M1–M20).

## What LIVA is

A hybrid-intelligence, multi-agent **desktop AI assistant** (a "Jarvis"-style Cognitive OS) for **Windows & macOS**. It dynamically routes between **local GPU inference** (GGUF via llama.cpp) and **cloud APIs** to balance speed, reasoning depth, and consumer-hardware limits. Single-author project by Nguyen Anh Duong (FPT University Hanoi).

## Monorepo layout

npm workspaces · Node 20/22+ · ESM-only · TypeScript strict.

| Module | Stack | Role |
|--------|-------|------|
| `liva-gateway` | Node.js / TypeScript | "Central Brain" — `AgentLoop` FSM, 93+ MCP skills, 4-tier memory, security guards, self-evolution pipeline |
| `liva-ai-engine` | Python 3.11 + C++ (llama.cpp) | Local inference engine, gRPC server, Whisper STT, Edge-TTS voice |
| `liva-ui` | Vue 3 + Vite | Chat widget, 3D avatar (VRM/Live2D), memory dashboard |
| `liva-desktop` | Tauri v2 (Rust + WebView) | Transparent "Ghost Mode" overlay, system tray, Stronghold credential vault |
| `packages/liva-common` | TypeScript | Shared types/interfaces (no build step) |
| `mvc-simulation` | Java | Unrelated educational project (no build tooling, not CI'd) |

## Core architecture pillars

- **Agent loop** — `AgentLoop.ts` FSM (IDLE→THINKING→ACTING→REFLECTING); `SemanticRouter` classifies intent (<100ms, sqlite-vec cosine) → `PromptBuilder` assembles route-aware context → `ModelOrchestrator` picks local/cloud → `SkillRegistry` executes tools → `ZMAS_Guard` filters output.
- **Memory (LIVA-UHM)** — Consolidated single-file `node:sqlite` + `sqlite-vec` + `FTS5`. Tiers: L0 RAM → L1 StructuredMemory (events + KV) → L2 vector narratives → L3 personal knowledge (Ebbinghaus decay). Background `ReflectionDaemon` + `ConsolidationCron` distill memory asynchronously.
- **Sequential Hot-Swap** — Single model in VRAM at a time (consumer GPUs OOM otherwise). `ModelOrchestrator` swaps a fast Router (≈4B/5.3GB) ↔ heavy Expert (≈12–26B/6.7GB) via `mmap`, with a 120s Expert cooldown TTL.
- **Voice (full-duplex)** — Always-on mic, frontend WASM VAD + wake-word; two-stage barge-in with audio ducking; Whisper STT; Edge-TTS primary with Kokoro-JS offline fallback; semantic clause chunking for <300ms TTFS.
- **Self-evolution (Singularity)** — `EvolutionPipeline` mutates its own code via `ts-morph` AST surgery in `isolated-vm`/WASI sandboxes, with `RollbackManager` snapshot safety. GitNexus indexes the call graph for impact analysis.

## Ports & wiring

- **Ports:** gateway WS `8082` · UI dev `5173` · native gRPC engine `8100` · llama-server HTTP `8000`(/`8001`) · voice `8002` · Whisper STT `8101`.
- **Gateway ↔ engine:** gRPC (`liva_engine.proto`: StreamChat / Chat / HealthCheck / Embed / SwapModel) in prod; OpenAI-compatible HTTP in dev/cloud.
- **UI ↔ gateway:** WebSocket on `127.0.0.1:8082`, MessagePack binary frames + raw MP3 voice chunks.

## Non-negotiable conventions

(Detailed in `AI_CONTEXT.md` §3–§6.) ESM only; `safeFetch()` not `fetch` (native fetch doesn't throw on 4xx/5xx); `node:worker_threads` for any >10ms CPU work; `pino` logger not `console.log`; `lru-cache` not unbounded `Map`; atomic `.tmp`+`safeRename` writes; banned libs (axios, puppeteer, lancedb, transformers-on-main-thread, sqlite3, Docker/WSL2). **Platform branching is STRICT:** macOS work → `mac` branch, Windows → `main`.

## Tests

- **vitest** (TS) — **must** run `--pool=forks --max-workers=1` + raised heap (`NODE_OPTIONS=--max-old-space-size=4096`) or CI deadlocks.
- **pytest** (Python) — `--ignore=llama_cpp_src`.
- Baseline ≈174 test files / 1794+ tests. See the `/run-tests` skill for the correct per-workspace commands.
