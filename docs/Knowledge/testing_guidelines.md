---
title: "testing_guidelines"
tags:
  - liva/knowledge
author: "worker"
last_update: "2026-06-21T02:21:19Z"
---

# Knowledge: Testing Guidelines

## Executive Summary
This document defines testing conventions, guidelines, directory structure, mock requirements, and commands for the LIVA test suite.

## Detailed Description
### Testing Commands & Environment Flags
- **Jest (TypeScript Gateway)**:
  `cross-env NODE_OPTIONS=--experimental-vm-modules jest --runInBand`
  - `--runInBand` is required because tests execute sequentially to prevent SQLite database lock and resource conflicts.
- **Pytest (Python Voice Engine)**:
  Used to test the python voice engine modules.

### Directory Mirroring Rules
- Test files MUST mirror the source directory structure precisely:
  `src/path/to/Module.ts` → `tests/path/to/Module.test.ts`
- E.g., `src/memory/StructuredMemory.ts` has its test file located at `tests/memory/StructuredMemory.test.ts`.

### Mocking Guidelines & Fetch Requirements
- **NO REAL API CALLS**: Never call external web services or APIs during test execution.
- **Mocking Fetch**: Always mock global fetch using `vi.stubGlobal('fetch', vi.fn())` (for Vitest) or similar Jest mock mechanisms. Never use redundant mocking libraries like `axios-mock-adapter` or `nock`.
- **Negative Test Case Requirement**: High-quality tests must verify failures, not just happy paths. Every fetch mock should include at least one test case representing 4xx/5xx responses or timeouts.
- **Module-level Mock Completeness**: When mocking Node modules (like `fs`), ensure all methods used by the target module are mocked (`readFile`, `writeFile`, `rename`, `existsSync`, `mkdirSync`). Incomplete mocks cause silent test failures inside caught error blocks.
- **Fake Timers + Promise Rejections**: When testing timeouts with fake timers, always attach a `.catch()` block to the target promise before advancing timers to avoid unhandled rejection crashes.

### Database Test Cleanup
- SQLite test suites must clean up and delete temporary `.sqlite` files in `afterEach` or `afterAll` hooks to prevent workspace clutter and data pollution.
