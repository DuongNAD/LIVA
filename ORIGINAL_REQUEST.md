# Original User Request

## Initial Request — 2026-06-08T01:11:16+07:00

Implement and integrate key architectural upgrades into the LIVA engine, focusing on optimizing compute efficiency (Speculative Decoding and Persistent Prompt Caching) and enhancing memory retrieval and compaction (Hybrid Query Routing and Idle-Time Consolidation).

Working directory: e:\project\openclaw_remake
Integrity mode: development

## Requirements

### R1. Compute Engine Upgrades (Speculative Decoding & Prompt Caching)
- Integrate speculative decoding in the local native inference core (using a smaller draft model like Qwen-0.5B alongside the target model).
- Implement persistent system-prompt caching at the gRPC/IPC layer to prevent recalculating the system instructions on subsequent conversational turns, reducing Time-to-First-Token (TTFT) to near-zero.

### R2. Memory Subsystem Upgrades (Hybrid Routing & Background Consolidation)
- Build a hybrid routing layer using a small language model (SLM) or embedding similarity to analyze query intent and direct queries to either the Knowledge Graph (SQLite) or Vector Database (sqlite-vec) to avoid redundant querying.
- Implement an idle-time memory consolidation daemon/cron-job that runs in the background when the system is idle, clustering and summarizing short-term memories from the conversational turns into long-term structured knowledge (meta-entities) and cleaning cache.

### R3. Codebase Integration & Compliance
- Ensure all additions comply strictly with LIVA's code architecture constraints defined in `AI_CONTEXT.md` (e.g., no blocked event loops, use of `safeFetch`, SQLite debounced writes, Pino logging, and TypeScript strict safety).
- Ensure existing Tauri and Gateway functionalities remain fully operational and backward-compatible.

## Acceptance Criteria

### Compute Engine Verification
- [ ] Speculative decoding can be toggled on/off via configuration. When enabled, token generation speed (tokens/second) shows a measurable increase (at least 1.3x speedup on standard prompts).
- [ ] System prompt KV cache is successfully retained across consecutive messages, showing a TTFT of under 100ms on subsequent turns.

### Memory Subsystem Verification
- [ ] The hybrid routing layer successfully classifies user intents into target data sources (e.g., code/project logic -> Knowledge Graph; conversational memory/old experience -> Vector DB) with at least 90% accuracy on a test suite of 20 benchmark prompts.
- [ ] The idle consolidation job activates after a configured idle interval, correctly clusters short-term memory nodes, updates the SQLite DB, and does not leak memory or block the main Node.js event loop.

### Integration Verification
- [ ] All existing and new integration tests in the gateway build and pass successfully (`npm run test:gateway`).
- [ ] An automated verification script (`npm run test:upgrades` or similar) is provided in the repository to programmatically verify the speculative decoding throughput and routing accuracy.
