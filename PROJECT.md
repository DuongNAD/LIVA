# Project: LIVA Desktop UI Redesign

## Architecture
- **desktop_client**: Vue 3 + Vite + TypeScript frontend wrapped in Tauri v2.
  - Features professional, side-by-side window layout using modern CSS Grid/Flexbox.
  - Supports scaling under various OS scaling factors (e.g. 100%, 125%, 150%).
- **Tauri Integration**:
  - Global Tauri internals and APIs mocked for tests.
  - Interactive click-through zones updated via custom IPC commands.
- **liva-native-core**: Rust-based core backend server exposing WebSocket endpoint on port 8002.
  - Exposes tools like read_markdown, write_markdown, search_vault.
  - Implements state persistence via SQLite.
- **Verification Scripts**: Automated test suites to verify and audit the system components.
  - Vitest + jsdom + @vue/test-utils to run fully-isolated high-fidelity mock E2E client tests.

## Milestones
| Track | ID | Scope | Details | Dependencies | Status |
|---|---|---|---|---|---|
| Exploration | M0 | Codebase Exploration | Analyze current layout, styles, and config | none | DONE |
| Implementation | M2B | UI Redesign & Styling | Overhaul layout to feature-rich side-by-side dashboard | none | DONE |
| Implementation | M2C | Scaling & Event-Driven Click-Through | Replace Rust polling loop with event-driven hooks | M2B | DONE |
| E2E Test | M1A | Test Infrastructure & Tiers 1-4 | Set up Vitest/Playwright, write Tier 1-4 test cases for 6 features, publish `TEST_READY.md`. | none | DONE |
| E2E Test | M2D | E2E Test Integration & Verification | Wait for E2E tests ready & verify Tiers 1-4. | M1A, M2C | DONE |
| Release | M2F | Release & Audit | Compile final desktop-client.exe and generate clean Forensic Audit report. | M2E | DONE |

## Interface Contracts
- **Tauri Click-Through**: Vue registers interactive rectangular zones (buttons, inputs) using `update_interactive_zones` IPC command.
- **WebSocket Protocol**: Standard WebSocket communication on port 8002.
