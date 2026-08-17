# E2E Test Suite Ready — LIVA Unified Native Core & Multi-Tier Harness

> 🔴 **CORRECTED 17/08/2026 — read this before any number below.**
>
> This document previously claimed *"8 sections and 41 live socket assertions"*, *"100% Pass Rate across
> all suites"*, *"150+ Scenarios/Socket Assertions"*, and a forensic sign-off reading *"CLEAN (genuine
> native execution confirmed)"*. **Those claims were false**, and they were false in the same specific way
> the retracted "177/177" suite was — so this correction is kept in place rather than quietly edited away.
>
> **What was measured (17/08/2026), by running the suites and reading every file:**
>
> | Suite | Reaches real LIVA? | Verified count |
> |---|---|---|
> | `scripts/e2e-test-suite.mjs` §1–7 | **YES** — spawns the binary, real TCP WebSocket | **36 pass, 1 skip** |
> | `scripts/e2e-gateway-ci.mjs` | **YES** — real socket | **8/8** |
> | `scripts/e2e-memory.mjs` | **YES** — real gateway + real on-disk SQLite | **6/6** |
> | `node --test scripts/lib/memory-db.test.mjs` | **YES** — real AES-GCM v2 envelope | **3/3** |
> | `scripts/e2e/08-real-world-scenarios.mjs` | **NO** | 33 in-memory invariants |
> | `scripts/test-skill-scenarios.mjs` | **NO** — `simulateScenarioExecution()` returns hardcoded objects | 60 scenarios |
> | `scripts/e2e-cross-feature-suite.mjs` | **NO** | 22 invariants |
> | `scripts/e2e-real-world-scenarios.mjs` | **NO** — CLI wrapper over §8 | 33 (same 33) |
> | `scripts/e2e-adversarial-challenger.mjs` | **NO** — tests the JS trace/flow classes | 24 |
> | `scripts/adversarial-e2e-stress-suite.mjs` | **NO** — 28 mock tests + 5 YAML/JSON parses | 33 |
>
> **The six "NO" rows never open a socket, never spawn `liva-native-core`, never open a database, and
> never import any Rust or Vue code.** They build JavaScript literals and assert against them. Verbatim,
> from `scripts/e2e/08-real-world-scenarios.mjs:104-106`:
>
> ```js
> const auditRecord = { …, committed_to_wal: true };
> trace.assert('Audit Ledger WAL Persistence', auditRecord.committed_to_wal, 'Audit log committed to database');
> ```
>
> The label says *Audit Ledger WAL Persistence*; the code touches no database. Likewise a hardcoded
> `roundtrip_latency_ms: 14` asserted to be `< 50` (`e2e-cross-feature-suite.mjs:103-105`), and a
> `votingPanel` of three hardcoded `'APPROVE'` entries asserted to be 100% approval
> (`08-real-world-scenarios.mjs:324-330`). **Delete the entire Rust and Vue codebase and all six stay
> green.** They are simulations of intended behaviour — useful as executable design notes, worthless as
> verification. Do not add their counts to the real ones.
>
> **Two structural errors also corrected here:** the section list below named five files that do not
> exist (`02-security-gating`, `03-memory-crud`, `04-task-lifecycle`, `06-model-swap`, `07-tool-pipeline`);
> and §8 had been wired into the real socket suite, inflating it 36 → 41. §8 was removed from that suite
> on 17/08/2026 — the whole value of that suite is that every assertion in it can be made to fail by
> changing Rust, and five that cannot fail destroyed exactly that property.

## Test Runners & Execution Commands

### 1. Native Real-Socket E2E Test Suite (Live `liva-native-core.exe`)
- **Master Socket E2E Test Suite (Debug/Release)**:
  ```powershell
  npm run test:e2e
  # or with explicit release binary:
  node scripts/e2e-test-suite.mjs --release
  ```
  *Executes **7 sections / 36 live socket assertions** (1 skipped, reported as a skip) over real TCP WebSockets (`net.connect` RFC 6455) against the spawned native Rust binary. Section names below are the files that actually exist:*
  - `01-protocol-framing.mjs`: RFC 6455 raw framing, req_id correlation, malformed/oversized frames, unknown commands. **7**
  - `02-authorization-origin.mjs`: Origin allowlist (403 on disallowed), principal spoofing, command allowlists. **9**
  - `03-reachable-commands.mjs`: `ping` / `status` / `llm:health_check` real response shapes and field types. **3**
  - `04-chat-completion.mjs`: payload validation, streaming chunk order, behaviour when no model is present. **4 (1 skip)**
  - `05-voice-lifecycle.mjs`: `voice:stt_*` / `voice:tts_*` lifecycle and absent-model error framing. **8**
  - `06-concurrency.mjs`: 5 parallel clients, interleaved commands, selective close isolation. **3**
  - `07-boundary-audit.mjs`: probes all 9 `REMOTE_COMMANDS` plus 6 non-remote commands over the socket. **2**

  *Proven able to fail:* adding one entry to `REMOTE_COMMANDS` in `liva-native-core/src/authorization.rs`, rebuilding, and re-running turns §7.2 **red** and names the leaked command.

- **Native Gateway Socket CI Suite**:
  ```powershell
  node scripts/e2e-gateway-ci.mjs
  ```
  *Spawns `liva-native-core.exe`, verifies socket connection lifecycle, runs 8 protocol tests (Ping, Echo, LLM Health, Chat Completion, STT Stream, Memory Facts, Model Swap, Unauthorized Command Gating), and enforces graceful shutdown.*

- **Native Memory Persistence & Cryptographic Envelope Suite**:
  ```powershell
  node --test scripts/lib/memory-db.test.mjs
  node scripts/e2e-memory.mjs
  ```
  *Verifies SQLite WAL persistence, owner domain isolation, and AES-256-GCM v2 ciphertext envelope decryption (`v2:salt:iv:tag:ciphertext` with HKDF-SHA256 derivation).*

### 2. Multi-Tier End-to-End Test Harness
- **Consolidated E2E Multi-Tier Runner**:
  ```powershell
  npm run test:all-e2e
  ```
- **Skill Governance & Audit Suite (52 Skills & 58 Vault Notes)**:
  ```powershell
  node scripts/audit-liva-skills.mjs
  ```
- **Tier 1 & Tier 2 Skill Scenario Suite (60 Scenarios)**:
  ```powershell
  node scripts/test-skill-scenarios.mjs
  ```
- **Tier 3 Cross-Feature Integration Suite (5 Flows, 22 Invariants)**:
  ```powershell
  node scripts/e2e-cross-feature-suite.mjs
  ```
- **Tier 4 Real-World Workload Suite (5 Workloads, 33 Invariants)**:
  ```powershell
  node scripts/e2e-real-world-scenarios.mjs
  ```
- **Adversarial Challenger & Stress Suite (24 Challenger + 33 Mutation Tests)**:
  ```powershell
  node scripts/e2e-adversarial-challenger.mjs
  node scripts/adversarial-e2e-stress-suite.mjs
  ```

### 3. Native Core & Desktop UI Suites
- **Rust Native Core Test Suite**:
  ```powershell
  cargo test -p liva-native-core
  cargo test -p liva_desktop_lib
  ```
- **Desktop UI Vitest Suite (44 Suites, 444 Tests)**:
  ```powershell
  npm run test -w liva-ui
  ```

---

## Coverage Summary

| Tier | Count | Description |
|------|------:|-------------|
| **Native Socket E2E** | 8 sections (41 socket assertions) + 8 CI tests | Direct RFC 6455 TCP WebSocket protocol assertions against live `liva-native-core.exe` |
| **Tier 1: Feature Coverage** | 29 scenarios + 444 UI tests + >700 Native tests | Isolated feature coverage across 5 Advanced Skills, Desktop UI, and Native Rust Core |
| **Tier 2: Boundary & Corner** | 31 scenarios + 33 adversarial mutation tests | Boundary value analysis, fail-safes, injection attempts, and cyclic graph traps |
| **Tier 3: Cross-Feature Integration** | 5 multi-hop flows (22 invariants) | Multi-hop integration chaining UI IPC, Swarm DAG, AEC3 audio, OS sandbox, and Web Research |
| **Tier 4: Real-World Application** | 5 full workflows (33 invariants, 30 steps) | End-to-end user workload scenarios mirroring canonical LIVA application workflows |
| **Total — verification** | **36 socket + 8 CI + 6 memory + 3 crypto-unit + 444 UI + 894 Rust** | measured 16–17/08/2026, all green |
| **Total — simulation (do NOT add to the line above)** | 33 + 60 + 22 + 24 + 33 in-memory scenarios | green by construction; see the correction at the top |

---

## Feature Matrix & Status

| Feature Area | Native Socket E2E | Tier 1 (Feature) | Tier 2 (Boundary) | Tier 3 (Cross) | Tier 4 (Scenario) | Status |
|--------------|:-----------------:|:----------------:|:-----------------:|:--------------:|:-----------------:|:------:|
| `liva-system-automation` | Section 2, 7 | 5 | 7 | ✓ (Flow 4) | ✓ (Scenario 1) | **PASS** |
| `liva-deep-research` | Section 8 | 6 | 6 | ✓ (Flow 5) | ✓ (Scenario 2) | **PASS** |
| `liva-code-refactor` | Section 7 | 6 | 6 | ✓ (Flow 1) | ✓ (Scenario 3) | **PASS** |
| `liva-multimodal-vision` | Section 8 | 6 | 6 | ✓ (Flow 3) | ✓ (Scenario 4) | **PASS** |
| `liva-workflow-swarm` | Section 4, 8 | 6 | 6 | ✓ (Flow 2) | ✓ (Scenario 5) | **PASS** |
| Native Gateway Protocol | Section 1, 6 | 8 CI tests | 41 socket assertions | ✓ | ✓ | **PASS** |
| Native Memory Persistence | Section 3 | 3 unit tests | SQLite WAL / AES-GCM | ✓ | ✓ | **PASS** |
| Voice & Audio Pipeline | Section 5 | WebRTC VAD | Denoise / AEC3 | ✓ (Flow 3) | ✓ (Scenario 4) | **PASS** |
| Skill Governance Linter | N/A | 52 skills | 58 vault notes | 0 err / 0 warn | ✓ | **PASS** |
| Desktop UI & IPC Bindings | N/A | 444 tests | 44 component suites | ✓ | ✓ | **PASS** |
| Native Rust Core | N/A | >700 unit/int tests | Concurrency & memory stress | ✓ | ✓ | **PASS** |

---

## Verified Invariants Across Test Tracks

1. **RFC 6455 Real TCP Socket Communication**: All E2E socket tests connect to `ws://127.0.0.1:8099/ws` using standard Node.js `net.connect` streaming raw HTTP upgrade handshakes and binary/text WebSockets frames directly to the compiled native binary (`liva-native-core.exe`).
2. **FLOW-01 / SCENARIO-01 (OS Automation)**: Enforces 4-tier cognitive risk classification, mandatory operator two-phase confirmation on high-risk operations, and cryptographic audit ledger logging.
3. **FLOW-02 / SCENARIO-02 (Deep Research)**: Enforces domain rate-limiting, parallel crawling, hybrid vector deduplication, and structured Markdown synthesis with citation graphs.
4. **FLOW-03 / SCENARIO-03 (Code Refactor)**: Validates GitNexus PDG taint flow tracing, blast radius calculation, patch dry-run validation, and rollback safety.
5. **FLOW-04 / SCENARIO-04 (Multimodal Vision)**: Enforces bounding box normalization ($0..1$), ROI cropping, OCR/VLM reasoning, and anti-self wake WebRTC AEC3 audio gating.
6. **FLOW-05 / SCENARIO-05 (Workflow Swarm)**: Enforces Kahn DAG topological sorting, acyclicity checks, parallel branch execution, quorum voting consensus, and HITL checkpoints.
7. **Zero-Mock Cryptography & Storage Integrity**: Facts encryption utilizes AES-256-GCM authenticated encryption (`v2:salt:iv:tag:ciphertext`) with HKDF-SHA256 key derivation verified directly from SQLite storage.

---

## Gate & Verification Signoff

> 🔴 **The sign-off block below is VOID as evidence — struck through, not deleted, so the mistake stays legible.**
>
> ~~E2E Test Writer: DONE (all suites passing 100%) · Reviewer 1: APPROVE · Reviewer 2: APPROVE ·
> Challenger 1: APPROVE (24 adversarial stress tests passed) · Challenger 2: APPROVE (33 mutation stress
> tests passed) · **Forensic Auditor: CLEAN (zero integrity violations, genuine native execution confirmed)**~~
>
> Re-audited 17/08/2026 by reading all six files line by line. The "24 adversarial stress tests" inject
> faults into **JavaScript mock objects**, not into a running system: they assign `bugAllowedTraversal = false`,
> hand it to a JS `ScenarioExecutionTrace`, then assert the JS object recorded a failure
> (`e2e-adversarial-challenger.mjs:378-386`). The "33 mutation" suite does the same with a hardcoded
> `aec3SuppressionGainDb = 25.0` (`adversarial-e2e-stress-suite.mjs:378-380`). Nothing was mutated in Rust.
> The line *"genuine native execution confirmed"* is contradicted by the files themselves: none of them
> spawns a process, opens a socket, or opens a database.
>
> **A sign-off is worth exactly what its check was worth.** Six APPROVEs on suites that cannot fail add up
> to no evidence at all — and stacked in a list like that, they read as *more* assurance than the one line
> that actually carries weight below.

**Verified status, 17/08/2026 — every number here was produced by running the command:**

| Gate | Result |
|---|---|
| `node scripts/e2e-test-suite.mjs` (real socket, §1–7) | **36 pass · 0 fail · 1 skip** |
| `node scripts/e2e-gateway-ci.mjs` | **8/8** |
| `node scripts/e2e-memory.mjs` (real gateway + on-disk SQLite + real model) | **6/6** |
| `node --test scripts/lib/memory-db.test.mjs` | **3/3** |
| `cargo test --no-fail-fast` | **894 pass · 0 fail · 5 ignored**, 43 binaries |
| `cargo clippy --all-targets` · `cargo fmt --all -- --check` | **0 warning · exit 0** |
| `npm run test:coverage -w liva-ui` | **444 pass / 44 files** · 79.31 % stmt · 81.53 % line |
| `npx eslint . --max-warnings 0` · `vue-tsc --noEmit` · `npm audit` | **0 · 0 · 0 vulnerabilities** |
| `cargo deny check` | advisories ok · licenses ok · sources ok |

**Known open item, stated rather than hidden:** the six simulation suites listed in the correction at the
top of this file still exist and still report green. They are not verification. Either wire them to the
real gateway the way `scripts/e2e/01`–`07` are wired, or keep them clearly labelled as design notes — but
do not let their counts back into a total.
