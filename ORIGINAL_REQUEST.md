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

## Follow-up — 2026-06-10T14:49:37Z

Sửa đổi các vi phạm Zero-Any, lỗi race condition trong VADWorker và tích hợp WriteValidationGate kiểm tra mâu thuẫn bộ nhớ trong ConsolidationSteps.

Working directory: e:\Project\LIVA
Integrity mode: development

## Requirements

### R1. Xóa bỏ các vi phạm kiểu `any` và xử lý Race Condition trong VADWorker
- **VADWorker (`liva-gateway/src/workers/VADWorker.ts`)**:
  - Loại bỏ các khai báo `any` lỏng lẻo (`session`, `h`, `c`). Sử dụng kiểu rõ ràng từ `onnxruntime-web` (`InferenceSession`, `Tensor`).
  - Sửa lỗi Race Condition khi luồng âm thanh dồn dập trong hàm `processAudio` bằng cách triển khai hàng đợi FIFO (`audioQueue`) để xử lý tuần tự.
- **WhisperNode (`liva-gateway/src/services/WhisperNode.ts`)**:
  - Thay thế ép kiểu lỏng lẻo `(err as any).cause?.message` bằng kiểm tra cấu trúc nghiêm ngặt của `Error` có `cause`.
  - Thay đổi cổng mặc định kết nối Native Engine từ 8100 sang 8101 (đồng bộ với mặc định của Python).
- **VectorRepository (`liva-gateway/src/memory/VectorRepository.ts`)**:
  - Định nghĩa rõ các interface kết quả cho truy vấn SQLite thay vì sử dụng ép kiểu `as any` khi lấy dữ liệu dòng (`rows`).

### R2. Di chuyển và Tích hợp WriteValidationGate
- Di chuyển file `WriteValidationGate.ts` từ `liva-gateway/src/incubating/` về lại `liva-gateway/src/memory/`.
- Cập nhật các dẫn import liên quan sau khi di chuyển.
- Trong `liva-gateway/src/memory/ConsolidationSteps.ts` (hoặc nơi ghi nhận dữ liệu thực tế), import và tích hợp kiểm tra mâu thuẫn bộ nhớ bằng cách gọi `WriteValidationGate.getInstance().validateUpdate(...)` trước khi thực hiện ghi. Ngăn chặn ghi đè nếu phát hiện mâu thuẫn.

### R3. Kiểm thử và Đồng bộ hóa
- Mở khóa bài test (`tests/memory/WriteValidationGate.test.ts`), thay đổi các hàm mock NLI bằng cách gọi thông qua Router sẵn có.
- Đảm bảo toàn bộ dự án biên dịch thành công (`npm run typecheck` hoặc `tsc --noEmit` sạch lỗi) và toàn bộ unit test vượt qua 100%.

## Acceptance Criteria

### Hệ thống loại bỏ Any & Race Condition
- [ ] File `VADWorker.ts`, `WhisperNode.ts`, và `VectorRepository.ts` không còn bất kỳ vi phạm `any` hay ép kiểu `as any` không an toàn nào.
- [ ] VADWorker hoạt động tuần tự chính xác khi nhận các khung âm thanh dồn dập qua hàng đợi FIFO, không xảy ra xung đột dữ liệu trạng thái.

### Tích hợp WriteValidationGate
- [ ] `WriteValidationGate.ts` được chuyển thành công về thư mục `src/memory/`.
- [ ] Tiến trình củng cố bộ nhớ trong `ConsolidationSteps.ts` sẽ bỏ qua việc lưu trữ nếu `validateUpdate` trả về kết quả mâu thuẫn hoặc không an toàn.
- [ ] Các test cases cho `WriteValidationGate` hoạt động đầy đủ và chạy thành công không có lỗi bỏ qua (`.skip`).

## Follow-up — 2026-06-11T07:08:13Z

Eliminate log pollution, memory leaks, and database worker crashes during migration initialization and system shutdown in the LIVA Gateway.

Working directory: e:/Project/LIVA/liva-gateway
Integrity mode: development

## Requirements

### R1. Idempotent Column Alterations
Prevent duplicate column execution error logs by explicitly checking if columns already exist (e.g. via `PRAGMA table_info`) before executing `ALTER TABLE ADD COLUMN` statements.
Apply this to:
- `obsolete` in `l3_edges` (`GraphRepository.ts`)
- `source_event_ids` in `vectors_meta` (`VectorRepository.ts`)
- Fact tracking columns in `facts` (`StructuredMemory.ts`)

### R2. Safe Shutdown / Teardown
Introduce safe checks and timers cleanups to prevent resource leakage on shutdown:
- Introduce a shutdown/close state check (`#isClosed` flag) in `StructuredMemory.ts` to ignore `upsertVector` and `touchFact` calls once the database is closing.
- In `DatabaseWorkerBridge.ts`, prevent warning logs and automatic recovery attempts on exit if the worker is being intentionally disposed (`#isDisposed === true`).
- Clear `#vectorQueueTimer` properly during `StructuredMemory.close()`.

### R3. EventEmitter Memory Leaks & Test Pollution
Resolve listener accumulation and stopped actor warnings in the CoreKernel test suite:
- Reset static singleton instances in the dependency injection container (`DependencyContainer.ts`) between test cases.
- Properly register and unregister event listeners in `CoreKernel.ts` using named handlers rather than anonymous closures.
- Guard the state machine actor in `AgentLoop.ts` to prevent sending events (like `BARGE_IN`) to stopped actors.
- Wrap test kernel creations in `try..finally` blocks to guarantee `shutdown()` is called.

### R4. Isolated Stress-Test Database
Ensure that the memory stress test (`tests/memory-stress.test.ts`) is isolated and does not read/write to the default global database path `data/global/structured_memory.sqlite`, avoiding concurrent write conflicts with other tests.

## Acceptance Criteria

### Test Execution
- [ ] All Vitest tests in `liva-gateway` must pass 100% green.
- [ ] The console and test logs must be completely free of:
  - SQLite `duplicate column name` database worker errors
  - `Database worker exited with code 1` warnings on intentional close
  - `MaxListenersExceededWarning` listener leak warnings
  - Event sent to stopped actor `BARGE_IN` warnings

## Follow-up — 2026-06-18T09:02:32+07:00

Sửa lỗi dimension mismatch khi truyền dữ liệu âm thanh vào mô hình Nemotron 3.5 ASR ONNX trong liva-gateway.

Working directory: E:\Project\LIVA
Integrity mode: development

## Requirements

### R1. Khắc phục lỗi tensor shape trong NemotronSTT
- Sửa lỗi tensor shape của `audio_signal`, `cache_last_channel`, và `cache_last_time` trong [NemotronWorker.ts](file:///e:/Project/LIVA/liva-gateway/src/workers/NemotronWorker.ts).
- `audio_signal` mong đợi shape là `[1, 65, 128]` (65 frames log-mel spectrogram, mỗi frame có 128 mels).
- `cache_last_channel` mong đợi shape là `[1, 24, 56, 1024]`.
- `cache_last_time` mong đợi shape là `[1, 24, 1024, 8]`.

### R2. Tích lũy và chia chunk âm thanh (Chunking/Overlap)
- Thay đổi cơ chế gom nhóm dữ liệu âm thanh đầu vào trong [NemotronWorker.ts](file:///e:/Project/LIVA/liva-gateway/src/workers/NemotronWorker.ts) sao cho mỗi lần chạy encoder ONNX, dữ liệu đầu vào luôn có độ dài tương ứng chính xác 65 frames (10640 samples).
- Sử dụng cơ chế overlap shift là 56 frames (8960 samples) cho mỗi bước xử lý tiếp theo để khớp với `chunk_samples` và `pre_encode_cache_size`.
- Áp dụng bộ lọc preemphasis một cách liên tục trên luồng audio đầu vào để tránh đứt gãy tín hiệu.

## Acceptance Criteria

### Chạy thử nghiệm và kiểm tra lỗi
- [ ] LIVA gateway khởi động thành công và tải được 3 session ONNX (Encoder, Decoder, Joint).
- [ ] Khi nói qua microphone trên trang test-voice.html, tín hiệu được truyền qua websocket, xử lý qua VAD và chuyển tới NemotronSTT mà không gây ra bất kỳ lỗi tensor shape hay crash luồng worker nào.
- [ ] Trả về kết quả nhận dạng partial và final hiển thị chính xác trên giao diện trang test-voice.

## Follow-up — 2026-06-18T19:39:31+07:00

Review, clean up, and optimize the backend of the LIVA project (`liva-gateway`, `CoreKernel`, `AI/Voice Workers`). Safely remove outdated or redundant code, and implement a robust automated test suite using Jest to ensure maximum stability and reliability.

Working directory: e:/Project/LIVA/liva-gateway
Integrity mode: benchmark

## Requirements

### R1. Backend Codebase Cleanup
Identify and safely remove dead code, deprecated functions, and unused dependencies within the `liva-gateway` backend (focusing on `CoreKernel` and AI/Voice Workers). Ensure that core functionality remains intact.

### R2. Component Optimization
Refactor inefficient code paths in the Voice/AI Workers and `CoreKernel`. Improve resource utilization (e.g., CPU/GPU allocation) without breaking existing real-time streaming capabilities.

### R3. Automated Test Suite
Set up a testing framework (e.g., Jest) if not already present. Write comprehensive automated unit and integration tests for core backend pathways, covering message parsing, state management (Wake Word / Voice Modes), and worker orchestration.

## Acceptance Criteria

### Testing & Quality
- [ ] A testing framework (Jest) is successfully configured in `package.json`.
- [ ] Running `npm run test` (or equivalent test script) executes successfully without errors.
- [ ] At least 5 critical core components (`CoreKernel`, `VoiceOrchestrator`, etc.) have passing test suites.
- [ ] No existing functionality (Voice interaction, AI processing) is broken after cleanup.
- [ ] Redundant files or dead code blocks have been documented and safely deleted.

## Follow-up — 2026-06-19T00:30:06Z

# Teamwork Project Prompt

> Status: Ready for launch — awaiting user approval
> Goal: Delegate to the teamwork_preview multi-agent system

Upgrade LIVA towards Singularity by integrating cutting-edge local modules: Florence-2 for vision, FlashRank for semantic reranking, Moonshine for STT, an MCP Host Client, and AST Code Surgeon capabilities for safe self-healing.

Working directory: e:/Project/LIVA
Integrity mode: benchmark

## Requirements

### R1. Zero-Trust Deictic Vision (Florence-2)
Integrate the Florence-2 model via `onnxruntime-node` inside a dedicated Worker Thread. The model must perform local OCR and coordinate detection to replace the Cloud Vision API without blocking the main CPU thread or using GPU VRAM.

### R2. FlashRank Reranking for L2 Semantic Memory
Integrate FlashRank to re-score the top 10 event bricks retrieved by the existing `sqlite-vec` system. Ensure only the top 3 highest-scoring contexts are passed to the `PromptBuilder`.

### R3. Moonshine STT Optimization
Incorporate Moonshine ONNX into the STT streaming pipeline (e.g., `NemotronWorker.ts` or a new worker). Ensure it efficiently processes dynamic audio lengths to eliminate static padding delays.

### R4. Official MCP Servers Integration
Build a standardized MCP Host Client capable of connecting to external MCP servers. Successfully integrate and test the connection with at least 1 official MCP server (e.g., Git) to demonstrate plug-and-play capability.

### R5. Self-Healing AST Code Surgeon
Enhance `GitNexusIndexer` to perform blast-radius analysis before any code modification. Output a unified diff and impact report instead of directly executing atomic writes.

## Acceptance Criteria

### R1. Vision Testing
- [ ] Jest tests verify that the Florence-2 worker successfully loads the model and processes a mock image buffer to return coordinates/text.
- [ ] No VRAM allocation occurs during Vision inference.

### R2. Reranking Testing
- [ ] A test case provides 10 mock text chunks to FlashRank, and it successfully returns the top 3 sorted by relevance score.

### R3. STT Testing
- [ ] Jest integration tests confirm the Moonshine worker can transcribe an incoming audio buffer chunk without latency spikes caused by padding.

### R4. MCP Client Testing
- [ ] The MCP Host Client can successfully establish a connection to a test MCP server (e.g., Git) and execute a basic tool/command without internal API wrappers.

### R5. AST Surgeon Testing
- [ ] A test script triggers the AST Surgeon on a dummy file, and it successfully generates a blast-radius report and unified diff without mutating the target file.

## Follow-up — 2026-06-21T07:00:32Z

# Teamwork Project Prompt

> Status: Launched

Upgrade LIVA by creating a suite of new, standard AI assistant skills. The team should independently decide which skills to implement based on typical assistant capabilities (e.g., weather, time, basic web search, calculator). 

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Implement Standard Skills
Investigate the `liva-gateway` existing skill structure (`src/skills/`) and implement a suite of new standard assistant skills. Do not over-constrain the exact skills; use your judgment to pick the most useful ones.

### R2. Use Local Gemini Proxy
Any reasoning or LLM generation must utilize the newly rewritten `GeminiAPI.ts` wrapper (which connects to the local `gemini-web2api` proxy at `http://localhost:8081/v1`). Do not use multimodal/image inputs.

### R3. Write Unit Tests
For every new skill created, you must write a corresponding Jest integration test in the `liva-gateway/tests` directory.

## Acceptance Criteria

### Verification
- [ ] All newly created skills compile successfully without TypeScript errors.
- [ ] The `GeminiAPI.ts` wrapper is successfully used in at least one skill for text generation or structured output.
- [ ] `npm run test` passes successfully, including the newly written Jest tests for the new skills.
- [ ] No image or multimodal API calls are made.

## Follow-up — 2026-06-21T14:28:32Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Research and implement LIVA's dynamic, evolving personality based on user interactions.

Research and upgrade LIVA's memory architecture to support a dynamic, evolving personality. The personality must shift naturally based on how the user interacts over time (e.g., friendly and helpful if nurtured, or adopting different behaviors if treated poorly), reducing reliance on static prompts.

Working directory: `e:\Project\LIVA`
Integrity mode: development

## Requirements

### R1. Autonomous Architecture Research & Implementation
The team must research state-of-the-art methods for evolving AI persona memory (using open-source references or research literature) and seamlessly integrate the optimal architectural approach into LIVA's memory subsystem (`liva-gateway/src/memory/`).

### R2. Evolving Personality Dynamics
Implement the core logic that evaluates and updates LIVA's underlying personality state or persona context based on the sentiment and tone of the user's historical interactions. The system should naturally mimic a learning entity ("raising a child") without hard-coded static prompts.

## Acceptance Criteria

### Architecture & Implementation
- [ ] The team provides a brief Markdown report summarizing the chosen architectural approach, open-source references used, and why it is the best fit for LIVA.

### Objective Verification
- [ ] A programmatic simulation test script is provided (e.g., `tests/personality_divergence_simulation.ts`).
- [ ] The simulation script successfully feeds two distinct simulated user histories (one exclusively friendly/nurturing, one exclusively toxic/abusive) into the newly upgraded memory system.
- [ ] The simulation script automatically asserts and verifies that LIVA's resulting persona/tone significantly diverges between the two simulated histories.

### Quality Guardrails
- [ ] The `liva-gateway` project passes all TypeScript compilation (`npm run typecheck`) without errors.

## Follow-up — 2026-06-21T14:58:07Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Research, compare, and overhaul LIVA's core memory architecture for maximum performance.

Research, compare, and completely overhaul LIVA's core memory architecture to find the absolute best, most optimal solution for scalability, speed, and maintainability, substituting existing technologies if proven superior.

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Extensive Database & Architecture Research
The team must research and benchmark state-of-the-art memory/database architectures (e.g., PostgreSQL, Redis, Qdrant, vs heavily tuned SQLite) to find the absolute best replacement or upgrade for LIVA's current system. The new architecture must drastically improve read/write speed, high concurrency handling, and code maintainability.

### R2. Flawless Integration & Implementation
Replace or upgrade LIVA's existing memory layer (`liva-gateway/src/memory/`) with the selected optimal architecture. Ensure the system runs smoothly without any latency spikes, deadlocks, or crashes during heavy multi-agent interactions.

## Acceptance Criteria

### Architecture & Implementation
- [ ] A comprehensive Markdown report is generated detailing the benchmarking comparison between the old and new memory architectures, justifying the final technology choices.

### Objective Verification
- [ ] A programmatic Load/Stress Testing script (e.g., k6, artillery, or a custom stress tester) is provided in `tests/memory_stress_benchmark.ts`.
- [ ] The stress test asserts and objectively verifies that the new architecture sustains a significantly higher read/write throughput and concurrency load than the baseline without errors.

### Quality Guardrails
- [ ] The `liva-gateway` project passes all TypeScript compilation (`npm run typecheck`) without errors.
- [ ] All existing regression and unit tests (`npm run test`) pass smoothly, ensuring the architectural migration did not break core AI workflows.


## Follow-up — 2026-06-21T23:27:26+07:00

# Teamwork Project Prompt

> Status: Launched
> Goal: Conduct comprehensive audit and generate a strategic development plan.

Conduct a comprehensive, data-driven codebase audit across the entire LIVA project (Backend, Frontend, App) to identify bottlenecks, technical debt, and areas for upgrades, and generate a strategic development plan.

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Full-Stack Data-Driven Audit
The team must scan and analyze the entire LIVA repository, including the AI Gateway (Backend) and the client apps (Frontend/Mobile). The audit must not be based purely on guessing; the team must run programmatic profiling, linting, or static analysis tools to identify actual performance bottlenecks, security flaws, or outdated architectures.

### R2. Strategic Development Plan Generation
Based on the hard data gathered in R1, generate a comprehensive Markdown report titled `LIVA_Strategic_Upgrade_Plan.md`. The plan must propose concrete, prioritized next steps for upgrading, optimizing, or developing new features for LIVA.

## Acceptance Criteria

### Objective Verification
- [ ] A script or log output (e.g., `tests/audit_profiler.ts` or similar scanner scripts) must be created and executed, outputting raw metrics, warnings, or bottleneck reports to a file.
- [ ] The final `LIVA_Strategic_Upgrade_Plan.md` MUST cite the raw metrics or warnings generated by the scanner script to justify every proposed upgrade or optimization.

### Quality Guardrails
- [ ] The `LIVA_Strategic_Upgrade_Plan.md` is successfully generated and placed in the project root.
- [ ] No destructive actions (e.g., deleting core source code files blindly) are taken during the audit process.

## Follow-up — 2026-06-21T21:21:04Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Execute "Phase C" of the LIVA Strategic Upgrade Plan.

Execute "Phase C" of the LIVA Strategic Upgrade Plan: Decouple God Components. The primary goal is to break down three massive files into modular, maintainable subsystems while maintaining perfect functionality.

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Refactoring CoreKernel.ts
Deconstruct `liva-gateway/src/core/CoreKernel.ts` (currently 2,458 lines) into smaller, focused modules.
- Separate Lifecycle Management, Dependency Injection, and Signal Trapping into distinct services.
- Move plugin/skill loading logic to dedicated orchestrators.

### R2. Refactoring AgentLoop.ts
Deconstruct `liva-gateway/src/core/AgentLoop.ts` (currently 1,813 lines) by extracting:
- The Prompt Compilation pipeline.
- The Execution Engine (tool dispatch, response parsing).
- State tracking and loop halting mechanisms.

### R3. Refactoring StructuredMemory.ts
Deconstruct `liva-gateway/src/memory/StructuredMemory.ts` (currently 1,292 lines) by splitting:
- Read/Write logic (I/O).
- Semantic Search and Embedding interactions.
- Memory summarization and consolidation algorithms.

*IMPORTANT:* You must preserve all existing public interfaces exported by these files so that external dependents (like `liva-app` and tests) do not break. Instead of breaking external imports, re-export the newly decoupled modules from the original files or an `index.ts`.

## Acceptance Criteria

### Objective Verification
- [ ] The files `CoreKernel.ts`, `AgentLoop.ts`, and `StructuredMemory.ts` are each reduced to less than 500 lines.
- [ ] Running `npm run typecheck` outputs exactly 0 errors.
- [ ] Running `npx eslint liva-gateway/src` outputs exactly 0 warnings/errors.
- [ ] All 2,618 existing tests pass perfectly without any skips (`npm run test`).

## Follow-up — 2026-06-21T23:23:06Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Conduct a comprehensive full-stack scan of the LIVA project to identify new areas for upgrade and optimization, following the completion of Phases A, B, and C. Propose the next strategic development phases (Phase D, Phase E).

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Full-Stack Strategic Scan
Scan both the Frontend and Backend components of the LIVA project. Use static analysis, profilers, shell scripts, and code reading tools to identify the largest remaining bottlenecks, technical debt, or areas lacking modern best practices.

### R2. Pure Research (No Code Modification)
The team must ONLY read, profile, and analyze. They must NOT modify any source code files or fix any bugs during this phase.

### R3. Strategic Roadmap Generation
Generate a detailed markdown report proposing the next logical upgrade phases (e.g., Phase D, Phase E) with concrete reasoning and expected impact based on the evidence gathered.

## Acceptance Criteria

### Objective Verification
- [ ] The file `LIVA_Next_Upgrades_Plan.md` is generated and saved in the root directory (`e:\Project\LIVA`).
- [ ] The report explicitly defines a "Phase D" and a "Phase E" upgrade proposal.
- [ ] The report cites at least 3 concrete data points (e.g., specific file paths, line numbers, performance metrics, or linter outputs) as evidence to justify its proposals.
- [ ] Running `git status` or checking the file system verifies that no original source code files (e.g., `.ts`, `.tsx`) have been modified by the team.

## Follow-up — 2026-06-21T23:31:50Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Execute "Phase D" of the LIVA Strategic Upgrade Plan: Core Reliability, Database Optimization, and System Performance. Address database indexing, write-amplification, memory leaks, and IPC/AST configuration errors based on the strategic scan report.

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Database Optimization & Indexing
- In `liva-gateway/src/memory/VectorRepository.ts`, add SQLite indexes for `vectors_meta(type, domain, category)` and `vectors_meta(created_at)`.
- Introduce batch transaction capabilities in `DatabaseWorkerBridge`.
- Refactor the sequential database update loops in `EventRepository.ts` and `ConsolidationSteps.ts` to use these atomic batch transactions, preventing write-amplification.

### R2. Frontend Memory Leaks & Tauri IPC Fixes
- Ensure event listeners (`visibilitychange`, `click`, `keydown`) are properly removed inside the `dispose()` and `stopPipeline()` functions of `liva-ui/src/composables/useVRM.ts` and `useVoicePipeline.ts`.
- In `liva-desktop/src-tauri/src/lib.rs`, implement and register the missing `read_vault_key` and `write_vault_key` IPC handlers.

### R3. AST Paths Resolution
- In `liva-gateway/src/workers/ASTWorker.ts`, initialize the ts-morph `Project` to explicitly load the repository's `tsconfig.json` so that path aliases resolve correctly.

## Acceptance Criteria

### Objective Verification
- [ ] Running `npm run typecheck` in the Gateway and `npx vue-tsc --noEmit` in the UI outputs exactly 0 errors.
- [ ] Running `npx eslint` across the modified Gateway and UI files outputs exactly 0 warnings/errors.
- [ ] All existing 2,618 tests (`npm run test -w liva-gateway`) continue to pass successfully.
- [ ] A programmatic database test confirms that vector metadata queries execute using indexes (no table scans) and that batch inserts successfully commit as atomic transactions.

## Follow-up — 2026-06-22T00:17:25Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

Execute "Phase E" of the LIVA Strategic Upgrade Plan: Advanced Local AI Capabilities, Client Security, and Offline Isolation. Focus on Voice/AI engines, WASM pruning, Localizing CDNs, and Tauri Hardening based on the strategic scan report.

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Real-time Voice Engine & STT Upgrades
- In `liva-ai-engine/voice_engine.py`, rewrite the Edge-TTS stream handler to yield audio chunks immediately via WebSocket (Real-time Chunk Streaming) to eliminate Time-To-First-Sound latency.
- In `liva-ai-engine/whisper_stt_server.py`, validate the WAV sample rate and automatically resample 44.1kHz/48kHz input streams to 16kHz before passing to the Whisper model.

### R2. Offline Isolation & WASM Clean-up
- Remove all unused `ort-wasm-*.wasm` binaries (~77.5MB) from `liva-ui/public/` and uninstall `onnxruntime-web` from `liva-ui/package.json`.
- Localize all Fastly/jsDelivr CDN assets (`live2d.min.js`, Mediapipe tasks) in `liva-ui/widget.html` and `useFaceTracking.ts` by downloading them into `liva-ui/public/assets/` to ensure LIVA operates 100% offline.

### R3. Advanced AI Workers & Security Hardening
- Implement proper Tokenizer encoding in `liva-gateway/src/workers/FlashRankWorker.ts` and decoding in `MoonshineWorker.ts` to replace the dummy bypass mocks.
- Harden Tauri security in `liva-desktop/src-tauri`: Define a strict `csp` in `tauri.conf.json` and switch the Stronghold key derivation in `lib.rs` to a secure KDF like Argon2id. Optimize the cursor polling thread to an event-driven mechanism to save CPU overhead.

## Acceptance Criteria

### Objective Verification
- [ ] Running `npm run typecheck` in the Gateway and `npx vue-tsc --noEmit` in the UI outputs exactly 0 errors.
- [ ] All existing 2,630 tests (`npm run test -w liva-gateway`) continue to pass successfully.
- [ ] A verification script asserts that the `ort-wasm-*.wasm` files no longer exist and the `liva-ui` build size is significantly reduced.
- [ ] A manual or simulated verification confirms that the STT server correctly resamples 48kHz audio and that Tauri compiles without errors (`cargo check`).


## Follow-up — 2026-06-22T00:43:38Z

# Teamwork Project Prompt

> Status: Launched
> Goal: Comprehensive Testing & Debugging (Phase F)

Execute "Phase F" of the LIVA project: Comprehensive Testing & Auto-Debugging. The user has explicitly granted permission to execute this without asking for approval.

Working directory: `e:\Project\LIVA`
Integrity mode: benchmark

## Requirements

### R1. Coverage Analysis
- Run test coverage tools (e.g., `jest --coverage` for TS, `pytest --cov` for Python) across the entire LIVA system (Frontend, Backend Gateway, and AI Engines).
- Identify modules, components, or functions that lack test coverage.

### R2. Auto-Test Generation
- Automatically write and integrate missing Unit Tests and Integration Tests for the uncovered areas identified in R1.
- Ensure the newly added tests mock dependencies appropriately to maintain fast execution times.

### R3. Auto-Debugging
- If any existing tests fail, or if new tests reveal hidden bugs, automatically investigate the root cause.
- Apply code fixes to resolve the bugs immediately.

## Acceptance Criteria

### Objective Verification
- [ ] A final report `LIVA_Phase_F_Testing_Report.md` is generated detailing the initial coverage vs. final coverage, and documenting all bugs found and fixed.
- [ ] Running all tests across the system (`npm run test -w liva-gateway`, `pytest`, etc.) results in 100% passes.
- [ ] Zero Type errors or ESLint warnings remain after the code fixes.

---
*Execute immediately.*



