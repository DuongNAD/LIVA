---
name: liva-code-refactor
description: Perform safe automated code refactoring, dead code pruning, and architectural migrations guided by GitNexus Code Graph and Program Dependence Graph (PDG) blast radius analysis. Use when refactoring functions, extracting abstractions, eliminating technical debt, replacing unsafe unwrap calls, or restructuring module hierarchies.
---

# LIVA Code Refactor

## Workflow

1. **Refactoring Target & Scope Definition**:
   - Ingest target symbol, module, or pattern to refactor (e.g. migrating synchronous I/O to async, removing deprecated wrappers, eliminating `unwrap()` calls).
   - Locate definitions and call sites using `gitnexus_query` and `gitnexus_context({name: "target_symbol"})`.

2. **PDG Taint & Control-Dependence Analysis**:
   - Trace control conditions and guard clauses governing the target symbol:
     - `gitnexus_pdg_query({mode: "controls", target: "target_symbol"})`
   - Trace reaching definitions and data flows across procedural boundaries:
     - `gitnexus_pdg_query({mode: "flows", target: "target_symbol", variable: "var_name"})`

3. **Blast Radius & Upstream Impact Assessment**:
   - MUST run upstream impact analysis before touching any code:
     - `gitnexus_impact({target: "target_symbol", direction: "upstream"})`
   - Evaluate risk classification (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`).
   - If risk is `HIGH` or `CRITICAL`, present detailed caller impact report and request explicit operator confirmation before generating patches.

4. **Safe Refactoring Patch Formulation**:
   - Apply structured refactoring recipes:
     - **Dead Code Elimination**: Remove unused structs/functions and update module exports cleanly.
     - **Error Handling Hardening**: Replace `.unwrap()` / `.expect()` with structured `Result<T, E>` / `?` propagation.
     - **Async Runtime Optimization**: Move blocking CPU/sync I/O into `tokio::task::spawn_blocking`.
     - **Symbol Renaming**: Use `gitnexus_rename` to update all call graph sites atomically.
   - Enforce minimal diffs without reformatting unrelated lines.

5. **AST Verification, Build Validation & Regression Testing**:
   - Create rollback backup snapshot before writing changes (`BackupGuard`).
   - Run compilation check: `cargo check --workspace` / `npm run build:ui`.
   - Run co-located unit and integration tests: `cargo test` / `npm test`.
   - If tests fail, automatically rollback and diagnose regression cause.
   - Run `gitnexus_detect_changes()` to ensure only intended symbols were modified.

6. **Refactoring Log & Obsidian Documentation**:
   - Record completed refactoring report into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Refactor - <Task_Title>.md` via `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/refactor, code/graph]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Two-Phase Confirmation for code modifications; automated analysis for graph and PDG queries.
- **Tool Dependencies**: Requires `gitnexus` MCP server (`impact`, `pdg_query`, `context`, `detect_changes`, `rename`), local compiler toolchain (`cargo`, `npm`), and `obsidian` MCP.
- **Git Invariant**: AI agent staging ends at `git add`; commits, merges, and remote operations are strictly user-only.

## Stop Conditions

Stop and report immediately when:
- GitNexus impact analysis returns a CRITICAL risk level affecting database WAL transactions or IPC authorization kernels without an approved migration plan.
- The refactored code introduces compilation errors or broken test suites that cannot be resolved cleanly in 2 iterations.
- GitNexus PDG index is stale or corrupted and needs re-indexing.
