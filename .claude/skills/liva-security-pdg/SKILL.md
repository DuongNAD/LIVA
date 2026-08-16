---
name: liva-security-pdg
description: Audit source code for vulnerabilities using Program Dependence Graph (PDG) taint tracking, control-dependence analysis, and source-to-sink verification. Use when performing security audits, analyzing taint flows, reviewing pull requests for exploits, checking sanitization barriers, or verifying secure coding compliance.
---

# LIVA Security PDG

## Workflow

1. **Vulnerability Surface Identification**:
   - Query entry points (HTTP handlers, IPC commands, WebSocket listeners, CLI input parsers) using GitNexus code graph:
     - `gitnexus_query({query: "untrusted input handler"})`
   - Retrieve full caller/callee context and execution flows via `gitnexus_context({name: "target_symbol"})`.

2. **PDG Taint Flow & Source-to-Sink Analysis (`explain`)**:
   - Inspect potential taint flows from untrusted input sources (user payload, HTTP query, query params, external files) to critical security sinks (SQL query execution, shell execution, filesystem I/O, raw HTML rendering):
     - `gitnexus_explain({target: "target_file_or_symbol"})`
   - Identify unescaped or unvalidated data paths reaching sinks without passing through effective sanitization barriers.

3. **Control & Data Dependence Tracing (`pdg_query`)**:
   - Trace control conditions and guard clauses governing sink execution:
     - `gitnexus_pdg_query({mode: "controls", target: "sink_symbol"})`
   - Trace reaching definitions and variable mutation paths across procedural boundaries:
     - `gitnexus_pdg_query({mode: "flows", target: "source_function", variable: "untrusted_var"})`

4. **Blast Radius & Upstream Impact Assessment (`impact`)**:
   - Run upstream impact analysis on the vulnerable symbol:
     - `gitnexus_impact({target: "vulnerable_symbol", direction: "upstream"})`
   - Classify risk level (`LOW`, `MEDIUM`, `HIGH`, `CRITICAL`). If risk is `HIGH` or `CRITICAL`, warn the user explicitly before proposing remediation.

5. **Remediation & Secure Patch Formulation**:
   - Formulate a minimal, defense-in-depth security patch:
     - SQL Injection -> Convert string concatenation to parameterized queries (`?1`, `$1`).
     - Path Traversal -> Apply 3-layer sandbox (`resolve_path` with canonical ancestor containment).
     - Command Injection -> Replace shell invocation (`sh -c`) with explicit vector arguments (`Command::new().args([...])`).
     - Deserialization / Injection -> Enforce strict JSON Schema type validation and length limits.
   - Run `gitnexus_detect_changes()` to verify that the patch affects only the intended symbols without introducing unintended regressions.

6. **Audit Dossier & Obsidian Persistence**:
   - Persist the security audit report into `teamwork_projects/obsidian_llm_wiki/vault/Knowledge/Security - <Vulnerability_Audit_Title>.md` using `write_markdown`.
   - Adhere strictly to the Obsidian frontmatter standard (`title`, `tags: [liva/knowledge, liva/security, audit/pdg]`, `author: "codex"`, `last_update`).

## Platform Constraints

- **Execution Mode**: Diagnostic & Advisory. Taint tracing and PDG queries run automatically. Code modifications operate strictly under two-phase confirmation with dry-run diffs.
- **Tool Dependencies**: Requires `gitnexus` MCP server (`explain`, `pdg_query`, `impact`, `detect_changes`, `query`, `context`) and `obsidian` MCP server (`write_markdown`, `search_vault`).
- **Graph Prerequisite**: Requires an active GitNexus PDG index (`node .gitnexus/run.cjs analyze --pdg`).
- **Git Invariant**: Modifications staged via `git add` only; all commit and push operations remain strictly user-controlled.

## Stop Conditions

Stop and report immediately when:
- GitNexus returns a CRITICAL risk blast radius affecting core authentication, cryptographic signing, or database transaction kernels.
- A proposed remediation would break public API contracts or Tauri IPC schemas without a migration plan.
- The GitNexus PDG index is stale or corrupted and needs re-indexing (`node .gitnexus/run.cjs analyze --pdg`).
- Unmasked credentials, master keys, or live user passwords are discovered in source code.
