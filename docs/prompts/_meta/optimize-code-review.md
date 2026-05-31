# Meta-Prompt: Optimize Code Review Prompt
Target Output: `docs/prompts/code-review-prompt.md`

You are the prompt architect for the LIVA system. Your task is to generate and optimize the `code-review-prompt.md` file. This generated prompt will be used by other AI agents to conduct high-fidelity codebase reviews of the LIVA project.

## Instructions for Writing the Output Prompt

The generated `code-review-prompt.md` must instruct the reviewing AI agent to perform the following checks:

1. **Verify Tech Stack Compliance**:
   - Strictly check for banned libraries (e.g., `axios`, `puppeteer`, `@xenova/transformers`, `@lancedb/lancedb`, synchronous file I/O `fs.*Sync` on the main Event Loop, `request`, etc.).
   - Verify that native `fetch` uses the `safeFetch` wrapper from `src/utils/HttpClient.ts`.
   - Ensure proper use of `pino` logger instead of `console.log`.

2. **Check Event Loop Protection**:
   - Verify that any CPU-heavy actions (AST mutations via ts-morph, massive JSON parsing, Silero ONNX VAD inference) are run inside `node:worker_threads` (e.g. `ASTWorker.ts`, `VADWorker.ts`).

3. **Check TypeScript Conventions & Zero `any` Policy**:
   - Scan for forbidden use of `any` types.
   - Verify proper schemas (Zod) are used for boundary JSON data.
   - Check that approved workarounds for `any` (like Sequelize raw where clauses or external library types) are commented explicitly.

4. **Enforce Complexity Zones & God Components (AST-based)**:
   - Check cyclomatic/cognitive complexity (nested loops, excessive conditional branching):
     - **Safe**: Complexity < 15.
     - **Monitor**: 15–24.
     - **RED**: 25–34 (Needs decomposition plan).
     - **God Component**: >= 35 (Decomposition is mandatory before new features).

5. **Protect Modular `.skills/` Runbooks**:
   - Ensure files under `.skills/` are not incorrectly flagged as dead code or violating logging standards. They are procedural runbooks.

6. **Actionable Payloads Requirement**:
   - If refactoring is recommended (e.g. removing unused imports or fixing `any` casts), the review MUST include a machine-readable JSON array of type `FileMutation[]` wrapped inside a code block:
     ```json
     [
       {
         "type": "modify",
         "filePath": "src/path/to/file.ts",
         "code": "<<<< SEARCH\n[Old code]\n====\n[New code]\n>>>> REPLACE"
       }
     }
     ```

7. **AI Pre-Commit Guardrail (Audit XML Result)**:
   - The review MUST append a strict XML block at the very end of the report representing the commit decision:
     ```xml
     <audit_result>
     {
       "block": true or false,
       "reason": "Specify reason if blocked, else empty"
     }
     </audit_result>
     ```
   - Set `"block": true` if there are severe violations (like `any` cast, cyclomatic complexity >= 35, or banned imports).

8. **Windows & PowerShell Environment Conventions**:
   - Check local CLI scripts for Windows compatibility (backslash paths, PowerShell `$env:`, `;` chaining, no Bash-style syntax).
