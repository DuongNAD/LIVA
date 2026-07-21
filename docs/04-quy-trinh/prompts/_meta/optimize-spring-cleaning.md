# Meta-Prompt: Optimize Spring Cleaning Prompt
Target Output: `docs/04-quy-trinh/prompts/spring-cleaning-prompt.md`

You are the prompt architect for the LIVA system. Your task is to generate and optimize the `spring-cleaning-prompt.md` file. This generated prompt will be used by other AI agents to identify and clean up dead code, unused dependencies, and orphaned files in the codebase.

## Instructions for Writing the Output Prompt

The generated `spring-cleaning-prompt.md` must instruct the cleaning AI agent to perform the following checks:

1. **Detect Unused Imports and Dead Code**:
   - Spot typescript files with exported symbols that are never imported elsewhere.
   - Scan for unused variables, functions, and imports.

2. **Orphaned and Legacy Files**:
   - Spot outdated files (such as files remaining from Electron migration, temporary folders, or duplicate configurations).
   - **CRITICAL SAFEGUARD**: Never delete or flag files inside `.skills/` (e.g. `capacitor-ops`, `clerk-sync-tracer`, `project-migrations`, `security-isolation`). These are modular runbooks and are completely exempt from dead-code/orphan checks.

3. **Verify Configuration Integrity**:
   - Ensure the correct `.env` files are used and no secret keys are checked into Git.
   - Verify package configurations (like `package.json`, `tsconfig.json`, `Cargo.toml`).

4. **Actionable Payloads for Cleanup**:
   - The cleaning prompt MUST instruct the AI to output a machine-readable JSON array of type `FileMutation[]` containing modify/delete instructions for the candidates identified as safe to remove, using the `<<<< SEARCH ==== >>>> REPLACE` format for modifications:
     ```json
     [
       {
         "type": "delete",
         "filePath": "src/legacy/file.ts"
       },
       {
         "type": "modify",
         "filePath": "src/core/active.ts",
         "code": "<<<< SEARCH\nimport { unused } from './stale';\n====\n>>>> REPLACE"
       }
     ]
     ```

5. **PowerShell-Compatible Command Checks**:
   - Recommend using standard PowerShell git log/blame command sequences for checking commit velocity, staleness, and ownership rather than hypothetical bash tools.

6. **Generate Report Format**:
   - Save output to `docs/03-danh-gia/bao-cao/spring-cleaning/spring-cleaning-report-{YYYY-MM-DD}.md`.
   - List potential candidates for deletion or optimization, indicating risks and the recommended execution command. Include the JSON mutations block at the end.
