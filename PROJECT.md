# Project: LIVA Architectural Teardown & Redesign

## Architecture
- **liva-gateway**: TypeScript/Node.js based assistant gateway. Database operations are currently handled via a single SQLite connection inside a DatabaseWorker thread.
- **liva-native-core**: A new Rust-based core backend utilizing the Tokio runtime and exposing basic IPC functions to lay the foundation for future gateway/engine integration.

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | Baseline Exploration | Explore SQLite worker implementation, analyze test suite execution, run impact analysis on DB symbols. | none | DONE |
| 2 | SQLite Database Overhaul | Implement SQLite WAL mode and Connection Pool in liva-gateway (1 Write thread, multiple Read threads/connections). | M1 | DONE |
| 3 | Bootstrap Native Core | Create the liva-native-core Cargo project with Tokio runtime and basic IPC boilerplate. | none | DONE |
| 4 | Final Verification & Audit | Verify 100% pass rate of the 2718 tests in liva-gateway, verify liva-native-core builds, and run Forensic Audit. | M2, M3 | DONE |

## Interface Contracts
### SQLite Connection Pool in liva-gateway
- Enable WAL mode (`PRAGMA journal_mode = WAL;`).
- Implement connection pool with 1 Writer thread/connection and multiple Reader threads/connections.
- Preserve same public API / messages as the original `DatabaseWorker` to ensure existing gateway code and tests are unaffected.

### Rust liva-native-core IPC
- Exposed IPC interface (functions/traits/structs) for communication.
- Tokio multi-threaded asynchronous runtime configuration.

## Code Layout
- `liva-gateway/src/workers/DatabaseWorker.ts`
- `liva-gateway/src/memory/DatabaseWorkerBridge.ts`
- `liva-native-core/`
