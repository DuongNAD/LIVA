<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **LIVA** (24380 symbols, 53853 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user. For unified PDG impact, add `mode: "pdg"` with optional `line: <N>` — it returns statement-level `affectedStatements` over CDG + REACHING_DEF and inter-procedural symbols in `interproceduralByDepth`/`byDepth`; no-layer/degraded PDG results are UNKNOWN-risk notes (`--pdg` layer).
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).
- For control/data dependence, `pdg_query({mode: "controls", target: "fileOrSymbol"})` answers "under what condition does X run?" (CDG, incl. guard clauses) and `pdg_query({mode: "flows", target, variable})` traces "where does variable Y flow?" (REACHING_DEF). `--pdg` layer.

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/LIVA/context` | Codebase overview, check index freshness |
| `gitnexus://repo/LIVA/clusters` | All functional areas |
| `gitnexus://repo/LIVA/processes` | All execution flows |
| `gitnexus://repo/LIVA/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->

# LIVA — Workspace & Runtime Map

- Cargo workspace (root `Cargo.toml`): `liva-native-core` (Rust engine — LLM/STT/TTS/agents, WebSocket gateway on port 8002) + `liva-desktop/src-tauri` (Tauri v2 shell that embeds the core in-process). All builds output to the **root** `target\` dir; `liva-native-core\target\` is a stale pre-workspace leftover.
- The frontend is `liva-ui` (Vue 3 + Vite, dev port 5173) — Tauri serves `../liva-ui/dist`. `liva-desktop`'s own package.json scripts are vestigial.
- `liva-voice/` is a separate, still-active Python voice-cloning service: `cd liva-voice; python liva_api.py` (FastAPI, port 8765), started manually. It is NOT the deleted legacy Python (`liva-ai-engine`/`liva-gateway`) that AGENTS.md forbids touching. Its `requirements.txt` now covers the API + ML deps (fastapi/uvicorn/pydantic/edge-tts included).
- Full dev run: `npm run dev` (root) → `scripts\start_all.ps1` (frees ports, starts liva-ui, then `tauri dev`). `STARTUP_GUIDE.md` is pre-migration and stale — don't follow it.

# LIVA Native Core — Build & Test Instructions

## Build Commands
- Build Rust core (output lands in root `target\`, not `liva-native-core\target\`):
  ```powershell
  cd liva-native-core
  cargo build            # add --release for release mode
  ```
- GPU builds: `cargo build --features cuda` (or `vulkan`, `openblas`); default is CPU.
- Prerequisites: CMake + LLVM with `LIBCLANG_PATH` set (llama.cpp is compiled from C++ source; CI does `choco install llvm`); Rust ≥ 1.85 (edition 2024). First build is very long — llama-cpp crates are pinned to `opt-level=3` even in dev.
- Runtime shells out to `espeak-ng` (TTS G2P) and `ffmpeg` (Telegram voice) — both must be on PATH.

## Test Commands
- Run standard Rust unit & integration tests:
  ```powershell
  cd liva-native-core
  cargo test
  ```
  `tests\sandbox_stress.rs` and `tests\self_correction_stress.rs` spawn nested `cargo test` subprocesses — slow, not hung.
- CI (`.github\workflows\test.yml`, windows-latest) — 13 steps, **every step a gate**: `docs-check.mjs` → `docs-citations.mjs` → `npm ci` → cargo cache → LLVM → `vue-tsc --noEmit -p tsconfig.app.json` → `eslint --max-warnings 0` → `npm run test:coverage -w liva-ui` (vitest + istanbul coverage gate) → `cargo test` → `cargo check --all-targets --features experimental` → clippy. No fmt gate; **clippy is a HARD gate** (`-- -D warnings`, 0 warnings since 2026-07-22 — journey 80 → 35 via `--fix`, then 35 → 0 by hand with provably-equivalent rewrites; DSP loop rewrites were additionally verified by a seed-42 VieNeu WAV hash, byte-identical. Remaining `#[allow]`s are deliberate with in-place justifications). **Two measurement traps here, both of which produced a false all-clear before 2026-07-22 — treat any always-green check as suspect.** (1) Clippy: measure with `--message-format=short` and grep `": warning:"`; the short format prefixes paths with `liva-native-core\src\…`, so a naive `grep '^src/'` reports zero. (2) Typecheck: `liva-ui/tsconfig.json` is a solution-style config (`"files": []` + references), so plain `tsc --noEmit` checks **zero files** — and plain `tsc` can't read `.vue` SFCs either. Always use `vue-tsc --noEmit -p tsconfig.app.json`.
- Run specialized verification/correctness executables:
  ```powershell
  # Voice modules correctness (ASR, TTS, preemption, fade-out safety)
  .\target\debug\verify_round2.exe
  
  # LLM router performance and sliding window pruning
  .\target\debug\router_stress.exe
  
  # Voice engine load benchmark (G2P speed, ASR/TTS throughput, chunk boundaries)
  .\target\debug\voice_stress.exe
  
  # WebRTC streaming duplex pipeline (VAD, preemption latency, session IDs)
  .\target\debug\verify_duplex.exe

  # Functional correctness of handle_command, db, crypto, stt, llm, tts
  .\target\debug\verify_integrations.exe

  # Vision change-detection benchmark (find_changes on 1920x1080)
  .\target\debug\screen_vision_bench.exe
  ```
- **End-to-end check over a real WebSocket** — the only test that exercises the dispatch layer (everything else calls `handle_command` in-process and never touches a socket, which is how a swallowed `Err` branch survived unnoticed):
  ```powershell
  # Terminal 1 — keep stdin OPEN. The core reads stdin for IPC and exits on EOF;
  # backgrounding it with stdin closed prints "shutting down" and exits 0, which
  # looks exactly like a successful run.
  $env:LIVA_SERVER_PORT="8099"; $env:LIVA_DB_IN_MEMORY="1"
  .\target\debug\liva-native-core.exe

  # Terminal 2
  node scripts/e2e-gateway.mjs        # PORT=8002 to point elsewhere
  ```
  Works on a debug build and should be run there — `vision:ask` fails fast in debug, which is the case worth checking.

# Environment & Models

- The core reads `LIVA_*` env vars (source of truth: `liva-native-core\src\main.rs`): `LIVA_ENCRYPTION_KEY` (32-byte key, effectively required), `LIVA_SERVER_PORT` (default 8002), `LIVA_STT_MODEL_DIR` (default `models/nemotron-asr`), `LIVA_STT_VI_ENGINE` (`nemotron`|`parakeet` — opt-in offline vi STT; + `LIVA_PARAKEET_MODEL_PATH`/`LIVA_PARAKEET_THREADS`), `LIVA_TTS_MODEL_PATH` (default `models/kokoro-v1.0.onnx`), `LIVA_LLM_MODEL_DIR`/`LIVA_LLM_N_CTX`/`LIVA_LLM_N_GPU_LAYERS`, `LIVA_DB_PATH`, `LIVA_VAULT_PATH`, `TELEGRAM_BOT_TOKEN`. `.env.example` (v30.0 overhaul) is current: the core reads only `LIVA_*`, while `AI_*` are UI-managed (`ApiManagementView.vue`) and legacy `NATIVE_*` are gone — for `LIVA_*` still trust the code (`main.rs`) as source of truth.
- Model weights (`*.onnx`, `*.gguf`) are gitignored and fetched out-of-band. `models/kokoro-v1.0.onnx` is absent by default, so TTS init fails until the model is supplied.
- **Long-term memory is off until you supply an embedding model.** `llm/embedder.rs` wants `models/embedding/{model.onnx,tokenizer.json}` — a 384-dim model, recommended `intfloat/multilingual-e5-small`; override the path with `LIVA_EMBEDDING_MODEL_DIR`, tune retrieval with `LIVA_RAG_TOP_K` (default 3). Missing weights are **not** fatal: startup logs a `WARN` naming the exact directory and model, RAG silently no-ops, and everything else runs. Confirmed live on 2026-07-22 — the gateway boots and serves commands with the directory absent.
- `models/nemotron-asr` is a nested git repo with LFS (NOT a registered submodule) — it permanently shows as "modified content" in `git status`; leave it alone.

# Conventions

- NEVER run `git commit`, `git push`, or `git pull` autonomously — only when the user explicitly asks (per AGENTS.md).
- TS/Vue (enforced by ESLint + husky pre-commit): no `console.*`, no native `fetch` (use `safeFetch()`), no sync `fs*Sync`. Pre-commit runs `eslint --max-warnings 0` on staged `*.{ts,vue}` (via `.lintstagedrc.json`), then an AI audit script that requires `.env` (bypass: `SKIP_AI_HOOK=1`). It does **not** run `tsc` — that gate lives only in CI.
  - `.vue` files were **not linted at all** until 22/07/2026 — `eslint.config.js` had no SFC parser, so all 22 components silently fell outside the three rules above. Now wired via `vue-eslint-parser`, and the 74 `any` sites that had accumulated there were cleaned up the same day — 2 remain, each behind an `eslint-disable-next-line` with a stated reason. Safe to do en masse because TS types are **erased at compile time**: `vite build` before and after emits **19/19 byte-identical** files once chunk hashes and scoped-CSS ids are normalized. `eslint-plugin-vue`'s own style rules are still not enabled — only the parser.
- Docs, comments, and commit messages are often Vietnamese; console I/O assumes UTF-8 (`chcp 65001`).
