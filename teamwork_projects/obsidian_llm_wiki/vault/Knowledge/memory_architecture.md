---
title: "memory_architecture"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-06-21T02:21:19Z"
---

# Knowledge: Memory Architecture

## Executive Summary
This document outlines the detailed memory architecture of the LIVA system (LIVA-UHM v2 — Consolidated Brain), covering memory layers L0/L1/L2/L3, ReflectionDaemon, and ConsolidationCron.

## Detailed Description
### Memory Layers
- **L0: Local Context (RAM)** — In-memory cache in MemoryManager.
- **L1: StructuredMemory (SQLite)** — Event bricks (Φ Factual + Ψ Relational) + KV facts. Persists raw conversational turns directly into `turn_layer_nodes` in `StructuredMemory.sqlite`.
- **L2: VectorMemory (sqlite-vec)** — Consolidated narratives. Tích hợp **H-MEM Positional Index** trỏ ngược về L1 qua `source_event_ids` (O(1) `json_each` Drill-down).
- **L3: PersonalKnowledge (KV)** — Insights người dùng. Áp dụng **Ebbinghaus Forgetting Curve** (V8 Math.exp decay + chunking), Strength < 0.2 bị loại khỏi prompt.

### Orchestration Pipeline
- **SemanticRouter** — Routes queries (<100ms, sqlite-vec cosine + FTS5 Drill-down).
- **ReflectionDaemon** — Extracts Φ/Ψ ngầm asynchronously via batched extraction using Zod Dual Schema (Factual/Relational). Emits passive signals via MemoryEventBus (0 extra LLM calls). Debounced at 12s.
- **ConsolidationCron** — Hợp nhất L1→L2+L3. Processes idle events into L2 `AXIOM` and temporal `ANCHOR` vectors.
  - Triggered by: 30min Idle HOẶC Passive Signal Burst (topicShiftCount >= 3 OR unconsolidatedCount >= 20, 15s debounce).
  - 🚨 Strict Guardrail: Kích hoạt CHỈ KHI `agentLoop.getState() === 'IDLE'` để bảo vệ VRAM.
- **MemoryDreamingPipeline** — Tách biệt vùng nhớ thô (read-write log) và vùng nhớ tinh chế (read-only index).
  - SHA-256 deduplication, weight accumulation và tầm quan trọng được xếp hạng.
  - Git-Diff Human-in-the-loop để hiển thị thay đổi cấu trúc bộ nhớ.
  - Auto-commit nếu tỉ lệ nén (compression ratio) > 30%, ngược lại giữ chờ supervisor phê duyệt.

### Agentic Memory Management (AgeMem)
ManageMemory Skill — Agent CRUD trực tiếp lên L1 KV Facts (add/update/delete/search).
- Namespace Isolation: Chỉ categories whitelisted (user_preferences, relationships, facts, work_context, personal_info).
- HITL Guard: delete action BẮT BUỘC human approval.
- Rate Limit: Max 5 mutations/turn.
- Ebbinghaus Sync: update → memory_strength reset to 1.0.
- Audit: source='agent_explicit' (phân biệt vs 'auto_extract').

### DLQ 3-Strike Schema
- `events.consolidation_status`: 'pending' (new) | 'consolidated' (done) | 'dlq' (failed 3x)
- `events.retry_count`: 0-3 (auto-increment on Zod fail)
- ⚠️ Backward Compat: ALTER TABLE DEFAULT 'consolidated' — old data KHÔNG bị re-process.
- ⚠️ Partial Index: idx_events_pending ON events(eventId) WHERE consolidation_status = 'pending'.

### L2 Semantic Memory Injection
Activated in v22:
- PromptBuilder.buildContextPrompt()
  - IF route = factual_recall | deep_reasoning
  - IF remainingBudget > 500 chars
  - EmbeddingWorker.embed(userText) [Isolated CPU Thread]
  - StructuredMemory.searchAnchors(queryVec, top_k=3)
  - Inject into <context_memory> XML sandbox (max 30% budget)
- Opt-out: FF_DISABLE_L2_INJECTION=true

### KV Cache Optimization & 2-Tier Inference Array (v27)
- PromptBuilder.prepareFullAiMessages()
  - Generates 100% static System Prompt (only core instructions and static profile metadata).
  - Extracts and returns all dynamic elements (RAG context, tools schemas, token budgets) inside dynamicContextBlock wrapped in <SYSTEM_CONTEXT> XML tags.
- AgentLoop / IsolatedAgentTurn
  - Ephemeral Injection: dynamicContextBlock + dynamicContext (System Time/Location) is injected into the last User Message during inference only.
  - 2-Tier Inference Array: Clones clean history (aiMessages) into executionMessages for LLM cycles. Pushes assistant tool calls and user tool results to executionMessages to retain 100% KV cache prefill hits across turns (<10ms).
  - Clean session messages (clean User text and final clean reply) are written to SQLite, avoiding database context bloat.

### Unified Hybrid Memory (UHM) Guardrails (MUST follow)
| ID | Rule | Rationale |
|---|---|---|
| G1 | No SQLite math functions | `Math.exp()` in V8 only — SQLite lacks `EXP()` |
| G2 | RAM-buffered fact touches | Prevents write amplification on hot paths |
| G3 | `json_each()` for variable binding | Bypasses SQLite 999-param limit safely |
| G4 | Cap `sourceEventIds` at 50 | Prevents VRAM/RAM overflow on vector meta |
| G5 | Zod-validated `source_event_ids` | `EventIdsSchema.safeParse()` — prevents LLM garbage crashing `json_each` |
| G6 | 15s affective debounce | Prevents event loop flooding |
| G7 | EventBus decoupling | `MemoryEventBus` — zero import coupling between ReflectionDaemon ↔ ConsolidationCron |
| G8 | Atomic transactions | BEGIN/COMMIT/ROLLBACK for batch writes |
| G9 | Dual VRAM guard | `isRunning` + `agentLoopStateGetter() === 'IDLE'` — blocks concurrent LLM ops |
| G10 | Shutdown flush guarantee | `close()` → `flushFactTouches()` before `db.close()` |
| G11 | Chunked decay + `setImmediate` yield | 500-row chunks prevent Event Loop blocking >10ms |
| G12 | `VACUUM INTO` for backup | NEVER `fs.promises.cp` on running SQLite — guaranteed WAL corruption |
| G13 | DLQ 3-Strike | Events failing Zod 3x → `consolidation_status='dlq'`, excluded from retry |
| G14 | AgeMem namespace isolation | Only whitelisted categories accessible via ManageMemory skill |
