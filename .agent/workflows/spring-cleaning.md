---
description: Run a Spring Cleaning analysis and generate a timestamped report
---

# Spring Cleaning Workflow

1. **Read the Spring Cleaning Prompt**:
   Review the instructions in `docs/prompts/spring-cleaning-prompt.md`.

2. **Execute the Analysis**:
   Execute the instructions in `docs/prompts/spring-cleaning-prompt.md`.
   - Protect all modular skills inside `.skills/` (capacitor-ops, clerk-sync-tracer, project-migrations, security-isolation) and never delete or flag their internal helper scripts, resources, or examples.
   - Run standard PowerShell terminal commands for Git log/blame staleness verification instead of hypothetical MCP tools.

3. **Generate the Report**:
   Save output to `docs/reports/spring-cleaning/spring-cleaning-report-{YYYY-MM-DD}.md`.
   - Use today's date in the filename
   - If multiple reports on same day, append time: `{YYYY-MM-DD}-{HH-MM}.md`

4. **Apply Retention Policy**:
   Keep only the **3 most recent** reports in `docs/reports/spring-cleaning/`.
   Move older reports to `docs/reports/archive/`.

5. **Report Completion**:
   Summarize findings and confirm report location to the user.