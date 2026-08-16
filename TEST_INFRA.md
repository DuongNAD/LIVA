# E2E Test Infra: LIVA Intelligent Assistant Ecosystem

> ⚠️ **SUPERSEDED 16/08/2026.** This document describes the 5-tier / 177-assertion design that
> `scripts/e2e-test-suite.mjs` used to implement. That suite was **green by construction** — it asserted
> against JavaScript reimplementations of LIVA's algorithms in `scripts/e2e/helpers.mjs` and never reached
> the Rust core, the UI, a socket, or a real database. It has been replaced. Retraction and evidence:
> [`TEST_READY.md`](TEST_READY.md).
>
> **The tier structure below is retained as the record of what was designed, not as a description of what
> runs.** What runs now is 7 sections / 36 assertions against a real spawned binary over a real TCP
> WebSocket. The count is smaller on purpose: `authorization.rs:154` grants `CommandPrincipal::WebSocketRemote`
> exactly the 9 commands in `REMOTE_COMMANDS`, so F1–F15 **cannot** be exercised end-to-end through that
> socket — their genuine coverage is the in-process Rust tests (`cargo test`, 894 passing). Padding a
> socket suite out to 177 would mean faking the difference, which is precisely the defect being removed.
>
> The replacement is proven able to fail: widening `REMOTE_COMMANDS` by one entry turns it red and names
> the leaked command.

## Test Philosophy
- **Requirement-Driven & Opaque-Box**: All test assertions are derived strictly from `ORIGINAL_REQUEST.md` and user-facing contracts, operating independently of internal implementation details.
- **Methodology**: 4-Tier Systematic Testing (Category-Partition, Boundary Value Analysis, Pairwise Combinatorial Testing, Real-World Workload Testing) + Tier 5 Adversarial Coverage Hardening.

## Feature Inventory
| # | Feature | Source | Tier 1 (Coverage) | Tier 2 (Boundary) | Tier 3 (Pairwise) | Tier 4 (Workloads) |
|---|---------|--------|:-----------------:|:-----------------:|:-----------------:|:------------------:|
| F1 | SQLite WAL Pool Concurrency | R1 | 5 | 5 | ✓ | ✓ |
| F2 | Low-Latency IPC Streaming (<100ms) | R1 | 5 | 5 | ✓ | ✓ |
| F3 | Workspace Build & Clean Clippy | R1 | 5 | 5 | ✓ | ✓ |
| F4 | Global Toast Notification System | R2 | 5 | 5 | ✓ | ✓ |
| F5 | Shimmer Skeleton Loaders | R2 | 5 | 5 | ✓ | ✓ |
| F6 | Native File Dialog Configuration | R2 | 5 | 5 | ✓ | ✓ |
| F7 | BI Analytics Dashboard | R2 | 5 | 5 | ✓ | ✓ |
| F8 | Obsidian PKM Vault Explorer | R2 | 5 | 5 | ✓ | ✓ |
| F9 | Frontend Clean Build & Vitest | R2 | 5 | 5 | ✓ | ✓ |
| F10 | AI Router Token Guard & KV Prune | R3 | 5 | 5 | ✓ | ✓ |
| F11 | Hybrid Vector & FTS5 Search (RRF) | R3 | 5 | 5 | ✓ | ✓ |
| F12 | Swarm DAG StateGraph Execution | R3 | 5 | 5 | ✓ | ✓ |
| F13 | DPAPI Keystore & Transcript AES Encryption | R4 | 5 | 5 | ✓ | ✓ |
| F14 | Atomic Right-to-be-Forgotten Deletion | R4 | 5 | 5 | ✓ | ✓ |
| F15 | Tech-Debt Ledger & Skills Governance | R4 | 5 | 5 | ✓ | ✓ |

## Test Architecture
- **Rust Native Test Runner**: `cargo test -p liva-native-core --lib` (620+ tests).
- **Clippy & Linter Check**: `cargo clippy --workspace --all-targets -- -D warnings`.
- **Dependency Vulnerability Audit**: `cargo audit`.
- **Frontend Test Runner**: `npm run test -w liva-ui` (Vitest, 410+ tests).
- **Frontend Type & Build Runner**: `npm run build -w liva-ui` (`vue-tsc -b && vite build`).
- **Frontend Linter**: `npx eslint liva-ui/src`.
- **Obsidian PKM Test Runner**: `npm test -w obsidian-llm-wiki` (42+ tests).
- **Skills Governance Audit**: `node scripts/audit-liva-skills.mjs` & `node --test scripts/audit-liva-skills.test.mjs`.

## Real-World Application Scenarios (Tier 4)
| # | Scenario | Features Exercised | Complexity |
|---|----------|--------------------|------------|
| 1 | High-concurrency simultaneous database read/write queries without SQLITE_BUSY | F1, F2, F13 | High |
| 2 | End-to-end token streaming response rendering with toast error handling | F2, F4, F9, F10 | High |
| 3 | Multilingual Vietnamese query hybrid search with RRF vector + FTS ranking | F10, F11, F13 | High |
| 4 | Multi-agent DAG workflow dispatch with scoped tool execution policy | F10, F12, F14 | High |
| 5 | Right-to-be-forgotten conversation purge with secure delete pragma verification | F1, F13, F14 | High |
| 6 | BI dashboard metrics visualization and Obsidian vault note exploration | F7, F8, F9 | Medium |
| 7 | Full ecosystem clean compile, zero-warning clippy, and 100/100 tech debt audit | F3, F9, F15 | High |

## Coverage Thresholds
- Tier 1: ≥5 test cases per feature (75 test cases minimum)
- Tier 2: ≥5 boundary/corner test cases per feature (75 test cases minimum)
- Tier 3: Pairwise coverage across major module intersections (15+ cross-feature tests)
- Tier 4: ≥7 realistic end-to-end workflow scenarios
- Tier 5: Adversarial edge cases and forensic integrity verification
