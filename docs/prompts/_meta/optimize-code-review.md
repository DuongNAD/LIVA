# Meta-Prompt: Optimize Code Review Prompt
Target Output: `docs/prompts/code-review-prompt.md`

You are the prompt architect for the LIVA system. Your task is to generate and optimize the `code-review-prompt.md` file. This generated prompt will be used by other AI agents to conduct high-fidelity codebase reviews of the LIVA project (Rust `liva-native-core` + Vue 3 `liva-ui` + Tauri `liva-desktop`).

## Instructions for Writing the Output Prompt

The generated `code-review-prompt.md` must instruct the reviewing AI agent to perform the following checks:

1. **Verify Tech Stack Compliance (enforced rules)**:
   - TypeScript/Vue (ESLint-enforced): no `console.*` (`no-console` is an error); native `fetch` is banned — HTTP requests must use the `safeFetch` wrapper from `liva-ui/src/utils/fetch.ts`; synchronous `fs.*Sync` file I/O is banned — use `fs.promises`.
   - Rust: the Cargo workspace must keep building (`cargo build`) and the `liva-native-core` test suite must keep passing (`cargo test`).

2. **Check Runtime Protection**:
   - Verify that CPU-heavy actions (audio/VAD inference, parsing, diffing) run in the Rust core — inside async tasks or `spawn_blocking`, without blocking the Tokio runtime — and never on the Vue UI thread.

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

5. **Protect Modular Skill Runbooks**:
   - Ensure files under `.claude/skills/` and the vault's `Skills/` notes are not incorrectly flagged as dead code or violating logging standards. They are procedural runbooks.

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
   - The review MUST append a strict XML block at the very end of the report representing the commit decision. It must be the LAST block of the output — the pre-commit parser (`scripts/ai-pre-commit.js`) trusts only the final `<audit_result>` block and fails closed when it is missing or malformed:
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
