# Project: LIVA Native Engine Modernization & Native CUA Integration

## Architecture

LIVA's native engine (`liva-native-core`) is modernized to integrate native Computer-Use Agent (CUA) capabilities derived from `trycua/cua`'s `cua-driver` in Rust. The architecture enables non-intrusive desktop automation, safe sandboxed execution, and fast System-1 UI action routing with zero external non-Rust daemon dependencies, while respecting strict sequential compilation bounds (`-j 2`) and memory ceilings.

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                       LIVA DESKTOP CLIENT                                        │
│                 Tauri v2 · WebView2 / Vue 3.5 · Floating Widget & Dashboard UI                    │
├───────────────────────────────────────────────┬──────────────────────────────────────────────────┤
│                  User UI                      │                CUA Control & Monitor             │
│  - WidgetApp.vue & Chat UI                    │  - Floating Emergency Kill-Switch Button (Esc)   │
│  - 3D Avatar (Web Worker OffscreenCanvas)     │  - Permission Mode Selector (Standard/Bounded)   │
│  - Voice Duplex Audio (WASAPI / WebRTC AEC3)  │  - Live Audit Ledger Stream & Window Inspector   │
└───────────────────────┬───────────────────────┴──────────────────────────▲───────────────────────┘
                        │ Native Tauri v2 IPC (`cua:*` verbs)              │
                        ▼                                                  │
┌──────────────────────────────────────────────────────────────────────────┴───────────────────────┐
│                                LIVA NATIVE CORE ENGINE (Rust)                                    │
│                     (AppState Lifecycle Coordinator · Non-blocking Facade)                        │
├──────────────────────┬────────────────────────┬─────────────────────┬────────────────────────────┤
│       crates/        │        crates/         │       crates/       │          crates/           │
│   liva-core-types    │      liva-storage      │      liva-llm       │        liva-tools          │
│  - Domain Models     │  - Micro-Batched       │  - LlmActor (OS Thr)│  - Diagnostic Suite & CLI  │
│  - CUA Action Types  │    Single-Writer WAL   │  - Priority Queue   │  - CUA Benchmark & Test    │
│  - Security Verdicts │  - CUA Audit Ledger    │  - Real-time Stream │    Harness                 │
├──────────────────────┴────────────────────────┴─────────────────────┴────────────────────────────┤
│                         crates/liva-cua (Native CUA Subsystem)                                   │
│  ┌───────────────────────┬────────────────────────┬───────────────────────────────────────────┐  │
│  │     Driver Engine     │   Coordinate/Geometry  │            Security & Kill-Switch         │  │
│  │  - Window Enumeration │  - VirtualDesk Norm    │  - Permission Modes (Standard/Bounded/Unr)│  │
│  │    (EnumWindows+UIA)  │    (0..=65535, multi)  │  - Bounded Allowlist & Denylist Filter    │  │
│  │  - Non-Intrusive Input│  - Signed LPARAM Math  │  - Emergency Kill-Switch (<50ms SLA)      │  │
│  │    (PostMessageW, UIA │  - DWM Crop Inset      │  - Structured Audit Ledger (SQLite WAL)   │  │
│  │     fg_bypass, Pointer│  - Per-Monitor V2 DPI  │  - UIPI & Token Integrity Guard           │  │
│  │     Touch Injection)  │                        │                                           │  │
│  └───────────────────────┴────────────────────────┴───────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────────────────────────────────┤
│                           System-1 GUI Action Router & Vision                                    │
│  - Multimodal Screen Capture (`NativeScreenCapturer` / `xcap` with WGC)                          │
│  - Localized ROI Diff Verification (`DiffEngine::diff_region`, < 80ms)                           │
│  - Low-Latency Reactive Action Execution (< 15ms per step, > 95% LLM token savings)             │
│  - Deterministic Pre-Execution Checks (HWND liveness, occlusion, client bounds containment)      │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

---

## Feature Inventory

Every feature from the Survey phase is mapped below with its assigned milestone.

| # | Feature | Description | Milestone | Source |
|---|---------|-------------|-----------|--------|
| 1 | Window Enumeration & Inspection | Canonical Win32 `EnumWindows` z-order + UIA tree walker; DWM extended frame bounds with drop-shadow crop inset; UWP `ApplicationFrameHost` resolution; and state tracking | M1 | Survey R1 |
| 2 | Coordinate Translation & Multi-Monitor Virtual Desktop | Virtual desktop normalization (`virtualdesk.rs`, 0..=65535 with negative origins), signed 16-bit packed LPARAM math (`lparam.rs`), Per-Monitor V2 DPI awareness, and bitmap-to-screen coordinate alignment | M1 | Survey R1 |
| 3 | Non-Intrusive Background Input Synthesis | Multi-tier input actuator: UIA InvokePattern with `DisabledHwndGuard` (`EnableWindow(false)`), universal synthetic pointer (`PT_TOUCH`) injection via `InjectSyntheticPointerInput` with `NoActivateGuard` (`WS_EX_NOACTIVATE`), deepest-child `PostMessageW`, and focused-descendant keyboard routing | M1 | Survey R1 |
| 4 | Special Window State Fallbacks & UIPI Handling | Fail-closed detection and refusal for minimized/hidden windows (`IsIconic` sentinel); UIPI detection via process token integrity RID; off-screen element scrolling recovery; and target PID guards | M1 | Survey R1 |
| 5 | Multi-Tier Security Boundaries | Multi-tier policy engine: `Standard` (read-only/blocked input), `Bounded` (process allowlist, protected window denylist, coordinate containment, dangerous hotkey filtering), and `Unrestricted` | M2 | Survey R2 |
| 6 | Emergency Kill-Switch Subsystem (< 50ms SLA) | Multi-trigger abort mechanism (5ms Esc key listener / hook, Tauri IPC event, physical mouse displacement > 50px) triggering atomic halt (`EMERGENCY_HALT` SeqCst), Tokio cancellation token, queue flush, and synthetic key/button release in < 15ms | M2 | Survey R2 |
| 7 | Structured Audit Ledger Persistence | Append-only audit events (`timestamp`, `action_type`, `target_app`, `coordinates`, `permission_mode`, `security_verdict`, `latency_ms`) persisted asynchronously to SQLite WAL via `DbActor` and streaming JSONL | M2 | Survey R2 |
| 8 | Vision-Aligned Fast Action Router (System-1 GUI Loop) | Reactive action execution loop translating visual targets into atomic action grammar (`Click`, `DoubleClick`, `Scroll`, `TypeText`, `Hotkey`, `Drag`) with > 95% token savings, deterministic pre-checks, and localized post-execution ROI diff verification (< 80ms) | M3 | Survey R3 |
| 9 | Crate Modularization & Legacy Pruning | Establish dedicated `crates/liva-cua` workspace crate, wire into `AppState`, prune dead `src/evolution` behind `#[cfg(feature = "experimental")]`, remove empty stubs, and expose unified Tauri IPC commands (`cua:*`) with authorization | M4 | Survey R4 |
| 10 | Comprehensive Quality Gates & Anti-Regression Verification | Sequential compilation `-j 2`, sequential test execution `-- --test-threads 2`, RAM pre-flight check >= 4GB, zero regression across voice duplex, memory, and LLM pipelines | M4 | Survey R4 |

---

## Milestones

| # | Name | Scope | Dependencies | Status |
|---|------|-------|-------------|--------|
| M1 | Native CUA Desktop Automation Engine | Features 1, 2, 3, 4: Port cua-driver core abstractions to Rust crate/module; window enumeration, coordinate translation, background input synthesis, UIPI detection, and fallback handling | none | DONE |
| M2 | Sandboxing, Permission Modes & Emergency Kill-Switch | Features 5, 6, 7: Multi-tier security boundaries (Standard, Bounded, Unrestricted), < 50ms Emergency Kill-Switch (< 15ms achieved), and SQLite WAL structured audit ledger via DbActor | M1 | DONE |
| M3 | Vision-Aligned Fast Action Router (System-1 GUI Loop) | Feature 8: Interface screen capture / visual ROI diffing with low-latency System-1 action executor; atomic action grammar; deterministic pre/post checks (< 80ms diff); > 95% token reduction | M1, M2 | DONE |
| M4 | LIVA Core Modularization, Tauri IPC & System Quality | Features 9, 10: Crate modularization, AppState wiring, Tauri IPC commands (`cua:*`), dead legacy pruning (`src/evolution`), -j 2 sequential compilation, and zero-regression quality verification | M1, M2, M3 | DONE |

---

## Interface Contracts

### `liva-cua` ↔ `liva-native-core`

```rust
// Core Engine Interface
pub struct CuaEngine {
    pub config: CuaConfig,
    pub security_governor: Arc<SecurityGovernor>,
    pub kill_switch: Arc<KillSwitchController>,
    pub action_router: Arc<System1ActionRouter>,
    pub audit_ledger: Arc<AuditLedger>,
}

impl CuaEngine {
    pub fn new(config: CuaConfig, db: Option<DbActorHandle>) -> Self;
    pub async fn list_windows(&self, filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, CuaError>;
    pub async fn execute_action(&self, action: CuaAction) -> Result<CuaActionResult, CuaError>;
    pub fn trigger_emergency_halt(&self) -> Result<(), CuaError>;
    pub fn set_permission_mode(&self, mode: PermissionMode) -> Result<(), CuaError>;
    pub fn get_status(&self) -> CuaStatus;
}
```

### `liva-native-core` ↔ `Tauri IPC`

```json
// Endpoints exposed in authorization.rs (Principal: TauriDashboard, TauriWidget)
// "cua:list_windows"      -> { "windows": [...] }
// "cua:execute_action"    -> { "action": CuaAction } -> { "result": CuaActionResult }
// "cua:emergency_stop"    -> {} -> { "halted": true, "latency_ms": 12 }
// "cua:set_mode"          -> { "mode": "Bounded", "allowlist": ["notepad.exe"] }
// "cua:get_status"        -> { "mode": "Bounded", "halted": false, "active_actions": 0 }
// "cua:query_audit_logs"  -> { "limit": 50 } -> { "events": [...] }
```

---

## Code Layout

- `crates/liva-cua/Cargo.toml`: Package manifest with `windows-sys = "0.52.0"` (`Win32_Foundation`, `Win32_UI_WindowsAndMessaging`, `Win32_UI_Input_KeyboardAndMouse`, `Win32_UI_HiDpi`, `Win32_Graphics_Gdi`).
- `crates/liva-cua/src/`:
  - `lib.rs`: Public crate API (`CuaEngine`, `CuaConfig`, `CuaError`).
  - `types.rs`: Data models (`CuaWindowInfo`, `CuaAction`, `CuaActionResult`, `PermissionMode`).
  - `window.rs`: Win32 `EnumWindows` + UIA top-level window enumeration and UWP frame resolution.
  - `geometry/`: `virtualdesk.rs` (0..=65535 multi-monitor math), `lparam.rs` (signed 16-bit packing), `dpi.rs`.
  - `input/`: `mod.rs` (actuation router), `win32_msg.rs` (deepest-child PostMessage & focused keyboard), `pointer.rs` (synthetic pointer injection with `WS_EX_NOACTIVATE`), `uipi.rs` (token integrity check).
  - `security.rs`: Multi-tier `SecurityGovernor`, allowlist/denylist filter, coordinate containment.
  - `kill_switch.rs`: < 50ms Emergency Kill-Switch, `AtomicBool` halt flag, Esc key listener thread, mouse thrash detector.
  - `audit.rs`: Structured append-only audit ledger with SQLite WAL persistence.
  - `router.rs`: Low-latency System-1 action executor, pre-execution checks, post-execution ROI diffing.
- `liva-native-core/src/`:
  - `lib.rs`: Wires `CuaEngine` into `AppState`, gates `evolution` behind `experimental`.
  - `authorization.rs`: Adds `CUA_COMMANDS` to `TauriDashboard` / `TauriWidget` allowlist.
  - `commands/cua.rs`: IPC command handlers for `cua:*` verbs.
