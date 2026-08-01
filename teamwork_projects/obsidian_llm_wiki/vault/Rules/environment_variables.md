---
title: "environment_variables"
tags:
  - liva/rule
author: "codex"
last_update: "2026-07-30T00:00:00+07:00"
severity: "CRITICAL"
scope: "all-agents"
---

# Rule: Environment Variables

## Rule statement

LIVA's active backend is the Unified Native Engine in Rust. Environment variables are operator
overrides read by Rust; there is no Node gateway `ConfigManager`, no boot-time `.env` scrubber and
no supported `liva_vault.json` credential flow.

Secrets must not be written to `.env`, `data/liva-config.json`, logs, generated diagnostics or
frontend state. Desktop credentials belong in Tauri Stronghold through a purpose-specific command.
Until every UI is wired to Stronghold, do not claim a credential is encrypted merely because a
vault primitive exists.

## Current authority

The complete, source-verified variable registry is:

- `docs/02-van-hanh/01-cau-hinh-va-bien-moi-truong.md`
- Rust readers under `liva-native-core/src/`
- security decisions in `docs/05-chat-luong/threat-model.md`

Do not copy the full list into Vault; a second list will drift.

## Critical variable families

| Family | Purpose | Safety rule |
|---|---|---|
| `LIVA_HOME` | writable user/resource root | operator override; do not derive secret paths from cwd |
| `LIVA_DB_PATH` | SQLite path override | must be explicit; warn about stray DBs, never auto-merge |
| `LIVA_DB_IN_MEMORY` | ephemeral DB mode | test/development; no durability promise |
| `LIVA_ENCRYPTION_KEY` | DB fact key override | secret; never log; Windows on-disk DB otherwise uses DPAPI device key |
| `LIVA_ENCRYPTION_KEY_OLD` | one-time rekey source | migration-only; remove after verified rekey |
| `LIVA_SERVER_HOST` / `LIVA_SERVER_PORT` | WebSocket bind | non-loopback is forbidden until authentication is configured |
| `LIVA_WS_ALLOWED_ORIGINS` | additional browser origins | exact origins only; not an authentication mechanism |
| `LIVA_MCP_AUTOEXEC` | widens external-tool execution | security-sensitive operator policy |
| Telegram token/allowlist variables | bot identity and callers | token is secret; empty allowlist stays fail-closed |

## Required behavior

1. Prefer config for non-secret product settings and Stronghold for credentials.
2. Keep `data/liva-config.json` secret-free; `get_config` must not become a secret export.
3. Do not print recovery/device keys to stderr or tracing.
4. Do not add a new environment variable without:
   - a Rust read site;
   - a default/failure policy;
   - documentation in the canonical operations registry;
   - tests for security-sensitive values.
5. Do not restore legacy `AI_PROVIDER`, `AI_API_KEY`, Zalo, Node gateway or Python-core variables
   unless a new implementation and approved contract explicitly require them.

## Known gap

The current UI still has credential paths that are not connected to Stronghold. Treat
`docs/05-chat-luong/threat-model.md` S0 as required work, not as an implemented guarantee.
