<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **LIVA** (22625 symbols, 50143 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

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
- CI (`.github\workflows\test.yml`, windows-latest): `npm run test -w liva-ui` (vitest) + `cargo test` in liva-native-core. No fmt/clippy gate.
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

# Environment & Models

- The core reads `LIVA_*` env vars (source of truth: `liva-native-core\src\main.rs`): `LIVA_ENCRYPTION_KEY` (32-byte key, effectively required), `LIVA_SERVER_PORT` (default 8002), `LIVA_STT_MODEL_DIR` (default `models/nemotron-asr`), `LIVA_STT_VI_ENGINE` (`nemotron`|`parakeet` — opt-in offline vi STT; + `LIVA_PARAKEET_MODEL_PATH`/`LIVA_PARAKEET_THREADS`), `LIVA_TTS_MODEL_PATH` (default `models/kokoro-v1.0.onnx`), `LIVA_LLM_MODEL_DIR`/`LIVA_LLM_N_CTX`/`LIVA_LLM_N_GPU_LAYERS`, `LIVA_DB_PATH`, `LIVA_VAULT_PATH`, `TELEGRAM_BOT_TOKEN`. `.env.example` (v30.0 overhaul) is current: the core reads only `LIVA_*`, while `AI_*` are UI-managed (`ApiManagementView.vue`) and legacy `NATIVE_*` are gone — for `LIVA_*` still trust the code (`main.rs`) as source of truth.
- Model weights (`*.onnx`, `*.gguf`) are gitignored and fetched out-of-band. `models/kokoro-v1.0.onnx` is absent by default, so TTS init fails until the model is supplied.
- `models/nemotron-asr` is a nested git repo with LFS (NOT a registered submodule) — it permanently shows as "modified content" in `git status`; leave it alone.

# Conventions

- NEVER run `git commit`, `git push`, or `git pull` autonomously — only when the user explicitly asks (per AGENTS.md).
- TS/Vue (enforced by ESLint + husky pre-commit): no `console.*`, no native `fetch` (use `safeFetch()`), no sync `fs*Sync`. Pre-commit runs `eslint --max-warnings 0` + `tsc --noEmit` on staged files, then an AI audit script that requires `.env` (bypass: `SKIP_AI_HOOK=1`).
- Docs, comments, and commit messages are often Vietnamese; console I/O assumes UTF-8 (`chcp 65001`).
