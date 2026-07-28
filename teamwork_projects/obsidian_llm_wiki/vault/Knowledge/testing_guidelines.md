---
title: "testing_guidelines"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-07-28T00:00:00+07:00"
---

# Knowledge: Testing Guidelines

## Test layers

- Rust native core: `cargo test` for unit and integration behavior; run `cargo check --all-targets` and Clippy for compile/lint gates.
- Vue/TypeScript UI: Vitest for units, TypeScript type-checking, ESLint, and Playwright for user-visible end-to-end flows.
- Obsidian MCP and Node maintenance scripts: Node's test runner or the package's existing Vitest suite.
- Active Python voice service: Pytest with local fixtures; never call paid or live external APIs.

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
