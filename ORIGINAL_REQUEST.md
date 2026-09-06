# Original User Request

## 2026-09-02T18:06:57Z

Thực hiện giải quyết các vấn đề kỹ thuật Nhóm 1 & 2 của dự án LIVA: RUST-01 (Audio Ring Buffer Concurrency khi Barge-in), SEC-01 (Device Keystore đa nền tảng macOS/Linux), FE-03 (Bổ sung/graceful fallback cho các lệnh UI Avatar & Skills), RUST-04 (An toàn RwLock poison trong Diff Reviewer), và FE-04 (ESLint ignore build artifacts). Đảm bảo toàn bộ test suites và kiểm tra tĩnh đều vượt qua 100%.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Khắc phục Race Condition trong Audio SPSC Ring Buffer (RUST-01)
- Trong `liva-native-core/src/webrtc/ring_buffer.rs`, thiết kế lại việc cập nhật con trỏ `tail` trong các hàm flush (`flush_consumer`, `flush_playback`) và đọc (`pop_slice`) sử dụng phép toán nguyên tử Compare-And-Swap (`compare_exchange`) hoặc cờ `AtomicBool` thông báo ngắt để luồng consumer tự dọn dẹp trước khi đọc, loại bỏ hoàn toàn khả năng ghi đè con trỏ sai lệch khi người dùng nói chen ngang (Barge-in).
- Bổ sung test case kiểm thử concurrency stress test đa luồng cho trường hợp flush đồng thời với `pop_slice`.

### R2. Hỗ trợ Device Keystore Đa Nền Tảng macOS & Linux (SEC-01)
- Trong `liva-native-core/src/keystore.rs` và `lib.rs`, bổ sung nhánh xử lý an toàn cho hệ điều hành non-Windows (`cfg(not(windows))`).
- Thay vì trả về `Err(KeyError::Unsupported)` làm crash quá trình boot, triển khai cơ chế sinh và lưu trữ khóa thiết bị cục bộ với phân quyền an toàn (file permissions `0600` trên Unix) trong thư mục cấu hình chuẩn của LIVA, bảo vệ dữ liệu khóa bằng hashing hoặc khóa máy chủ sở hữu, cho phép ứng dụng khởi động mượt mà out-of-the-box trên macOS và Linux mà không bắt buộc người dùng cấu hình thủ công `LIVA_ENCRYPTION_KEY`.

### R3. Xử lý các Lệnh UI Chưa Có Handler (FE-03)
- Đối với các lệnh giao diện chưa có handler backend:
  - `AvatarGallery.vue`: `import_avatar_folder`, `delete_avatar_model`.
  - `SkillsView.vue`: `test_skill`, `test_all_skills`, `toggle_skill`, `toggle_all_skills`.
- Cài đặt handler phản hồi chuẩn trong `liva-native-core/src/commands/` hoặc áp dụng graceful fallback trên frontend (hiển thị thông báo trạng thái hoặc gắn trạng thái chờ/disabled với tooltip rõ ràng), đảm bảo không gây treo giao diện hay crash IPC call.

### R4. Gia cố Concurrency & Cấu hình Linter (RUST-04, FE-04)
- Trong `liva-native-core/src/agent/graph/diff_reviewer.rs`, thay thế toàn bộ các lời gọi `.write().unwrap()` và `.read().unwrap()` trên `RwLock` bằng cách xử lý an toàn `.unwrap_or_else(|e| e.into_inner())` để ngăn chặn panic dây chuyền nếu một luồng bị panic trước đó.
- Trong `eslint.config.js`, bổ sung `target*`, `target_m4`, `target/**` vào mảng `ignores` để ngăn ESLint quét nhầm các file build artifact.

### R5. Quy chuẩn Kiến trúc & Ranh giới Git
- Tuân thủ nghiêm ngặt nguyên tắc trong `AGENTS.md` (không hồi sinh mã legacy Node.js/Python).
- Ranh giới Git: Chỉ dừng ở staging (`git add`), không tự ý chạy `git commit` hay push.

## Acceptance Criteria

### Verification & Automated Testing
- [ ] `cargo check --workspace` thực thi thành công 0 lỗi.
- [ ] `cargo test -p liva-native-core` pass 100% các bài test (bao gồm các test concurrency mới của `ring_buffer` và `keystore`).
- [ ] Khởi động/kiểm thử Keystore trên macOS thành công mà không nhận lỗi `KeyError::Unsupported`.
- [ ] `npm test` trong `liva-ui` tiếp tục pass 100% 490/490 tests.
- [ ] Lệnh `npm run lint` hoặc `npx eslint` không còn quét nhầm các tệp trong thư mục `target*`.
- [ ] Không có file mã nguồn nào bị phá vỡ kiến trúc hoặc rò rỉ logic legacy.

## 2026-09-03T04:56:55Z

Requested team: Full multi-agent team (parallel audit, reset, skill enhancement, and verification)

Comprehensive system audit, workspace reset, and agent skills enhancement for the LIVA Unified Native Engine to ensure peak operational readiness and reliability.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Safe Workspace Reset & Run-Ready Hygiene
Safely purge obsolete and redundant build artifacts (specifically extraneous build directories such as `target_m3` and `target_m4`, along with root transient `.log` files), reset locks, and verify configuration templates (`.env.example`) and local SQLite WAL database paths so that the workspace is pristine and immediately runnable.

### R2. Native Core & Stack Health Audit
Execute deep diagnostics across `liva-native-core` and `liva-desktop`. Fix any compilation errors or critical warnings, ensure database WAL connection pool stability, and verify all native Rust unit and integration test suites pass without regressions. Adhere strictly to the completed Rust migration (no legacy Node.js/Python gateway restoration).

### R3. Agent Skills Ecosystem Expansion & Standardization
Add two new production skills:
1. `liva-engine-ops`: Skill for managing the Native Engine Daemon lifecycle, checking runtime health, diagnosing IPC channels, and inspecting SQLite WAL database states.
2. `liva-perf-audit`: Skill for latency profiling, memory benchmarking, and automated regression assessment.
Audit and upgrade the existing skills (`liva-skill-governance` and `liva-technical-debt-triage`), ensuring bidirectional parity between `.agents/skills` and `.claude/skills` compliant with LIVA governance standards.

### R4. Guardrails & Autonomous Safety Protocol
Adhere strictly to LIVA agent guidelines: all file edits must respect Git boundary constraints (no autonomous `git commit`, `git push`, or `git checkout`), verify impacts, and provide a clear, structured operational verification report.

## Acceptance Criteria

### Workspace Hygiene & Build Readiness
- [ ] Extraneous targets (`target_m3`, `target_m4`) and orphan root logs (`e2e_run.log`, `liva_native_engine_daemon.log`, etc.) are cleaned or archived.
- [ ] `cargo check --workspace` and `cargo check -p liva-native-core` compile with 0 errors.
- [ ] Rust test suite (`cargo test -p liva-native-core`) passes with all existing tests green.

### Skills Governance & Ecosystem
- [ ] `liva-engine-ops` skill is created with complete `SKILL.md` specifications, scripts/references, and valid YAML frontmatter.
- [ ] `liva-perf-audit` skill is created with complete `SKILL.md` specifications and automated verification guidance.
- [ ] Parity between `.agents/skills` and `.claude/skills` is verified with 0 schema or metadata violations.
- [ ] Existing skills (`liva-skill-governance`, `liva-technical-debt-triage`) are refreshed and validated against latest project state.

### Verification & Delivery
- [ ] A final run-ready verification report (`RUN_READY_REPORT.md` or walkthrough artifact) is produced detailing system health, clean launch steps, and new skill capabilities.
