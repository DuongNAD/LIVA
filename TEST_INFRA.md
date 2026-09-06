# E2E Test Infra: LIVA Remediation

## Test Philosophy
- Multi-tier opaque-box and white-box verification covering frontend IPC routing, desktop window permissions, Rust native engine commands & checkpointer, LLM output filter, and dependency security.

## Feature Inventory
| # | Feature | Source | Tier 1 (Unit) | Tier 2 (Boundary) | Tier 3 (Integration) | Tier 4 (E2E) |
|---|---------|--------|:-------------:|:-----------------:|:--------------------:|:------------:|
| 1 | FE-01 (TauriAdapter IPC) | ORIGINAL_REQUEST | `PlatformAdapter.test.ts` | Null/empty args, unknown cmds | `useLivingCanvas.test.ts` | Living Canvas sync |
| 2 | FE-02 (Widget Allow-Hide) | ORIGINAL_REQUEST | `capability_policy.rs` | Window minimize/hide | Tray minimize flow | Desktop capability validation |
| 3 | RUST-02 (Browser is_running) | ORIGINAL_REQUEST | `commands::browser::tests` | driver closed vs open | `browser:status` IPC | Status query accuracy |
| 4 | RUST-03 (Checkpointer) | ORIGINAL_REQUEST | `agent::graph::checkpoint::tests` | 0 checkpoints, target < min | `restore_time_travel` | Time-travel rollback |
| 5 | LOG-01 (Reasoning filter) | ORIGINAL_REQUEST | `llm::output_filter::tests` | split chunks, mixed tokens | Streaming engine pipeline | Prompt & stream filtering |
| 6 | SEC-02 (fast-uri CVE) | ORIGINAL_REQUEST | `package.json` overrides | Version resolution | `npm ls fast-uri` | Dependency tree security |

## Test Commands
- Frontend tests: `cd liva-ui && npm test`
- Frontend build: `cd liva-ui && npm run build`
- Desktop tests: `cargo test -p liva-desktop`
- Native Core tests: `cargo test -p liva-native-core --lib`
- Clippy: `cargo clippy --workspace --all-targets`
- Cargo check: `cargo check --workspace`
