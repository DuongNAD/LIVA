---
title: "Cơ sở tri thức — con trỏ tới Obsidian vault"
updated: 2026-07-21
commit: a6c735c
status: index
owns: []
covers: []
---
# Knowledge Base — Single Source of Truth

The LIVA knowledge base (Knowledge / Rules / Skills / Templates) lives in the Obsidian vault:

- **Path**: [`teamwork_projects/obsidian_llm_wiki/vault/`](../../teamwork_projects/obsidian_llm_wiki/vault/)

The copies that used to live under `docs/Knowledge/`, `docs/Rules/`, `docs/Skills/`, and `docs/Templates/` were exact duplicates and have been removed. Do not re-create them — edit the vault instead.

AI agents access the vault through the Obsidian LLM Wiki MCP server (tools: `read_markdown`, `search_vault`; see `teamwork_projects/obsidian_llm_wiki/src/server.ts`). The vault path is configured via `LIVA_VAULT_PATH` in [`.env.example`](../../.env.example).
