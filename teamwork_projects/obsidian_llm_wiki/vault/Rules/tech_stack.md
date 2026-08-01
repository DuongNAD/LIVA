---
title: "tech_stack"
tags:
  - liva/rule
author: "codex"
last_update: "2026-07-31T07:35:00+07:00"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Tech Stack

## Rule statement

LIVA uses one active backend: `liva-native-core` in Rust. Tauri v2 embeds/calls that core directly.
Do not restore general backend behavior in Node.js or Python.

## Current stack

| Layer | Allowed current technology |
|---|---|
| Native backend | Rust stable, Tokio, Axum/WebSocket, Serde |
| Persistence | `rusqlite` + `r2d2`, SQLite WAL, FTS5, `sqlite-vec` |
| Local inference | `llama-cpp-2`/GGUF and ONNX Runtime where implemented |
| Desktop | Tauri v2 Rust host + system WebView |
| UI | Vue 3, TypeScript, Vite |
| Shared frontend contracts | `packages/liva-common` |
| Voice exception | `liva-voice` Python service only for its existing voice-specific boundary |
| Tooling | Node.js/npm for UI, docs, build and repository scripts—not backend business logic |
| Tests | `cargo test` for Rust; Vitest for Vue and Obsidian MCP; Node test runner for repository scripts |

## Architectural bans

- No resurrection of any retired Node/Python general-backend package.
- No `node:sqlite`, Node event-loop database owner or Node credential vault.
- No Python implementation of general AI routing, persistence or command handling.
- No second writable database owner beside the Rust core.
- No blocking inference or unbounded filesystem/network work on Tokio control paths.
- No concurrent heavyweight model load without an explicit VRAM/resource design.
- No raw native-extension/model load from untrusted cwd/search paths in production.
- No claim that `evolution::Sandbox` is secure isolation; it is currently a host `cargo test` runner.
- No copying a live WAL database file as a backup; use SQLite backup API or `VACUUM INTO`.
- No release with a known Rust/npm vulnerability in either lockfile; temporary upstream git pins must name the exact commit and their removal condition.

## Frontend rules

- Use the established platform/gateway adapters instead of inventing a parallel transport.
- Do not place API secrets in Vue state, config JSON or gateway messages.
- Keep high-frequency token/audio paths bounded and avoid deep reactive work per frame/token.
- New Tauri permissions must be scoped to the window and command that needs them.

## Verification

- Code architecture: GitNexus query/context/impact before symbol changes.
- Rust: format, targeted tests, then workspace-appropriate clippy/test.
- Supply chain: `cargo audit`, full `npm audit --audit-level=high`, then runtime-only npm audit.
- Docs: `npm run docs:check`, inventory/capability checks and Vault validation.
- Canonical architecture: `docs/01-kien-truc/cognitive-runtime.md`,
  `docs/03-he-thong-con/persistence.md` and `docs/05-chat-luong/threat-model.md`.
