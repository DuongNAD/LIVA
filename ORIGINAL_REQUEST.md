# Original User Request

## Initial Request — 2026-06-08T01:11:16+07:00

Implement and integrate key architectural upgrades into the LIVA engine, focusing on optimizing compute efficiency (Speculative Decoding and Persistent Prompt Caching) and enhancing memory retrieval and compaction (Hybrid Query Routing and Idle-Time Consolidation).

Working directory: e:\project\openclaw_remake
Integrity mode: development

## Requirements

### R1. Compute Engine Upgrades (Speculative Decoding & Prompt Caching)
- Integrate speculative decoding in the local native inference core (using a smaller draft model like Qwen-0.5B alongside the target model).
- Implement persistent system-prompt caching at the gRPC/IPC layer to prevent recalculating the system instructions on subsequent conversational turns, reducing Time-to-First-Token (TTFT) to near-zero.

### R2. Memory Subsystem Upgrades (Hybrid Routing & Background Consolidation)
- Build a hybrid routing layer using a small language model (SLM) or embedding similarity to analyze query intent and direct queries to either the Knowledge Graph (SQLite) or Vector Database (sqlite-vec) to avoid redundant querying.
- Implement an idle-time memory consolidation daemon/cron-job that runs in the background when the system is idle, clustering and summarizing short-term memories from the conversational turns into long-term structured knowledge (meta-entities) and cleaning cache.

### R3. Codebase Integration & Compliance
- Ensure all additions comply strictly with LIVA's code architecture constraints defined in `AI_CONTEXT.md` (e.g., no blocked event loops, use of `safeFetch`, SQLite debounced writes, Pino logging, and TypeScript strict safety).
- Ensure existing Tauri and Gateway functionalities remain fully operational and backward-compatible.

## Acceptance Criteria

### Compute Engine Verification
- [ ] Speculative decoding can be toggled on/off via configuration. When enabled, token generation speed (tokens/second) shows a measurable increase (at least 1.3x speedup on standard prompts).
- [ ] System prompt KV cache is successfully retained across consecutive messages, showing a TTFT of under 100ms on subsequent turns.

### Memory Subsystem Verification
- [ ] The hybrid routing layer successfully classifies user intents into target data sources (e.g., code/project logic -> Knowledge Graph; conversational memory/old experience -> Vector DB) with at least 90% accuracy on a test suite of 20 benchmark prompts.
- [ ] The idle consolidation job activates after a configured idle interval, correctly clusters short-term memory nodes, updates the SQLite DB, and does not leak memory or block the main Node.js event loop.

### Integration Verification
- [ ] All existing and new integration tests in the gateway build and pass successfully (`npm run test:gateway`).
- [ ] An automated verification script (`npm run test:upgrades` or similar) is provided in the repository to programmatically verify the speculative decoding throughput and routing accuracy.

---

## Initial Request — 2026-06-10T16:46:36+07:00

Nghiên cứu, đánh giá, và nâng cấp dự án LIVA — một hybrid-intelligence desktop AI assistant — bằng cách phân tích toàn bộ cải tiến từ nhánh `origin/mac` (8 commits, 77 files changed, ~15,684 additions) và port những tối ưu phù hợp vào nhánh `main` (Windows). Đồng thời nghiên cứu và đề xuất các tính năng mới để tối ưu thêm hiệu suất hệ thống.

**Mục đích**: Eval — đo benchmark và kiểm tra kỹ trước khi đưa vào production.

Working directory: e:\Project\LIVA
Integrity mode: development

**CRITICAL**: Đọc kỹ `AI_CONTEXT.md` trước khi viết code. Tuân thủ 100% coding standards (safeFetch, pino logger, no console.log, no blocking I/O, worker threads cho >10ms CPU tasks, zod v4+ validation, true private `#` fields).

**Branch Context**: Nhánh `origin/mac` chứa các cải tiến đã được test (228/228 vitest, 81/81 pytest, 2457/2457 tests passed) bao gồm:
- Speculative decoding integration (draft model support)
- Dynamic thread allocation (P-core / E-core aware)
- Hybrid routing upgrades (VirtualManager: kg_recall, deep_reasoning, graph search)
- ConsolidationCron hardening (true `#` private fields, VRAM guards)
- AgentLoop engine warm-up wait (90s dynamic loop)
- Cross-platform path resolution (Windows/macOS)
- Python native engine upgrades (1646 lines changed)
- Upgrade test suites (caching, consolidation, routing, speculative)
- `consolidated_docs.md` chứa llama.cpp wiki documentation
- Performance report (M25) với benchmark results trên Apple Silicon

## Requirements

### R1. Evaluate & Port mac Branch Optimizations to Windows main
Phân tích toàn bộ 77 files đã thay đổi trong `origin/mac` (so với `main`). Đánh giá từng cải tiến và quyết định những gì phù hợp để port vào Windows `main`. Ưu tiên:
- **ModelOrchestrator**: Dynamic thread allocation, speculative decoding (draft model args), flash-attn auto, startup timeout 90s, warm-up state tracking (`#isWarmingUp`), auto-restart on clean exit
- **VirtualManager**: Route-based parallel I/O (kg_recall → graph search, deep_reasoning → vector + graph), `#searchGraph()` multi-hop with AsyncChunker, `isWholeWordMatch()` Vietnamese boundary detection
- **AgentLoop**: Engine warm-up wait loop, speculative cache validation (partialText match), better fallback error handling ("no cloud fallback configured")
- **ConsolidationCron**: Migrate ALL fields to true `#` private, add VRAM guards to `consolidateNow()`, `isRunning` getter, test accessor methods
- **SemanticRouter**: Route classification improvements
- **AutoGPUSetup & SystemHealth**: Cross-platform hardware detection

Bỏ qua các thay đổi chỉ liên quan macOS mà không áp dụng được cho Windows (ví dụ: `sysctl` commands, macOS memory pressure detection, MLX framework). Sử dụng lệnh `git diff main..origin/mac -- <file>` để xem chi tiết thay đổi.

### R2. Update consolidated_docs.md with llama.cpp Wiki
Cập nhật `consolidated_docs.md` trong nhánh `main` với tài liệu llama.cpp wiki mới nhất từ `origin/mac`. Đây là reference material quan trọng về speculative decoding, KV cache optimization, quantization, GBNF grammars, build instructions, và server API. Lấy nội dung từ `git show origin/mac:consolidated_docs.md`.

### R3. Port & Adapt Upgrade Test Suites
Port 4 test suites mới từ `origin/mac` vào `main`:
- `tests/upgrades/caching.test.ts` — Prompt caching verification
- `tests/upgrades/speculative.test.ts` — Speculative decoding throughput tests
- `tests/upgrades/routing.test.ts` — Hybrid routing accuracy tests
- `tests/upgrades/consolidation.test.ts` — Idle consolidation stability tests

Điều chỉnh tests cho Windows environment (PowerShell paths, Windows GPU detection thay vì Metal). Tất cả tests phải pass với `npx vitest run`.

### R4. Research & Propose New Optimizations
Nghiên cứu và đề xuất ít nhất 3 cải tiến mới chưa có trong cả hai nhánh. Các hướng nghiên cứu gợi ý:
- Response streaming latency optimization
- VRAM utilization efficiency improvements
- Context window management / token budget optimization
- Memory system (L1/L2/L3) performance tuning
- Startup time reduction
- Cold start optimization for llama-server

Implement ít nhất 1 cải tiến đã đề xuất. Cung cấp benchmark trước/sau để chứng minh hiệu quả.

### R5. Codebase Compliance & Regression Testing
Đảm bảo tất cả thay đổi tuân thủ nghiêm ngặt `AI_CONTEXT.md`:
- Zero `console.log` — chỉ dùng `logger` (pino)
- Zero blocking I/O trên main thread
- `safeFetch()` cho mọi network calls
- True private `#` fields cho process handles và timers
- `clearTimeout` trong `finally` block
- Tất cả existing tests vẫn pass sau khi port

## Acceptance Criteria

### Port Quality
- [ ] Chạy `git diff main..origin/mac -- <file>` cho mỗi file cần port, ghi lại danh sách files đã port và files đã bỏ qua (kèm lý do)
- [ ] Tất cả private fields trong `ConsolidationCron.ts` sử dụng true `#` private syntax
- [ ] `ModelOrchestrator` hỗ trợ speculative decoding config (`LIVA_ENABLE_SPECULATIVE`, `LIVA_DRAFT_MODEL_NAME`) với graceful fallback khi draft model không tồn tại
- [ ] `VirtualManager` có `#searchGraph()` method với route-based parallel I/O
- [ ] `AgentLoop.handleUserInput()` returns `Promise<void>` và có engine warm-up wait loop

### Testing
- [ ] Tất cả existing tests pass: `npx vitest run` trong `liva-gateway/` — ZERO failures
- [ ] 4 upgrade test files mới tồn tại và pass trong `liva-gateway/tests/upgrades/`
- [ ] Chạy `npx tsc --noEmit` trong `liva-gateway/` — ZERO type errors

### Documentation & Wiki
- [ ] `consolidated_docs.md` được cập nhật với llama.cpp wiki content từ mac branch
- [ ] `AI_CONTEXT.md` được cập nhật để phản ánh các tính năng mới (speculative decoding, hybrid routing improvements)

### New Optimizations
- [ ] Báo cáo nghiên cứu chứa ít nhất 3 đề xuất tối ưu mới với phân tích khả thi
- [ ] Ít nhất 1 cải tiến mới được implement với benchmark before/after
- [ ] Tất cả code mới tuân thủ `AI_CONTEXT.md` coding standards

### Compliance Verification
- [ ] Zero violations: chạy `grep -r "console.log" liva-gateway/src/ --include="*.ts" | grep -v "node_modules" | grep -v ".test."` trả về empty
- [ ] Chạy `npx vitest run` output cho thấy >= 2457 tests passed (không giảm so với baseline)

---

## Follow-up — 2026-06-10T19:21:52+07:00

Nghiên cứu nâng cấp hệ thống quản lý kỹ năng (skills) và phát triển các kỹ năng mới cho LIVA dựa trên thiết kế hiện tại và tham khảo các tính năng từ dự án OpenClaw.

Working directory: e:\Project\LIVA

## Requirements

### R1. Nâng cấp hệ thống tải kỹ năng (Skill Loader Upgrade)
- Tích hợp cơ chế nạp động / hot-reloading kỹ năng (tương tự lệnh `openclaw skills reload`) để tự động phát hiện, xác thực (Zod schema) và nạp lại các kỹ năng mới hoặc sửa đổi trong thư mục `src/skills` mà không cần khởi động lại toàn bộ tiến trình Gateway.
- Đảm bảo cơ chế reload đóng các kết nối cũ một cách an sau, không gây rò rỉ bộ nhớ hoặc trùng lặp event listeners.

### R2. Phát triển thêm các kỹ năng mới lấy cảm hứng từ OpenClaw
- Triển khai tối thiểu 2 kỹ năng mới tích hợp sâu dựa trên OpenClaw, ví dụ:
  - **GitHub Operator**: Tương tác với GitHub API (quản lý issue, pull request, kiểm tra trạng thái repo).
  - **Google Calendar/Scheduler**: Tự động đồng bộ lịch trình cá nhân.
  - **Spotify/Media controller**: Quản lý nhạc và trình phát media cục bộ.
- Các kỹ năng mới phải có đầy đủ định nghĩa metadata chuẩn `AgentSkill` và schema validation.

### R3. Kiểm thử và Tài liệu hóa
- Bổ sung Unit/Integration tests đầy đủ cho cơ chế Hot-reload và các kỹ năng mới tạo ra.
- Đảm bảo toàn bộ mã nguồn kiểm thử chạy thành công (`npm run test` hoặc `vitest` pass 100%).
- Cập nhật tài liệu hướng dẫn phát triển kỹ năng để cộng đồng dễ dàng đóng góp.

## Acceptance Criteria

### Cơ chế Hot-Reload
- [ ] Khi thêm hoặc chỉnh sửa một file trong thư mục `src/skills`, `LocalMCPServer` tự động phát hiện sự thay đổi, biên dịch/xác thực lại Zod schema, và cập nhật thành công vào danh sách MCP tools có sẵn.
- [ ] Quá trình nạp lại kỹ năng không làm gián đoạn các kết nối WebSocket client hiện có và không rò rỉ tài nguyên.

### Kỹ năng mới (GitHub / Calendar / Spotify...)
- [ ] Triển khai thành công ít nhất 2 kỹ năng mới, có thể gọi trực tiếp thông qua Gateway MCP client.
- [ ] Có đầy đủ unit tests chứng minh tính đúng đắn của logic xử lý và xử lý lỗi (ví dụ: lỗi kết nối mạng, API token không hợp lệ).

---

## Follow-up — 2026-06-10T12:22:56Z

The user has confirmed that the target is a production-grade, stable system for daily use (Production) and there are no restrictions on code generation methods (integrity_mode: development). Please ensure the skill hot-reload mechanism and all new skills are implemented with maximum robustness, proper resource cleanup (preventing memory leaks on reload), error logging, and high test coverage to guarantee long-term stability.

## Follow-up — 2026-06-10T16:18:05+07:00

Implement the remaining Windows-specific hardware and user experience optimizations (Energy-Aware Eco Mode and Spatial Cross-Device Handoff/Presence Detector) on the LIVA Windows (`main`) branch.

Working directory: e:\Project\LIVA

## Requirements

### R1. Energy-Aware Eco Mode Integration
- Integrate `PowerMonitorService` with `ProactiveDaemon` to stop background digest scraping when running on battery or low battery (<30%).
- Hook `PowerMonitorService` with `CoreKernel.yieldVRAM()` to automatically release local VRAM (kill local llama-server) and force 100% Cloud API routing when unplugged, and restore it once plugged back in.
- Reduce UI avatar render FPS to 5 when Eco Mode is active.

### R2. Spatial Cross-Device Handoff (PresenceDetector)
- Implement `PresenceDetector` on Windows to monitor mouse and keyboard idle time.
- If system is idle > 3 minutes, set `PRESENCE = AWAY` and mute all desktop audio (TTS) and UI Toasts.
- Automatically reroute agent responses to the user's Telegram via `TelegramManager` when the user is away.
- Restore `PRESENCE = ACTIVE` and resume desktop output seamlessly upon mouse/keyboard movement detection.

## Acceptance Criteria

### Eco Mode Verification
- [ ] Transitioning to battery power triggers `VRAMGuard.yield()` and halts `ProactiveDaemon` scraping.
- [ ] Plugging in AC power restores local LLM warming and resumes background scraping.

### Presence Detector Verification
- [ ] System idle for 3 minutes mutes desktop output and routes a test notification to Telegram.
- [ ] User activity (mouse/keyboard input) instantly restores active status and unmutes desktop output.
- [ ] Pytest and Vitest test suites pass successfully.
