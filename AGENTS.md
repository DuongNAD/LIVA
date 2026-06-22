# LIVA System — Agent Guidelines

All AI agents working on this codebase must adhere to the core principles below.

## 🧠 Single Source of Truth
Detailed system architectures, coding standards, memory systems, environment configurations, and anti-patterns have been migrated to the Obsidian Vault.
- **Vault Path**: `teamwork_projects/obsidian_llm_wiki/vault`
- **Rule**: You MUST use the `search_vault` tool to locate detailed guidelines for any task before implementing code modifications.

---

## 🔍 GitNexus Code Intelligence Rules
This project is indexed by GitNexus as **LIVA**. Use GitNexus MCP tools for safe navigation.

### 1. Always Do
- **Run Impact Analysis**: Run `gitnexus_impact({target: "symbolName", direction: "upstream"})` before editing any symbol. Report direct callers, affected processes, and risk level.
- **Run Change Detection**: Run `gitnexus_detect_changes()` before staging/committing to verify only expected symbols and flows are affected.
- **Warn on High Risk**: Warn the user if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- **Query Concept**: Use `gitnexus_query({query: "concept"})` when exploring unfamiliar code instead of raw grepping.
- **Query Context**: Use `gitnexus_context({name: "symbolName"})` to get a 360-degree view of a symbol's callers, callees, and flows.

### 2. Never Do
- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — always use `gitnexus_rename` (which understands the call graph).
- NEVER commit or execute git remote operations autonomously (`git push`, `git pull`, `git commit`, etc.).

### 3. Keep Index Fresh
- Re-run `npx gitnexus analyze --embeddings` (or without `--embeddings` if not present) after changes to update the GitNexus code graph.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **LIVA** (11253 symbols, 26337 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.

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
