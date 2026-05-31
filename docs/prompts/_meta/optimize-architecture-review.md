# Meta-Prompt: Optimize Architecture Review Prompt
Target Output: `docs/prompts/architecture-review.md`

You are the prompt architect for the LIVA system. Your task is to generate and optimize the `architecture-review.md` file. This generated prompt will be used by other AI agents to perform full-spectrum architectural audits of the LIVA codebase.

## Instructions for Writing the Output Prompt

The generated `architecture-review.md` must instruct the AI agent to:

1. **Phase 1: Live Data Gathering**:
   - Instruct the auditor to inspect Gateway ↔ Tauri UI websocket connections, model endpoints, and SQLite state.
   - Run PowerShell queries to check SQLite vector schema indices (`sqlite-vec`), WAL configurations, and commit log status (`git log -n 5`).
   - Query GitNexus index status using queries if index is fresh.

2. **Phase 2: Deep Architecture Analysis**:
   - Evaluate the decoupled CPU embedding design (EmbeddingWorker on ONNX).
   - Evaluate sequential hot-swap logic to avoid OOM under consumer GPUs.
   - Evaluate event loop safety (ensuring no synchronous I/O or CPU operations >10ms on main thread).
   - Evaluate the hybrid Voice and VAD pipelines (Silero ONNX WASM wake word in UI offloading).

3. **Phase 3: Architecture Health Score & Ledger Logging**:
   - Instruct the auditor to compute an **Architecture Health Score** out of 100 based on standard metrics:
     - Deduct 5 points per "God Component" (Cognitive complexity >= 35).
     - Deduct 5 points per blocker bug or compile error.
     - Deduct 2 points per warning or minor standard violation.
   - The output MUST include a JSON payload to append to `tech-debt-ledger.json`:
     ```json
     {
       "timestamp": "2026-05-31T13:00:00Z",
       "score": 85,
       "godComponentsCount": 0,
       "violationsCount": 3,
       "codeRedTriggered": false
     }
     ```
   - **Code Red Trigger**: If the score is less than 70, the prompt must instruct the auditor to issue a prominent "CODE RED" status warning in the report, blocking feature expansion.

4. **Phase 4: Generate the Architectural Audit Report**:
   - Save the audit report to `docs/reports/architecture-review/architecture-review-report-{YYYY-MM-DD}.md`.
   - Provide an "Architecture Vibe Rating" and the "Critical 3" upgrade candidates.
   - Outline clear recommendations for the next development sprints.
