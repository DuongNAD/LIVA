---
description: Perform a high-fidelity code review optimized for Vibe Coding
---

# Code Review Workflow

1. **Read the Code Review Prompt**:
   Review the detailed instructions in `docs/prompts/code-review-prompt.md`.

2. **Analyze the Codebase**:
   Focus on the following areas:
   - **Correctness**: Logical bugs or edge cases
   - **Security**: Authentication guards and input validation
   - **AI Context Readiness**: JSDoc, file size (< 500 lines), and naming clarity
   - **Dead Code**: Unused imports or orphaned files (protecting all `.skills/` files)

3. **Check Vibe Coding Compliance**:
   - Identify "God Components" (files > 500 lines) and provide a deconstruction plan
   - Check for `as any` type assertions and suggest proper type augmentations
   - Verify module separation (logic in services, endpoints in routes)
   - Protect all procedural `.skills/` runbooks (capacitor-ops, clerk-sync-tracer, project-migrations, security-isolation) from being flagged as dead code or console violations

4. **Generate the Report**:
   Save output to `docs/reports/code-review/code-review-report-{YYYY-MM-DD}.md`.
   - Use today's date in the filename
   - If multiple reports on same day, append time: `{YYYY-MM-DD}-{HH-MM}.md`

5. **Apply Retention Policy**:
   Keep only the **5 most recent** reports in `docs/reports/code-review/`.
   Move older reports to `docs/reports/archive/`.

6. **Produce the Vibe Report & Recommended Actions**:
   Include a grading matrix (1-10) for:
   - Security
   - Stability
   - Maintainability
   - Overall Vibe Score
   
   Ensure each item in the Recommended Actions table specifies the optimal **Execution Environment** (IDE vs. Application vs. CLI) according to the 2.0 decision matrix.