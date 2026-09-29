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

## Native Cargo Suites Added 17/09/2026 (not socket-E2E counts)

These are `cargo test` suites inside `liva-native-core`. Their counts must **not** be merged
into the live-socket numbers below — different runners, different proof.

| Suite | Nature | Verified 17/09/2026 |
|---|---|---|
| `dynamic_prompt_assembly_tests` | model-free unit (exact budget + legacy API) | 26/26 pass |
| `vision_context_guard_tests` | model-free unit | 21/21 pass |
| `completion_stream_tests` | model-free unit (stream cancellation, heartbeat, wire shape) | 4/4 pass (regression verified red before fix) |
| `budgeted_completion_entry_tests` | model-free unit (no-model / vocab-only entry) | 2/2 pass |
| `websocket_transport` + `integration_tests` | unit over transport/session logic | 8/8 + 7/7 pass |
| `prompt_budget_runtime` | opt-in `#[ignore]`, real model (release): `LIVA_TEST_MODEL_PATH=E:\AI_Models\gemma-4-E2B-it-Q4_K_M.gguf` | 1/1 pass × 2 runs (13.5 s / 13.3 s) — real inference through the exact-budget entry |
| `vision_budget_runtime` | opt-in `#[ignore]`, release | BLOCKED — no mmproj fixture on disk; explicit `--ignored` run fails loudly (exit 101, config message), never fake-passes |

Full lib run 17/09/2026 (debug): 735 passed / 1 failed — the single failure is the
pre-existing `preflight::tests::n_gpu_layers_bang_0_khong_bao_gio_la_xanh` (outside the changed files).
Root cause verified 17/09/2026 by reading `muc_vision`: the advice text is intentionally split
into a "no model on disk" branch (which must NOT suggest the env var, line 145–150 of preflight.rs)
and a "model present but layers=0" branch (which does, line 152–158), while the test at line 560
unconditionally asserts the string `LIVA_LLM_N_GPU_LAYERS`. It therefore only passes on machines
where the configured router GGUF exists — `data/liva-config.json` exists here but
`gemma-4-E4B-it-qat-GGUF/…UD-Q4_K_XL.gguf` was removed from disk. Not a runtime defect; whether
the test should branch on model presence is the file owner's call.

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
  cargo test -p liva-native-core --lib
  cargo test -p liva_desktop_lib
  ```
- **Empirical Adversarial Challenge Suites (Milestones M1–M3 Hardening)**:
  ```powershell
  cargo test -p liva-native-core --test m1_resilience_adversarial_challenge
  cargo test -p liva-native-core --test m1_audio_panic_adversarial_challenge
  cargo test -p liva-native-core --test m2_redaction_adversarial_challenge
  cargo test -p liva-native-core --test m2_memory_idle_adversarial_challenge
  cargo test -p liva-native-core --test m3_ipc_sync_adversarial_challenge
  cargo test -p liva-native-core --test command_authorization
  ```
- **Desktop UI Vitest Suite (47 Suites, 511 Tests)**:
  ```powershell
  npm run test -w liva-ui
  ```

---

## Coverage Summary

| Tier | Count | Description |
|------|------:|-------------|
| **Native Socket E2E** | 7 sections (36 socket assertions, 1 skip) + 8 CI tests | Direct RFC 6455 TCP WebSocket protocol assertions against live `liva-native-core.exe` |
| **Tier 1: Feature Coverage** | 29 scenarios + 511 UI tests + 713 Native unit tests | Isolated feature coverage across 5 Advanced Skills, Desktop UI (47 files), and Native Rust Core library |
| **Tier 2: Boundary & Hardening** | 31 scenarios + 51 empirical challenge tests + 9 command auth + 33 mutation tests | White-box stress-testing: DbActor burst (>1024 ops), vector rollback, VAD clamp, Decree 13 PII, multi-model idle unload, IPC ACL |
| **Tier 3: Cross-Feature Integration** | 5 multi-hop flows (22 invariants) | Multi-hop integration chaining UI IPC, Swarm DAG, AEC3 audio, OS sandbox, and Web Research |
| **Tier 4: Real-World Application** | 5 full workflows (33 invariants, 30 steps) | End-to-end user workload scenarios mirroring canonical LIVA application workflows |
| **Total — verification** | **36 socket + 8 CI + 6 memory + 3 crypto-unit + 511 UI + 713 Rust lib + 9 command auth + 51 M1–M3 challenge** | measured 13/09/2026, all green (>1,337 genuine assertions) |
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
| Native Gateway Protocol | Section 1, 6 | 8 CI tests | 36 socket assertions | ✓ | ✓ | **PASS** |
| Native Memory Persistence | Section 3 | 3 unit tests | SQLite WAL / AES-GCM | ✓ | ✓ | **PASS** |
| Voice & Audio Pipeline | Section 5 | WebRTC VAD | Denoise / AEC3 | ✓ (Flow 3) | ✓ (Scenario 4) | **PASS** |
| Skill Governance Linter | N/A | 52 skills | 58 vault notes | 0 err / 0 warn | ✓ | **PASS** |
| Desktop UI & IPC Bindings | N/A | 511 tests | 47 component suites | ✓ | ✓ | **PASS** |
| Native Rust Core | N/A | 713 unit tests | Concurrency & memory stress | ✓ | ✓ | **PASS** |
| Command Authorization & ACL | Section 2, 7 | 9 tests | Fail-closed principal isolation | ✓ | ✓ | **PASS** |
| M1: Resilience & Concurrency | Section 1, 6 | 7 tests (`m1_resilience`) | DbActor burst (>1024 ops), atomic vector rollback, WS accept recovery | ✓ | ✓ | **PASS** |
| M1: Audio Panic Elimination | Section 5 | 10 tests (`m1_audio_panic`) | VAD 32k clamp, DSP reset on turn/drop, Result::Err STT/VieNeu | ✓ | ✓ | **PASS** |
| M2: Decree 13 PII Redaction | N/A | 14 tests (`m2_redaction`) | CCCD 12-digit, VN mobile, bank accounts, deep JSON scrubbing | ✓ | ✓ | **PASS** |
| M2: Idle Memory Reclamation | N/A | 15 tests (`m2_memory_idle`) | Parakeet & VieNeu idle unload/lazy reload, non-blocking try_lock | ✓ | ✓ | **PASS** |
| M3: Desktop IPC Harmonization | N/A | 5 tests (`m3_ipc_sync`) | `audio_play_*` principal authorization, live WS no-op, burst stress | ✓ | ✓ | **PASS** |

---

## Verified Invariants Across Test Tracks

1. **RFC 6455 Real TCP Socket Communication**: All E2E socket tests connect to `ws://127.0.0.1:8099/ws` using standard Node.js `net.connect` streaming raw HTTP upgrade handshakes and binary/text WebSockets frames directly to the compiled native binary (`liva-native-core.exe`).
2. **FLOW-01 / SCENARIO-01 (OS Automation)**: Enforces 4-tier cognitive risk classification, mandatory operator two-phase confirmation on high-risk operations, and cryptographic audit ledger logging.
3. **FLOW-02 / SCENARIO-02 (Deep Research)**: Enforces domain rate-limiting, parallel crawling, hybrid vector deduplication, and structured Markdown synthesis with citation graphs.
4. **FLOW-03 / SCENARIO-03 (Code Refactor)**: Validates GitNexus PDG taint flow tracing, blast radius calculation, patch dry-run validation, and rollback safety.
5. **FLOW-04 / SCENARIO-04 (Multimodal Vision)**: Enforces bounding box normalization ($0..1$), ROI cropping, OCR/VLM reasoning, and anti-self wake WebRTC AEC3 audio gating.
6. **FLOW-05 / SCENARIO-05 (Workflow Swarm)**: Enforces Kahn DAG topological sorting, acyclicity checks, parallel branch execution, quorum voting consensus, and HITL checkpoints.
7. **Zero-Mock Cryptography & Storage Integrity**: Facts encryption utilizes AES-256-GCM authenticated encryption (`v2:salt:iv:tag:ciphertext`) with HKDF-SHA256 key derivation verified directly from SQLite storage.
8. **M1 DbActor Queue Discipline & Vector Upsert Atomicity**: All SQLite mutations serialize through `DbActorHandle` with queue capacity 1024 and backpressure tolerance under burst loads (>1200 ops). Multi-table vector upserts (`vectors_meta`, `vec_idx`, `vectors_fts`) execute within atomic transactions; failures in secondary tables or outer transaction rollbacks leave zero orphaned records. WebSocket accept loop recovers from TCP garbage and malformed handshakes with 50ms backoff without terminating the server task (`m1_resilience_adversarial_challenge.rs`).
9. **M1 VAD Memory Clamping & DSP Lifecycle Reset**: VAD residual buffer is strictly bounded to `MAX_RESIDUAL_CAPACITY = 32_000` samples (2.0s @ 16kHz); processing drains unaligned streaming chunks to `< 512` samples without memory accumulation. WebRTC actor resets VAD recurrent states, GTCRN denoiser caches, and AEC across turn boundaries, speech interruptions, and actor drop. Missing models and non-contiguous tensors return graceful `Result::Err` with zero panics (`m1_audio_panic_adversarial_challenge.rs`).
10. **M2 Vietnamese Decree 13 Compliance & PII Redaction**: `SecretScrubber::mask_secrets` masks Vietnamese Citizen ID (CCCD 12-digit with valid provincial/gender prefixes), Vietnamese phone numbers (mobile prefixes `03/05/07/08/09` and `+84`), and bank account numbers (9-16 digits following banking keywords and delimiters) across plain text, markdown, multiline audit logs, and deeply nested JSON structures while preserving UUIDs, timestamps, and general numbers (`m2_redaction_adversarial_challenge.rs`).
11. **M2 Multi-Model Idle Memory Reclamation & Non-Blocking Boot Guard**: Parakeet STT (~2.4GB) and VieNeu TTS (~500MB) models unload after 5 minutes of idle time. Boot Task #6 (`check_voice_idle_unload`) uses `try_lock()` to avoid blocking active audio streams. Parakeet lazily reinstantiates on the next incoming audio chunk and resets streaming flags cleanly. `get_memory_status` exposes real-time model residency, byte footprint, and reclamation eligibility under ACL gating (`m2_memory_idle_adversarial_challenge.rs`).
12. **M3 IPC Synchronization & Audio Event Principal Gating**: Desktop UI IPC events (`audio_play_started` and `audio_play_finished`) are registered in `authorization.rs` and permitted exclusively for `CommandPrincipal::WebSocketWidget` and `CommandPrincipal::TauriWidget`. Remote or dashboard principals fail closed (`CommandAuthorizationError`). Live WebSocket handlers execute audio events as non-blocking no-ops without emitting error frames, preserving socket stability under 200-event burst traffic and malformed payloads (`m3_ipc_sync_adversarial_challenge.rs`).

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

**Verified status, 13/09/2026 (Milestones M1–M4 Hardened & Verified) — every number here was produced by running the command:**

| Gate | Result |
|---|---|
| `node scripts/e2e-test-suite.mjs` (real socket, §1–7) | **36 pass · 0 fail · 1 skip** |
| `node scripts/e2e-gateway-ci.mjs` | **8/8** |
| `node scripts/e2e-memory.mjs` (real gateway + on-disk SQLite + real model) | **6/6** |
| `node --test scripts/lib/memory-db.test.mjs` | **3/3** |
| `cargo test -p liva-native-core --lib` | **713 pass · 0 fail · 3 ignored** (23.73s) |
| `cargo test -p liva-native-core --test command_authorization` | **9 pass · 0 fail** (0.02s) |
| `cargo test -p liva-native-core --test m1_resilience_adversarial_challenge` | **7 pass · 0 fail** (0.25s) |
| `cargo test -p liva-native-core --test m1_audio_panic_adversarial_challenge` | **10 pass · 0 fail** (0.42s) |
| `cargo test -p liva-native-core --test m2_redaction_adversarial_challenge` | **14 pass · 0 fail** (0.03s) |
| `cargo test -p liva-native-core --test m2_memory_idle_adversarial_challenge` | **15 pass · 0 fail** (21.31s) |
| `cargo test -p liva-native-core --test m3_ipc_sync_adversarial_challenge` | **5 pass · 0 fail** (0.03s) |
| `cargo clippy --all-targets` · `cargo fmt --all -- --check` | **0 warning · exit 0** |
| `npm run test -w liva-ui` | **511 pass / 47 files** (21.63s) |
| `npx eslint . --max-warnings 0` · `vue-tsc --noEmit` · `npm audit` | **0 · 0 · 0 vulnerabilities** |
| `cargo deny check` | advisories ok · licenses ok · sources ok |

**Known open item, stated rather than hidden:** the six simulation suites listed in the correction at the
top of this file still exist and still report green. They are not verification. Either wire them to the
real gateway the way `scripts/e2e/01`–`07` are wired, or keep them clearly labelled as design notes — but
do not let their counts back into a total.
