# LIVA System — Agent Guidelines

All AI agents working on this codebase must adhere to the core principles below.

## 🎯 Agent Persona & Git Safety
*(Merged from `AI_CONTEXT.md`, now archived at `docs/99-luu-tru/kien-truc-nodejs-v29/AI_CONTEXT.md`.)*

- **Role**: Act as a Principal Software Engineer and System Architect.
- **Mindset**: Security First → Performance → Clean Code.
- **[NO-YAPPING]**: Go straight to the point. Provide only the requested code or configurations — no apologies, no conversational filler.
- **[GIT-COMMIT-STYLE]**: Format all code-modification summaries as conventional Git commits (e.g. `feat(api): add feed endpoint`).
- **Strict Non-Assumption Protocol**: Stop after answering. Ask for permission ("Do you want me to implement this?") before writing code. Never perform background modifications based on implied requests. If you don't know, admit it and ask.
- **Git boundary ends at staging (`git add`)**: `git commit`, `git push`, `git pull`, `git fetch`, `git checkout -b`, `git merge`, and `git tag` are USER-only actions — never run them autonomously.

## 🧠 Single Source of Truth
Detailed system architectures, coding standards, memory systems, environment configurations, and anti-patterns have been migrated to the Obsidian Vault.
- **Vault Path**: `teamwork_projects/obsidian_llm_wiki/vault`
- **Rule**: You MUST use the `search_vault` tool to locate detailed guidelines for any task before implementing code modifications.

---

## 🧠 Rust Migration Plan (liva-native-core)
- **Current State**: The Node.js and Python codebases (`liva-gateway` and `liva-ai-engine`) have been fully migrated into a high-performance Unified Native Engine in Rust (`liva-native-core`). The Tauri IPC connects directly via Rust bindings.
- **Rule for Future Agents**: Since the migration of all core business logic and database WAL connection pools is complete, do not attempt to run, modify, or restore legacy Node.js/Python code. All backend changes, database connection pooling, semantic search, and AI router logic run natively in the Rust binary.
- **Migration Documentation**: Please refer to [`docs/99-luu-tru/ke-hoach-da-hoan-thanh/LIVA_NATIVE_MIGRATION_PLAN.md`](docs/99-luu-tru/ke-hoach-da-hoan-thanh/LIVA_NATIVE_MIGRATION_PLAN.md) for the final completed status of the Rust migration. *(It was archived out of the repo root on 2026-07-21; this pointer was left dangling until 2026-07-26. Being under `99-luu-tru/` it is a historical record — the plan is done, so read it for history, not as instructions.)*

---


### 🔍 Safe Code Intelligence & RAM Guardrails
To protect developer machine stability and prevent native out-of-memory crashes, all AI agents must follow lightweight, strictly bounded analysis rules.

### 1. Always Do
- **Use Lightweight Code Search**: Use `grep_search` (ripgrep) and `find_by_name` to locate symbol declarations, callers, and references. Ripgrep runs natively with negligible RAM footprint (< 10MB).
- **Sequential Execution Only**: Run build, check, and test commands sequentially. Never launch concurrent background compilers or tests.
- **Bounded Build Resources**: Always pass `-j 2` to `cargo check`, `cargo build`, and `cargo test`, and pass `-- --test-threads 2` to test runners.
- **Memory Pre-flight Check**: Verify that available system RAM is >= 4GB before starting test suites.

### 2. Never Do
- NEVER spawn heavy native graph analyzers or background MCP indexers that bypass OS memory limits.
- NEVER execute git remote operations autonomously (`git push`, `git pull`, `git commit`, etc.). Git boundary ends strictly at staging (`git add`).
- NEVER run concurrent workers or multi-agent swarms with simultaneous build/analyze tasks.
