---
title: "commands_reference"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-07-28T00:00:00+07:00"
---

# Knowledge: Commands Reference

Run commands from the repository root unless a command includes `-w` or an explicit directory.

## Workspace

| Command | Purpose |
|---|---|
| `npm install` | Install all npm workspace dependencies |
| `npm run dev` | Start the supported local LIVA development stack |
| `npm run build:ui` | Type-check and build the Vue UI |
| `npm run build:desktop` | Build the UI and desktop web assets |
| `npm run doctor` | Validate local model configuration |
| `npm run e2e:gateway` | Run the native gateway end-to-end harness |

## Quality gates

| Command | Purpose |
|---|---|
| `npm run test -w liva-ui` | Run Vue/Vitest unit tests |
| `npm run test:coverage -w liva-ui` | Run Vue tests with coverage |
| `cargo test --manifest-path liva-native-core/Cargo.toml` | Run native-core Rust tests |
| `cargo check --all-targets --manifest-path liva-native-core/Cargo.toml` | Compile-check native-core targets |
| `npm run typecheck -w obsidian-llm-wiki` | Type-check the Obsidian MCP server |
| `npm run test -w obsidian-llm-wiki` | Run Obsidian MCP tests |
| `npm run validate -w obsidian-llm-wiki` | Validate vault frontmatter and structure |
| `npm run test:skills` | Run negative and positive tests for skill governance |
| `npm run skills:audit` | Audit Claude/Codex skills and active vault knowledge |
| `npm run docs:check` | Check documentation structure |
| `npm run docs:cite` | Check documentation citations |

## Code intelligence

| Command | Purpose |
|---|---|
| `node .gitnexus/run.cjs status` | Check index freshness |
| `node .gitnexus/run.cjs query "<concept>"` | Explore unfamiliar execution flows |
| `node .gitnexus/run.cjs context <symbol>` | Inspect callers, callees, and processes |
| `node .gitnexus/run.cjs impact <symbol> --direction upstream` | Assess blast radius before an edit |
| `node .gitnexus/run.cjs detect-changes --scope unstaged` | Verify affected symbols and flows |
| `npm run gitnexus:index` | Rebuild the index with PDG and embeddings |

AI agents follow the repository Git boundary: staging is permitted; commit, merge, branch deletion, and remote operations remain user-only.
