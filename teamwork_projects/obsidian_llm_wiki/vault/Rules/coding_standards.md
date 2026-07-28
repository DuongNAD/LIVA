---
title: "coding_standards"
tags:
  - liva/rule
author: "worker"
last_update: "2026-07-28T00:00:00+07:00"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Coding Standards

## Current architecture

LIVA's application core is the Rust `liva-native-core`, called directly by Tauri. The former Node.js gateway and Python AI engine are retired and must not be restored. `liva-voice` remains an active, separate Python voice service. The UI is Vue 3 with TypeScript.

## Required priorities

Apply this order to design and review: security and data integrity, performance, then clean code.

### Rust and Tauri

- Do not block the Tokio runtime with synchronous filesystem, network, database, or inference work. Use asynchronous APIs or `spawn_blocking` for bounded CPU/blocking work.
- Propagate structured errors; do not `unwrap()` or `expect()` on user-controlled, network, database, or IPC paths.
- Validate every Tauri command input at the Rust boundary. Never place secrets, raw credentials, or sensitive user content in logs.
- Keep ownership and concurrency explicit. Avoid global mutable state and hold locks for the shortest practical duration.
- Preserve database WAL and transaction invariants. Any schema or persistence change needs rollback/error-path tests.

### Vue 3 and TypeScript

- Production TypeScript uses `unknown` plus narrowing instead of `any`.
- Validate data that crosses IPC, storage, or network boundaries.
- For high-frequency streaming output, prefer `shallowRef` plus `triggerRef` over deep reactive proxies.
- Timers and subscriptions in cached components must start in `onActivated` and stop in `onDeactivated`.

### Node.js maintenance scripts

- Repository scripts must use asynchronous filesystem APIs and cross-platform Node paths.
- Scripts must have deterministic non-zero exit codes for invalid state and machine-readable output when used by CI.

### Python voice service

- Python changes are limited to the active `liva-voice` service; they must not recreate legacy backend responsibilities.
- Keep inference off request/control loops and clean up model, audio, and temporary-file resources deterministically.

### Windows and PowerShell

- Local examples must use PowerShell syntax such as `$env:NAME`.
- Product code and npm scripts must remain cross-platform; do not bake Windows-only separators into runtime paths.

## Change safety

Before changing an existing symbol, use GitNexus upstream impact analysis. Warn and stop for approval on HIGH or CRITICAL risk. After implementation, run focused tests, relevant repository gates, and GitNexus change detection. Git operations stop at staging for AI agents.
