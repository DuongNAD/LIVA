---
title: "testing_guidelines"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-11-01T04:10:00+07:00"
---

# Knowledge: Testing Guidelines

## Test layers & Canonical Commands

- **Rust Workspace (7 crates)**:
  - Canonical test runner: `cargo test -j 2 -- --test-threads 2` across all workspace crates (`liva-native-core`, `liva-desktop`, `liva-core-types`, `liva-storage`, `liva-llm`, `liva-cua`, `liva-tools`).
  - Strict compile gate: `cargo check --workspace -j 2`.
  - Zero-warning linter gate: `cargo clippy -j 2` (must exit with 0 warnings across all crates).
- **Vue 3 / TypeScript UI (`liva-ui`)**:
  - Unit & composable tests: `npm test -w liva-ui` (Vitest suite).
  - TypeScript build verification: `npm run build:ui` (`vue-tsc -b` and Vite bundle).
- **Desktop Shell & Installer (`liva-desktop`)**:
  - Installer verification: `npm run check:installer` and `npm run test:installer`.
- **Documentation & Maintenance Tools**:
  - Documentation integrity: `npm run docs:check`.
  - Skills & governance audit: `node scripts/audit-liva-skills.mjs` and `npm run test:skills`.
  - Note: All legacy Python `liva-voice` services have been completely retired; all voice DSP and wake-word logic runs natively in the Rust binary.

Use the command already declared by the owning workspace instead of inventing a parallel test harness.

## Evidence requirements

- A completion claim requires fresh output from the exact acceptance command.
- Every new behavior needs a happy-path test and at least one failure, boundary, timeout, or malformed-input test.
- A test that only proves a parser accepts valid input is insufficient; validators must also reject invalid state.
- Network, model, clock, filesystem, and external-service dependencies must be controlled or mocked unless the test is explicitly marked as an integration test.

## Isolation and cleanup

- Give each database test an isolated temporary database and remove temporary WAL/SHM files during teardown.
- Use unique temporary directories and ports. Do not depend on developer services already running.
- Restore global mocks, fake timers, environment variables, subscriptions, and spawned processes even after failures.
- Attach rejection handlers before advancing fake timers so rejected promises cannot escape as unhandled failures.

## Security and concurrency

- Test invalid IPC payloads, authorization failures, path traversal, corrupt persisted data, and secret redaction where relevant.
- Rust async tests must cover cancellation, lock/contention, shutdown, and partial-failure paths when the implementation owns those concerns.
- Streaming tests should assert ordering, backpressure/cancellation, and bounded resource cleanup rather than only the final text.

## Repository-wide verification

Run the focused test first, then the smallest relevant workspace gate, followed by broader CI gates in proportion to blast radius. Before staging code changes, run GitNexus `detect_changes` and confirm only expected symbols and flows are affected.
