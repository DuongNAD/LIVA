# Project: LIVA Groups 1 & 2 Technical Hardening

## Architecture
- **Unified Native Engine in Rust (`liva-native-core`)**:
  - WebRTC Audio Subsystem: `webrtc::ring_buffer` provides lock-free SPSC single-producer single-consumer ring buffers with atomic CAS (`compare_exchange` / `compare_exchange_weak`) for playback and capture streams, eliminating pointer rollback during Barge-In.
  - Device Keystore: `keystore` handles DPAPI on Windows and HKDF-SHA256 machine-binding + AES-256-GCM authenticated encryption on macOS/Linux with POSIX 0600 file permissions and fail-closed corruption recovery.
  - Native Command Dispatcher & RBAC: `authorization` enforces allowlists (`DASHBOARD_COMMANDS`), and `commands::config` dispatches and executes native IPC calls (`import_avatar_folder`, `delete_avatar_model`, `test_skill`, `test_all_skills`, `toggle_skill`, `toggle_all_skills`).
  - Agent Engine & Graph: `agent::graph::diff_reviewer` manages sessions with RwLock poison recovery (`.unwrap_or_else(|e| e.into_inner())`).
- **Frontend & IPC (`liva-ui` & `packages/liva-common`)**:
  - Vue 3 + Vite + Tailwind UI communicating over Tauri IPC (`native_ipc_call`) and WebSocket.
  - Normalized schemas for avatar models in `AvatarGallery.vue` and resilient skill testing listeners with timeout protection in `SkillsView.vue`.
  - ESLint configuration ignoring build artifact targets (`target*`, `target_m4`, `target/**`).

## Feature Inventory
| # | Feature | Description | Milestone | Source | Status |
|---|---------|-------------|-----------|--------|--------|
| 1 | R1 (RUST-01): Audio SPSC Ring Buffer Concurrency fix | Atomic CAS (`compare_exchange`) in `pop_slice`, `flush_consumer`, `skip` to prevent pointer clobbering during Barge-In; multi-threaded stress tests | M1 | ORIGINAL_REQUEST §R1 | DONE |
| 2 | R2 (SEC-01): Device Keystore multi-platform support | macOS/Linux machine-bound AES-256-GCM sealing with POSIX 0600 permissions; eliminate `KeyError::Unsupported` on boot | M2 | ORIGINAL_REQUEST §R2 | DONE |
| 3 | R3 (FE-03): Unimplemented UI Commands Handling | Implement backend handlers & authorization for `import_avatar_folder`, `delete_avatar_model`, `test_skill`, `test_all_skills`, `toggle_skill`, `toggle_all_skills`; normalize avatar schemas & add UI timeouts | M3 | ORIGINAL_REQUEST §R3 | DONE |
| 4 | R4 (RUST-04): RwLock Poison Safety in Diff Reviewer | Replace `.write().unwrap()` and `.read().unwrap()` with `.unwrap_or_else(|e| e.into_inner())` across `DiffReviewRegistry` | M4 | ORIGINAL_REQUEST §R4 | DONE |
| 5 | R4 (FE-04): ESLint Build Artifact Ignores | Ignore `target*`, `target_m4`, `target/**` in `eslint.config.js` to prevent scanning build artifacts | M4 | ORIGINAL_REQUEST §R4 | DONE |
| 6 | R5: Architectural Integrity & Git Safety Boundaries | Obey AGENTS.md rules (no legacy Node/Python revival); staging only (`git add`), no auto commit/push | M1-M5 | ORIGINAL_REQUEST §R5 | DONE |

## Milestones
| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| 1 | M1: Ring Buffer Concurrency Fix (R1) | `liva-native-core/src/webrtc/ring_buffer.rs` | None | DONE |
| 2 | M2: Device Keystore Multi-Platform Support (R2) | `liva-native-core/src/keystore.rs`, `liva-native-core/src/lib.rs`, `liva-native-core/tests/crypto_boot_e2e.rs` | None | DONE |
| 3 | M3: Unimplemented UI Commands (R3) | `liva-native-core/src/authorization.rs`, `liva-native-core/src/commands/config.rs`, `packages/liva-common/src/types/websocket.ts`, `liva-ui/src/components/dashboard/AvatarGallery.vue`, `liva-ui/src/components/dashboard/SkillsView.vue` | None | DONE |
| 4 | M4: RwLock Poison Safety & ESLint Ignores (R4) | `liva-native-core/src/agent/graph/diff_reviewer.rs`, `eslint.config.js` | None | DONE |
| 5 | M5: Full Verification & Acceptance Gate | Full workspace test suites (`cargo check`, `cargo test`, `npm test`, `npx eslint`) | M1, M2, M3, M4 | DONE |

## Interface Contracts
### WebRTC RingBuffer ↔ Consumer Thread
- `pop_slice(&self, dst: &mut [T]) -> usize`: Copies up to `dst.len()` items. If concurrent `flush_consumer()` occurs during copy, CAS on `tail` fails, stale read is aborted, and returns 0.
- `flush_consumer(&self) -> usize`: Atomically CAS-advances `tail` to `head`, returning the count of discarded samples.
- `skip(&self, count: usize) -> usize`: Atomically advances `tail` by `min(count, available)`.

### Keystore ↔ LIVA Boot Lifecycle
- `load_or_create_device_key(db_path: &Path) -> Result<[u8; 32], KeyError>`:
  - Windows: DPAPI seal/unseal.
  - Non-Windows (macOS/Linux): HKDF-SHA256 machine identifier binding + AES-256-GCM + POSIX 0600 file permissions in `<data_dir>/.device_key`. Never returns `KeyError::Unsupported`.

### UI Frontend ↔ Native Core IPC
- Commands authorized in `DASHBOARD_COMMANDS`:
  - `import_avatar_folder`: `{ folderPath: string }` → `{ success: true, folderName: string, modelType: string }`
  - `delete_avatar_model`: `{ filename: string }` → `{ success: true, filename: string }`
  - `test_skill`: `{ name: string }` → `{ name: string, success: boolean, message: string, details: string, time: string }`
  - `test_all_skills`: `{}` → `{ success: boolean, total: number, passed: number, failed: number, results: [...] }`
  - `toggle_skill`: `{ name: string, enabled: boolean }` → `{ success: true, name: string, enabled: boolean }`
  - `toggle_all_skills`: `{ enabled: boolean }` → `{ success: true, enabled: boolean }`
