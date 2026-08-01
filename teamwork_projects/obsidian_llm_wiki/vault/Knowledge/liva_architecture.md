---
title: "liva_architecture"
tags:
  - liva/knowledge
  - liva/architecture
author: "codex"
last_update: "2026-07-31T06:00:00+07:00"
---

# Knowledge: LIVA System Architecture

## Runtime boundary

LIVA uses a unified Rust engine in `liva-native-core`. Tauri calls the native engine directly; the retired Node.js gateway and Python AI backend are historical code and must not be restored. The separate `liva-voice` Python service remains active for voice-specific workloads.

## Main components

- `liva-native-core`: AI routing, native tools, persistence, integrations, WebSocket behavior, wake/voice coordination, and security-sensitive backend logic.
- `liva-desktop`: Tauri desktop shell and native command boundary.
- `liva-ui`: Vue 3 and TypeScript user interface.
- `packages/liva-common`: shared TypeScript contracts used by supported front-end workspaces.
- `mobile_client`: mobile client.
- `teamwork_projects/obsidian_llm_wiki`: local MCP server and Obsidian knowledge vault.
- `liva-voice`: isolated Python voice service; it does not own general backend logic.

## Data and control flow

1. The desktop application starts the Tauri/Rust runtime.
2. UI requests cross the Tauri IPC boundary and are validated before reaching native capabilities.
3. The native core coordinates local/cloud model access, tools, persistence, integrations, and streaming results.
4. High-frequency results return to Vue through bounded streaming/event paths; the UI avoids deep reactive work per token.
5. Obsidian knowledge is accessed through the local MCP server. Agents call `search_vault` before changes governed by vault rules.

## Command and artifact boundaries

- Every external command entry is assigned a `CommandPrincipal`. Tauri derives privileged identity
  from the exact window label. WebSocket defaults to `WebSocketRemote`; a client that sends
  `?principal=widget|dashboard` is rejected with HTTP 403. Widget/dashboard may obtain a 256-bit,
  30-second, single-use session ticket only through their trusted Tauri capability; the server
  stores only its SHA-256 digest and derives the WebSocket principal on consume. Command
  authorization is fail-closed before domain handlers run.
- Tauri app commands are registered in `AppManifest`. Widget, dashboard and setup each have a
  separate capability file; vault/dialog/process permissions are not shared with widget/setup.
- Router GGUF, mmproj and vec0 are canonicalized under an approved root and must match SHA-256 from
  the manifest embedded in the Rust binary before entering a native parser or extension loader.
- Privileged WebSocket identity is loopback-only and session-bound. Setup/unknown windows,
  expired/replayed/duplicate tickets, self-declared principals and non-loopback session use are
  rejected.
- Tauri CSP allows only bundled self scripts/styles and forbids objects, base URI changes and
  framing. Shipped static HTML has no inline script, style block or style attribute.

## Canonical runtime documents

- `docs/03-he-thong-con/persistence.md`: data root, 20-table schema v5, migration, durability and
  data lifecycle.
- `docs/05-chat-luong/threat-model.md`: trust assumptions, encryption/keystore coverage,
  WebSocket/Tauri/MCP boundaries and hardening order.
- `docs/06-ke-hoach/roadmap.md`: program-level milestones and acceptance gates.

The former `docs/01-ban-ve/07-tang-du-lieu-va-bao-mat.md` is a frozen historical snapshot.

## Architectural invariants

- Security and data integrity precede performance and clean-code improvements.
- Secrets and sensitive user content must never be logged. Recovery/device keys do not use stderr;
  desktop credentials use purpose-scoped Stronghold commands.
- SQLite WAL, transaction, and connection-pool invariants are owned by the Rust core.
- Blocking I/O or inference must not run on Tokio control paths.
- Existing symbols require GitNexus upstream impact analysis before edits.
- Old branches containing the retired stack are not merged wholesale; useful behavior is reimplemented deliberately against the current Rust architecture.

`AGENTS.md` is the repository-level authority. This note summarizes the current runtime shape and must be updated when that authority changes.
