---
description: Perform a full-spectrum architectural audit using live MCP data
---

# Architecture Review Workflow

1. **Read the Architecture Review Prompt**:
   Review the detailed instructions in `docs/prompts/architecture-review.md`.

2. **Execute Phase 1 — Live Data Gathering**:
   Before any analysis, execute all MCP queries defined in the prompt's Phase 1 section:
   - Database state via `mcp_postgres_query`
   - Queue health via `mcp_redis_list`
   - Commit velocity via direct PowerShell terminal `git log` command
   - Open issues via `call_mcp_tool` wrapper (github-mcp-server)

3. **Execute Phase 2 — Architectural Analysis**:
   Work through each architectural dimension (4.1–4.9) using Phase 1 evidence.
   - Consult modular `.skills/` runbooks (capacitor-ops, clerk-sync-tracer, project-migrations, security-isolation)
   - Cross-reference `docs/architecture/` files for staleness

4. **Generate the Report**:
   Save output to `docs/reports/architecture-review/architecture-review-report-{YYYY-MM-DD}.md`.
   - Use today's date in the filename
   - If multiple reports on same day, append time: `{YYYY-MM-DD}-{HH-MM}.md`

5. **Apply Retention Policy**:
   Keep only the **3 most recent** reports in `docs/reports/architecture-review/`.
   Move older reports to `docs/reports/archive/`.

6. **Report Completion**:
   Summarize findings, the Architecture Vibe Rating, and the Critical 3 to the user.
   Confirm report location.
