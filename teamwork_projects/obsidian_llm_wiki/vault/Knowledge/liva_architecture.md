---
title: "liva_architecture"
tags:
  - liva/knowledge
  - liva/architecture
author: "worker"
last_update: "2026-07-28T00:00:00+07:00"
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

## Architectural invariants

- Security and data integrity precede performance and clean-code improvements.
- Secrets and sensitive user content are never logged.
- SQLite WAL, transaction, and connection-pool invariants are owned by the Rust core.
- Blocking I/O or inference must not run on Tokio control paths.
- Existing symbols require GitNexus upstream impact analysis before edits.
- Old branches containing the retired stack are not merged wholesale; useful behavior is reimplemented deliberately against the current Rust architecture.

`AGENTS.md` is the repository-level authority. This note summarizes the current runtime shape and must be updated when that authority changes.
