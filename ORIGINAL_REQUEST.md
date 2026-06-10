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
