# E2E Test Suite Ready

## Test Runner
- Command: `npm run test:upgrades` (run from the workspace root or `liva-gateway/` directory)
- Expected: all 23 Vitest upgrades test cases pass, and the standalone benchmark script exits with code 0.

## Coverage Summary
| Tier | Count | Description |
|------|------:|-------------|
| 1. Feature Coverage | 6 | Happy-path tests for Speculative parsing/args, Caching parameters, Router anchors, and Consolidation 9-step pipeline. |
| 2. Boundary & Corner | 7 | Edge case validation: prefix mismatch bypass, draft model missing fallback, gRPC Unavailable (14) retries, swap rollback, LLM busy guard, WAL SQLite busy contention, and battery-aware gating. |
| 3. Cross-Feature | 4 | Interaction checks: cache warming/hydration, dynamic model swap, and tool cache fast-path bypass. |
| 4. Real-World Application | 6 | Opaque-box execution: gRPC streaming response chat, hybrid routing accuracy check on 20 prompts, and speculative speedup simulation. |
| **Total** | **23** | Total upgraded Vitest test specs plus automated E2E benchmark checks. |

## Feature Checklist
| Feature | Tier 1 | Tier 2 | Tier 3 | Tier 4 |
|---------|:------:|:------:|:------:|:------:|
| Speculative Decoding | 2 | 2 | 1 | 1 |
| Persistent Caching | 2 | 2 | 1 | 1 |
| Hybrid Query Routing | 1 | 1 | 1 | 2 |
| Memory Consolidation | 1 | 2 | 1 | 2 |
