# Meta-Prompt: Optimize Architecture Review Prompt
Target Output: `docs/04-quy-trinh/prompts/architecture-review.md`

You are the prompt architect for the LIVA system. Your task is to generate and optimize the `architecture-review.md` file. This generated prompt will be used by other AI agents to perform full-spectrum architectural audits of the LIVA codebase (Rust `liva-native-core` + Vue 3 `liva-ui` + Tauri `liva-desktop`).

## Instructions for Writing the Output Prompt

The generated `architecture-review.md` must instruct the AI agent to:

1. **Phase 1: Live Data Gathering**:
   - Instruct the auditor to inspect the `liva-ui` ↔ embedded native core WebSocket surface (`ws://localhost:8002`), model configuration, and SQLite state.
   - Run PowerShell queries to check SQLite vector schema indices (`sqlite-vec`), WAL configurations, and commit log status (`git log -n 5`).
   - Query GitNexus index status using queries if index is fresh.

2. **Phase 2: Deep Architecture Analysis**:
   - Evaluate the decoupled embedding design (in-process `llama.cpp` embeddings via `llm:embed`, separated from text generation).
   - Evaluate sequential hot-swap logic to avoid OOM under consumer GPUs.
   - Evaluate async runtime safety (no blocking I/O or long synchronous CPU work inside Tokio async tasks; no heavy work on the Vue UI thread).
   - Evaluate the native voice pipeline (Nemotron ASR, Piper TTS, Silero VAD, and wake-word gating inside the Rust core).

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
   - Save the audit report to `docs/03-danh-gia/bao-cao/architecture-review/architecture-review-report-{YYYY-MM-DD}.md`.
   - Provide an "Architecture Vibe Rating" and the "Critical 3" upgrade candidates.
   - Outline clear recommendations for the next development sprints.
