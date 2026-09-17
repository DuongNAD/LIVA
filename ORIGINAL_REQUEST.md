# Original User Request

## Initial Request — 2026-08-14T04:53:11Z

Comprehensive audit of LIVA's native 4-tier memory architecture (L0 RAM Context, L1 Structured SQLite, L2 Vector sqlite-vec / H-MEM, L3 Obsidian Knowledge Graph) in `liva-native-core` and delivery of an exhaustive Audit Report alongside an actionable Technical Architecture & Optimization Blueprint.

Working directory: E:\Project\LIVA
Integrity mode: development

## Requirements

### R1. 4-Tier Memory Architecture Deep Audit
Analyze current Rust implementations in `liva-native-core` and architecture documentation:
- **L0 (RAM Context)**: Working context window management, sliding window / token budget allocation, caching mechanisms.
- **L1 (StructuredMemory SQLite)**: WAL mode connection pooling, event bricks (Φ Factual + Ψ Relational), `turn_layer_nodes`, transaction safety.
- **L2 (VectorMemory sqlite-vec & H-MEM)**: Vector indexing, embedding cache, positional index drill-down to L1 source events, hybrid search.
- **L3 (Knowledge Graph & Long-Term Memory)**: Obsidian vault synchronization, entity-relationship extraction, semantic link persistence.
- **Daemons & Pipelines**: `ReflectionDaemon`, `ConsolidationCron`, Dead Letter Queue (DLQ), and fact conflict resolution.

### R2. Bottleneck, Concurrency & Risk Matrix
Identify and document architectural and runtime risks with exact file/line citations:
- Lock contention & WAL serialization under high-concurrency / multi-agent access.
- Latency curves and scaling boundaries for vector KNN and hybrid FTS search.
- Memory drift, hallucinated fact persistence, and lack of temporal decay (Ebbinghaus curve).
- Data privacy, secure zeroization, dry-run safety, and GDPR/Decree 13 compliance audit trails.

### R3. Comprehensive Technical Optimization Blueprint
Design an actionable modernization plan referencing state-of-the-art memory patterns (Mem0, MemGPT/Letta, GraphRAG):
- Optimized Rust data structures (e.g., zero-copy deserialization, concurrent LRU caching via `moka`/`DashMap`).
- Dynamic memory decay, adaptive reflection triggers, and hierarchical summarization pipelines.
- Upgraded schema migrations and indexing strategies maintaining full backward compatibility with existing SQLite DBs and Tauri IPC.

## Acceptance Criteria

### Comprehensive Codebase Evidence
- [ ] Every finding and bottleneck explicitly references concrete structs, functions, or schema definitions within `liva-native-core` or documentation.
- [ ] Complete coverage across all 4 tiers (L0, L1, L2, L3) and both background daemons (`ReflectionDaemon`, `ConsolidationCron`).

### Structured Risk & Performance Matrix
- [ ] A formal risk matrix ranking issues by severity, reproduction scenarios, latency impact, and concurrency limits.

### Technical Blueprint & Roadmap
- [ ] Visual Mermaid architecture diagrams detailing the target memory lifecycle, read/write pipelines, and consolidation triggers.
- [ ] Phase-by-phase execution roadmap with zero breaking changes to existing Tauri IPC contracts.

## 2026-08-16T04:37:22Z

Nghiên cứu toàn diện kiến trúc DeepSeek Harness (deepseek-ai/deepseek-harness), phân tích mô hình vi nhân Cordis (Plugin/Event-Bus architecture) và các hệ thống cốt lõi (core/session, core/tools, core/system-prompt, llm/llm streaming, eval harness) để thiết kế, tối ưu hóa và tích hợp vào hệ sinh thái LIVA Native Core (Rust) và Tauri UI.

Working directory: e:/Project/LIVA
Integrity mode: development
Reference URL: https://github.com/deepseek-ai/deepseek-harness

## Requirements

### R1. DeepSeek Harness Architectural Deconstruction & Obsidian Knowledge Base
- Phân tích chi tiết mã nguồn DeepSeek Harness (Cordis microkernel, spatiotemporal composability, plugin lifecycle, session event append-only logs).
- Tạo tài liệu phân tích đối chiếu chuyên sâu 1-1 với LIVA Native Core tại teamwork_projects/obsidian_llm_wiki/vault/Knowledge/deepseek_harness_integration.md.
- Xác định rõ các mẫu thiết kế có thể chuyển giao (transferable patterns) và các điểm cần tránh (anti-patterns).

### R2. Scoped Event Stream & Guarded Tool Pipeline Design for Rust Core
- Thiết kế và triển khai mô hình Native Scoped Tool Registry và Session Event Stream trong liva-native-core.
- Tích hợp nguyên tắc fail-closed, kiểm soát CommandPrincipal, sandbox execution và cho phép đăng ký/gỡ bỏ tool động theo ngữ cảnh agent.
- Đảm bảo tuân thủ tuyệt đối các Invariants của LIVA: không chặn Tokio runtime, SQLite WAL connection pool an toàn, memory safety.

### R3. Streaming CoT / Reasoning Token & Dynamic Prompt Assembly Subsystem
- Nâng cấp pipeline streaming LLM trong liva-native-core để bóc tách token suy luận (<think> / CoT blocks) và token kết quả thực thi theo thời gian thực.
- Xây dựng hệ thống lắp ráp prompt động (Dynamic Prompt Assembly) mô-đun hóa dựa trên plugins/skills và ngân sách context window (token budget control).
- Truyền dữ liệu luồng CoT mượt mà sang Tauri IPC và Vue 3 UI mà không gây giật lag DOM.

### R4. Automated Evaluation & Benchmark Harness (LIVA-Eval)
- Xây dựng framework benchmark tự động cho LIVA (kế thừa triết lý từ Harness Benchmark runner) để đo lường:
  - Tốc độ phản hồi (Time to First Token - TTFT, Tokens Per Second - TPS).
  - Độ chính xác thực thi công cụ (Tool Call Accuracy & Argument Validation).
  - Khả năng xử lý tác vụ multi-step reasoning trên cả Local GGUF models và Cloud APIs.

## Acceptance Criteria

### Documentation & Architectural Artifacts
- [ ] Báo cáo phân tích chuyên sâu được tích hợp hoàn chỉnh vào Obsidian Vault với đầy đủ liên kết 2 chiều ([[liva_architecture]], [[threat-model]]).
- [ ] Bản thiết kế chi tiết (RFC & API specs) cho Scoped Tool Registry và CoT Streaming Seam.

### Rust Core Implementation & Code Quality
- [ ] Mã nguồn Rust trong liva-native-core biên dịch sạch sẽ (cargo check, cargo clippy --all-targets không phát sinh lỗi hoặc cảnh báo nghiêm trọng).
- [ ] Cơ chế Scoped Tool Pipeline có unit tests và integration tests đạt tỷ lệ vượt 100%.
- [ ] Cơ chế CoT stream parser xử lý chính xác các trường hợp biên (streaming dở dang, nested tags, tool calls xen kẽ).

### Security & Invariant Compliance
- [ ] Tuân thủ chặt chẽ mô hình phân quyền CommandPrincipal và Tauri CSP fail-closed.
- [ ] Tuyệt đối không chạy inference blocking hoặc I/O nặng trên Tokio control loops.
- [ ] Trước khi sửa đổi bất kỳ symbol hiện có nào, phải chạy phân tích tác động upstream với GitNexus.

## 2026-08-16T14:19:09+07:00

Nghiên cứu nâng cấp toàn diện, tối ưu hóa hiệu năng và hoàn thiện hệ sinh thái trợ lý thông minh LIVA (Rust Native Core, Tauri Desktop UI, AI Agent Swarm & RAG, Security & Quality).

Working directory: E:\Project\LIVA
Integrity mode: development

## Requirements

### R1. Native Core Optimization & Backend Performance
Nâng cấp và tối ưu hóa toàn diện `liva-native-core` (Rust), bao gồm cơ chế quản lý bộ nhớ, tối ưu hóa WAL SQLite connection pool, giảm thiểu độ trễ IPC giữa Tauri frontend và native backend, cũng như tối ưu hóa tốc độ xử lý dữ liệu và streaming response.

### R2. Desktop UI & Realtime User Experience
Hoàn thiện giao diện `liva-desktop` / `liva-ui`, tối ưu hóa trải nghiệm người dùng (UX), trạng thái tương tác mượt mà, xử lý đa luồng realtime, tích hợp trực quan hóa Dashboard BI, quản lý tri thức PKM/Obsidian mượt mà, và quản lý profile/cài đặt nhất quán.

### R3. AI Router, Multi-Agent Swarm & Vector RAG Enhancements
Nâng cấp năng lực định tuyến AI Router, cơ chế tìm kiếm lai (Hybrid Semantic & FTS Search), tối ưu hóa workflow phân rã tác vụ (DAG orchestration), quản lý bộ nhớ ngữ cảnh dài hạn và tăng cường độ tin cậy của các subagent tools.

### R4. Comprehensive Security, Compliance & Code Quality
Thực hiện kiểm toán toàn diện mã nguồn theo chuẩn bảo mật cao cấp (PDG taint analysis, PII Sanitizer), xử lý triệt để tech-debt còn tồn đọng trong `tech-debt-ledger.json`, loại bỏ code chết, và bổ sung test coverage cho các đường dẫn nghiệp vụ quan trọng.

## Acceptance Criteria

### Core Performance & Architecture
- [ ] Mọi crate trong workspace Rust biên dịch thành công mà không có lỗi (`cargo check --all-targets` và `cargo test` pass 100%).
- [ ] Không có cảnh báo nghiêm trọng từ `cargo clippy --workspace -- -D warnings`.
- [ ] Thời gian khởi động và phản hồi IPC giữa Rust backend và Frontend duy trì dưới 100ms cho các truy vấn dữ liệu cục bộ tiêu chuẩn.

### Desktop & UI Quality
- [ ] Frontend build thành công không lỗi cú pháp hay lint (`npm run build` / `npm run check` pass).
- [ ] Giao diện hoạt động trơn tru, không có hiện tượng giật lag khung hình khi stream phản hồi từ AI hoặc xử lý tập dữ liệu lớn.
- [ ] Tất cả các luồng tương tác chính (Chat, PKM/Notes, Analytics, Settings, Workflows) phản hồi đúng trạng thái lỗi và thành công (Error boundary, loading skeletons, toast alerts).

### AI & Agent Capabilities
- [ ] Các kịch bản định tuyến AI (Local models vs Cloud APIs, RAG semantic search, Subagent delegation) có test cases kiểm thử tự động xác nhận tính chính xác và khả năng tự phục hồi khi gặp lỗi kết nối.
- [ ] Quá trình trích xuất thông tin và truy xuất tài liệu qua Hybrid Vector-FTS đạt độ chính xác cao, không để lộ thông tin nhạy cảm (PII).

### Security & Maintainability
- [ ] Không có lỗ hổng bảo mật nghiêm trọng (critical/high severity) trong audit dependencies (`cargo audit` và `npm audit`).
- [ ] Tech-debt trong `tech-debt-ledger.json` được rà soát và cập nhật/giải quyết đối với các hạng mục đã hoàn thiện.
- [ ] Toàn bộ tài liệu kiến trúc hệ thống và hướng dẫn vận hành phản ánh chính xác cấu trúc sau khi nâng cấp.

## 2026-08-17T00:17:05+07:00

Nghiên cứu nâng cấp toàn diện, tối ưu hóa hiệu năng, xây dựng bộ kiểm thử thực chất và hoàn thiện hệ sinh thái trợ lý thông minh LIVA (Rust Native Core, Tauri Desktop UI, AI Agent Swarm & RAG, Security & Quality).

Working directory: E:\Project\LIVA
Integrity mode: development

## Requirements

### R1. Real End-to-End & Subsystem Verification
Xây dựng và hoàn thiện bộ kiểm thử E2E thực chất (Real E2E Test Suite), loại bỏ các mock/reimplementation thuật toán bằng JavaScript để kiểm thử trực tiếp trên Rust native binary (`liva-native-core`), kết nối IPC socket/Tauri bridge thực, database SQLite WAL trên đĩa và các luồng UI.

### R2. Native Core Optimization & Backend Performance
Nâng cấp và tối ưu hóa toàn diện `liva-native-core` (Rust), bao gồm cơ chế quản lý bộ nhớ, tối ưu hóa WAL SQLite connection pool (1-writer / 4-readers), giảm thiểu độ trễ IPC giữa Tauri frontend và native backend (<5ms), tối ưu hóa streaming response và ngăn ngừa lock contention.

### R3. Desktop UI & Realtime User Experience
Hoàn thiện giao diện `liva-desktop` / `liva-ui` (Vue 3.5 + TypeScript + Vite + Tauri v2), tối ưu hóa trải nghiệm người dùng (UX), render streaming mượt mà, xử lý đa luồng realtime, hoàn thiện Dashboard BI Analytics, Obsidian PKM Vault Explorer, và hệ thống Toast / Skeleton loaders nhất quán.

### R4. AI Router, Multi-Agent Swarm & Vector RAG Enhancements
Nâng cấp năng lực định tuyến AI Router (dynamic model template detection, token bounds, KV cache pruner), cơ chế tìm kiếm lai (Hybrid Semantic ONNX 384-dim & FTS5 unicode61 với RRF $K=60.0$), tối ưu hóa workflow phân rã tác vụ (Swarm DAG StateGraph orchestration), và cơ chế tự phục hồi lỗi.

### R5. Comprehensive Security, Compliance & Code Quality
Thực hiện kiểm toán toàn diện mã nguồn theo chuẩn bảo mật cao cấp (PDG taint analysis, PII Sanitizer), mã hóa dữ liệu nhạy cảm qua Windows DPAPI & AES-256-GCM, bảo đảm tuân thủ Nghị định 13 / GDPR với `PRAGMA secure_delete = ON`, xử lý triệt để tech-debt trong `tech-debt-ledger.json` và loại bỏ hoàn toàn code chết.

## Acceptance Criteria

### Core Architecture & Build Integrity
- [ ] Mọi crate trong workspace Rust biên dịch thành công và vượt qua 100% unit/integration tests (`cargo check --all-targets` và `cargo test -p liva-native-core --lib`).
- [ ] Không có bất kỳ cảnh báo nào từ `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Thời gian phản hồi IPC giữa Rust backend và Frontend duy trì ổn định dưới 10ms cho các truy vấn dữ liệu cục bộ.

### Real E2E Testing & Verification
- [ ] Bộ kiểm thử E2E thực thi trực tiếp trên socket/binary thật (`npm run test:e2e` / `e2e-gateway-ci.mjs` / `e2e-memory.mjs`) vượt qua 100% các ca kiểm thử mà không dựa vào reimplementation thuật toán bằng JS trong test helper.
- [ ] Xác nhận tính toàn vẹn của kết nối SQLite WAL bifurcated pool dưới tải đồng thời (concurrent load không bị `SQLITE_BUSY`).

### Desktop UI & UX Quality
- [ ] Frontend build thành công sạch sẽ không lỗi cú pháp hay lint (`npm run build -w liva-ui`, `npm run build -w liva-desktop`, `npx eslint liva-ui/src`).
- [ ] Bộ kiểm thử Vitest trên UI đạt 100% pass (`npm run test -w liva-ui`).
- [ ] Các màn hình chính (Chat Stream, BI Analytics, Obsidian Vault Explorer, Settings) hiển thị trạng thái mượt mà, có đầy đủ loading skeleton và error toast khi có sự cố.

### AI & Agent Capabilities
- [ ] AI Router và Hybrid Search RRF kết hợp vector ONNX và FTS5 hoạt động chính xác trên các truy vấn đa ngôn ngữ (bao gồm tiếng Việt có dấu).
- [ ] Swarm DAG StateGraph điều phối các tác nhân chuyên biệt chính xác theo DAG flow và cơ chế self-correction.

### Security, Compliance & Code Governance
- [ ] Dữ liệu hội thoại nhạy cảm được mã hóa an toàn qua AES-256-GCM / DPAPI và được xóa vĩnh viễn khi yêu cầu Right-to-be-Forgotten.
- [ ] SecretScrubber lọc sạch các secrets/PII nhạy cảm trong luồng nhật ký và prompt payloads.
- [ ] Báo cáo kiểm toán kỹ thuật `tech-debt-ledger.json` đạt điểm 100/100 (0 god components, 0 violations).

## Follow-up — 2026-08-17T00:22:40+07:00

Nghiên cứu toàn diện, tối ưu hoá hiệu năng và hoàn thiện hệ thống LIVA trên toàn bộ các tầng: Unified Native Core (Rust), 5 nhóm Agent Skills chuyên sâu thế hệ mới, và giao diện Desktop Tauri an toàn, mượt mà.

Working directory: E:\Project\LIVA
Integrity mode: development

### Requirements

#### R1. Native Core Optimization & Architecture Integrity
Hệ thống lõi `liva-native-core` phải đạt hiệu năng xử lý cao, quản lý bộ nhớ an toàn (zero memory leaks, safe concurrency), tối ưu hoá pool kết nối database WAL, giảm thiểu tối đa độ trễ cho pipeline xử lý AI router, semantic search và IPC communication. Toàn bộ logic backend tuân thủ triệt để kiến trúc thuần Rust, không hồi quy về legacy runtime.

#### R2. Advanced Agent Skills Ecosystem
Mở rộng kho kỹ năng của LIVA với 5 bộ Skill chuyên sâu mới, tuân thủ đúng chuẩn skill governance:
1. **System Automation & Safe OS Control**: Tự động hóa tác vụ hệ điều hành, quản lý tiến trình, file system an toàn với hàng rào bảo mật.
2. **Deep Research & Autonomous Web Agent**: Thu thập, xử lý, trích xuất và tổng hợp thông tin đa nguồn tự động.
3. **Code Assistant & Automated Refactoring**: Phân tích đồ thị mã nguồn (Code Graph / PDG), tự động đề xuất và tái cấu trúc code an toàn.
4. **Multimodal & Realtime Audio/Vision**: Tích hợp pipeline xử lý âm thanh/giọng nói và nhận diện hình ảnh/màn hình thời gian thực.
5. **Agentic Workflow Swarm**: Điều phối đa tác tử theo đồ thị DAG, cơ chế bỏ phiếu đồng thuận (voting consensus) và điểm dừng Human-in-the-Loop (HITL).

#### R3. Desktop UI/UX & Native IPC Interface
Hoàn thiện giao diện Tauri Desktop với khả năng phản hồi mượt mà (<16ms frame render), hiển thị trực quan trạng thái của các Agent/Skills, tích hợp mượt mà với Native Core qua kênh IPC/WebSocket bảo mật, quản lý luồng streaming dữ liệu và voice assistant ổn định.

#### R4. Security & Quality Governance
Mọi thay đổi và module mới phải vượt qua các rào chắn kiểm tra bảo mật, không phát sinh memory leak, không vi phạm quy tắc Git safety và GitNexus PDG taint flow analysis.

### Acceptance Criteria

#### Core Engine & Build Quality
- [ ] Toàn bộ workspace Rust biên dịch thành công (`cargo check --workspace`, `cargo clippy --workspace -- -D warnings`) không có lỗi hoặc cảnh báo nghiêm trọng.
- [ ] Bộ kiểm thử đơn vị và tích hợp (`cargo test --workspace`) đạt 100% pass rate.
- [ ] Database connection pool và WAL journaling hoạt động ổn định dưới tải concurrency mô phỏng mà không bị deadlock.

#### Skills Implementation & Governance
- [ ] Cả 5 skill mới được cấu trúc đầy đủ chuẩn định dạng trong `.agents/skills/` (gồm `SKILL.md` chuẩn YAML frontmatter, tài liệu hướng dẫn, scripts/schemas rõ ràng).
- [ ] Kiểm thử tự động `liva-skill-governance` xác nhận tất cả skills hợp lệ, không có cú pháp lỗi thời hoặc đường dẫn ảo.
- [ ] Mỗi skill đều có kịch bản test case đầu vào / đầu ra mẫu có thể kiểm chứng độc lập.

#### Desktop & IPC Integration
- [ ] Ứng dụng Desktop / UI build thành công (`npm run build` / `npm run lint`) không có lỗi biên dịch TypeScript/ESLint.
- [ ] Kênh IPC giao tiếp giữa UI và Native Core gửi nhận thông điệp, streaming response và state synchronization trơn tru, xử lý lỗi ngoại lệ an toàn.

## Follow-up — 2026-08-18T04:43:53Z

# Teamwork Project Prompt

> Status: Launched
> Requested team: Full team (multi-phase implementation with gate probe)

Thi hành spec Complexity Router cho LIVA (Rust core + Tauri) theo đúng thứ tự giai đoạn, bắt đầu bằng probe đo lường ở Giai đoạn 0 (cổng chặn cứng).

Working directory: E:\Project\LIVA
Integrity mode: development

## Requirements

### R0. Cổng chặn Giai đoạn 0 — Probe đo lường phần cứng và khả thi
Tạo binary `liva-native-core/src/bin/router_gate_probe.rs` (khai báo trong `Cargo.toml`) đo thực tế trên máy hiện tại:
- **Khối A (Embedding):** Đọc `model.n_embd()` của Router và Expert model, so sánh với chiều embedding của DB (`src/db.rs:1338`). Phán quyết `AN TOÀN` hoặc `PHÁ RAG`.
- **Khối B (Swap cost):** Đo thời gian `swap_model()` trung vị qua 3 lượt mỗi chiều (router ↔ expert) bằng `std::time::Instant`, in dung lượng file GGUF và thời gian trừ 500ms sleep.
- **Khối C (Vision sau swap):** Kiểm tra tương thích `mmproj` sau swap trên ảnh test 512×512 (hình chữ nhật đỏ góc trên trái) qua `answer_with_image`. Phán quyết `TƯƠNG THÍCH`, `LỖI RÕ RÀNG`, hoặc `HỎNG THẦM LẶNG`.
- **Điều kiện dừng:** Nếu Khối A `PHÁ RAG` hoặc Khối C `HỎNG THẦM LẶNG` hoặc thiếu file expert GGUF → DỪNG TOÀN BỘ, không triển khai các giai đoạn tiếp theo.

### R1. Giai đoạn 1 — Nối cấu hình Expert model
Thêm hàm `configured_expert_model_path() -> Option<std::path::PathBuf>` trong `src/paths.rs` và export tại `src/lib.rs`:
- Đọc `localModelsDir` và khoá `expertModel`.
- Trả về `None` khi chưa cấu hình hoặc provider khác `local` (không fallback về default path).
- Kèm unit tests trong `paths.rs`.

### R2. Giai đoạn 2 — Bộ phân loại độ khó Heuristic (0 token)
Tạo `src/agent/graph/complexity.rs` với `enum DoKho { Thuong, Kho }` và `fn phan_loai_do_kho(text: &str) -> DoKho`:
- Tái sử dụng `tokenize`, `has_word`, `has_phrase` từ `agent/graph/intent.rs` (không viết lại tokenizer).
- Heuristic thuần túy (độ dài câu, liên từ suy luận đa bước, yêu cầu code, đa câu hỏi), mặc định `Thuong`.
- Đo tỉ lệ phân bố `Thuong`/`Kho` trên corpus thực tế và báo cáo.

### R3. Giai đoạn 3 — Nối Escalation gợi ý Expert (khi Giai đoạn 0 đạt)
Tại 2 call site sản xuất (`src/agent/graph/pipeline.rs:106` và `src/websocket/dialogue.rs:33`):
- Ở nhánh `Intent::Chat`, gọi `phan_loai_do_kho`.
- Đính kèm cờ `goi_y_expert: bool` khi `DoKho::Kho` và expert model tồn tại hợp lệ.
- Không tự động swap làm block mutex chung.

## Acceptance Criteria

### CI & Code Standards
- [ ] `cargo fmt --all -- --check` pass.
- [ ] `cargo clippy --all-targets -- -D warnings` pass (0 warnings).
- [ ] `cargo test -p liva-native-core` pass toàn bộ test suite.
- [ ] Giữ nguyên 100% test hiện có của `route_intent` (~60 asserts).
- [ ] Comment và tên biến viết bằng tiếng Việt theo phong cách code xung quanh.
- [ ] Không tự tiện chạy `git commit` / `git push`.

### Functional Verification
- [ ] `cargo run --bin router_gate_probe` xuất đủ 3 phán quyết A, B, C với số đo thực tế.
- [ ] `paths.rs` unit tests kiểm tra đủ các nhánh: có `expertModel`, thiếu khoá (`None`), `provider != "local"` (`None`).
- [ ] Báo cáo cuối đầy đủ số liệu đo đạc, danh sách file thay đổi và tỉ lệ phân bố độ khó.

## Follow-up — 2026-08-18T05:56:00Z

Comprehensive, deep-dive multi-agent code audit and rigorous verification across all components of the LIVA project to evaluate authenticity, accuracy, system integrity, memory safety, and logical correctness.

Working directory: E:/Project/LIVA
Integrity mode: development

## Requirements

### R1. Granular Rust Native Engine & Concurrency Audit
Audit all crates within `crates/` (core business logic, state machines, async runtime, database connection pooling, SQLite WAL interactions, and vector/FTS search indexing). Identify any subtle race conditions, unhandled `unwrap()` / `expect()` panics, memory leaks, lock contention, or deadlocks in async Tokio / crossbeam contexts.

### R2. Tauri IPC, Data Contracts & Frontend Boundary Audit
Audit the IPC boundary layer between Tauri native bindings and the frontend UI. Verify type-safety, schema synchronization, serialization/deserialization edge cases, error propagation, and state reconciliation to prevent silent dropouts or deserialization panics.

### R3. Security, Guardrails & Architectural Standards Enforcement
Inspect the codebase against LIVA System Agent Guidelines and security best practices: verify Program Dependence Graph (PDG) data flows, taint tracking, sanitization barriers, PII masking, sandbox isolation, and adherence to the single-source-of-truth Rust native architecture (ensuring no lingering legacy runtime dependencies).

### R4. Empirical Verification & Automated Tooling Validation
Execute local automated diagnostics (`cargo check`, `cargo test`, `cargo clippy`, and frontend lint/type checks where applicable) to ground theoretical findings in empirical execution results and compiler diagnostic output.

### R5. Comprehensive Structured Audit & Remediation Report
Produce an exhaustive, evidence-backed Markdown audit report categorized by Severity (Critical, High, Medium, Low) and Domain (Architecture, Security, Correctness, Concurrency, Performance). Each finding must include exact file/line references, root-cause mechanism, blast radius, reproduction scenario, and concrete remediation diffs.

## Acceptance Criteria

### Coverage & Accuracy
- [ ] 100% of core crates and IPC bridge modules have been analyzed without skipping subsystems.
- [ ] All reported vulnerabilities, panics, or logic flaws cite exact file paths and line numbers with verifiable reproduction logic.
- [ ] Compiler diagnostics (`cargo check`, `cargo clippy`, `cargo test`) are recorded and correlated with static findings.
- [ ] No speculative or hallucinated defects: every flagged issue includes clear technical justification and blast radius assessment.
- [ ] Final audit report delivers clear, prioritized, and actionable step-by-step remediation plans.

## 2026-08-18T08:09:24Z

This is a single self-contained fix; keep it small and focused.

Refactor and enhance LIVA 3D avatar locomotion by implementing a unified additive pose blending pipeline, upper-body procedural kinematics (spine counter-rotation, forward lean, head leveling), distance-based stride phase calibration, and rim lighting for transparent desktop contrast.

Working directory: e:/Project/LIVA
Integrity mode: development

## Requirements

### R1. Unified Additive Pose Pipeline & Rest Pose Reset
Establish a deterministic 6-step humanoid pose execution order inside `use3DModel.ts` and `useAvatarAnimation.ts`:
1. Base pose evaluation (`animation.update(vrm, delta)` with 11-bone retargeted clip + base procedural)
2. Rest pose reset for unmapped upper-body nodes (`spine`, `head`, `neck`, `chest`)
3. Locomotion upper-body procedural layer (active when `motionWeight > 0`)
4. Idle procedural layer (breathing sine oscillation + OpenSimplex micro-sway) transitioned from destructive assignment to additive offsets
5. Gaze (`lookAt`), blink, and lip-sync facial blend shapes
6. VRM secondary animation / spring bone physics update (`vrm.update(delta)`)

### R2. Upper-Body Locomotion Procedural Kinematics (Slice A1)
Expose `stridePhase` and `motionWeight` from `useAvatarAnimation.ts` and apply natural upper-body kinematics in `use3DModel.ts` without perturbing root pelvis translation:
- Spine yaw counter-rotation: `-Math.sin(stridePhase) * 0.08 * motionWeight`
- Spine pitch forward lean: `(isRunning ? 0.2 : 0.06) * motionWeight`
- Head pitch stabilization: `-spine.rotation.x * 0.6` (levels gaze horizon during forward lean)
- Head yaw subtle balance: `Math.sin(stridePhase * 0.5) * 0.03 * motionWeight`

### R3. Distance-Based Stride Phase Calibration (Slice A2)
Refactor `stridePhase` progression in `useAvatarAnimation.ts` to advance strictly proportional to travel distance (`currentSpeed * delta / STRIDE_LENGTH`) rather than elapsed wall-clock time (`delta * hz`), calibrating `STRIDE_LENGTH.walk = WALK_SPEED / 1.05` to eliminate foot-slide during acceleration/deceleration transitions.

### R4. Rim Lighting for Transparent Desktop Overlay (Slice A4)
Add a subtle rear-top `DirectionalLight` (rim light) in `use3DModel.ts` pointing toward the camera to visually silhouette and separate the 3D avatar from arbitrary dark, light, or multicolored desktop backgrounds.

## Acceptance Criteria

### Pose Pipeline & Kinematics Correctness
- [ ] Unit test verifies the deterministic 6-step pose pipeline execution order:
  - When `motionWeight = 0`, spine rotation contains only breathing idle offset.
  - When `motionWeight = 1`, spine rotation combines both locomotion forward-lean/counter-rotation and idle breathing additively.
  - Swapping idle before locomotion or overwriting rest pose fails the test.
- [ ] Upper-body locomotion procedurals smoothly scale to 0 when `motionWeight` decays to 0 (no popping or sudden snapping on idle transition).
- [ ] `root.position.y` and horizontal pelvis translation remain unperturbed by upper-body procedurals to prevent artificial IK fight with `FootPlantIK`.

### Stride Phase & Locomotion Kinematics
- [ ] `stridePhase` advancement is governed by `currentSpeed * delta` and freezes when `currentSpeed == 0` even if `state == "walk"`.
- [ ] Nominal stride frequency at steady-state `WALK_SPEED` matches `1.05 Hz` baseline.

### Lighting & Rendering Integrity
- [ ] Rim light is attached to the Three.js scene hierarchy, positioned behind and above the avatar subject, without degrading existing MToon/PBR material shading.

### Quality & Regression Guardrails
- [ ] Full test suite passes: `npm run test:coverage -w liva-ui` maintains 100% threshold compliance with 0 regressions.
- [ ] No frame-rate-dependent lerp multipliers introduced.

## Follow-up — 2026-09-06T06:35:00Z

Đọc, phân tích toàn bộ mã nguồn LIVA trên tất cả các nhánh Git (`main`, `origin/mac-v2`, `test/perf-threshold-baseline`, `feat/openai-api-va-don-dep-avatar`), giữ lại và hoàn thiện các cải tiến trên nhánh hiện tại (`test/perf-threshold-baseline`), tích hợp các tính năng và bản vá tối ưu nhất từ các nhánh khác, sau đó xác thực toàn diện bằng hệ thống kiểm thử tự động để đưa dự án LIVA đạt chất lượng cao nhất.

Working directory: e:\Project\LIVA
Integrity mode: development

## Requirements

### R1. Khảo sát & Phân tích chéo các nhánh Git (Cross-Branch Audit)
- Kiểm tra toàn bộ lịch sử commit, file khác biệt (git diff) giữa các nhánh: `main`, `origin/mac-v2`, `test/perf-threshold-baseline`, `feat/openai-api-va-don-dep-avatar`.
- Lập ma trận đối soát tính năng và kiến trúc:
  - Nhánh `origin/mac-v2`: Lip-sync phoneme (OP_VISME thay vì RMS), giảm độ trễ mẩu TTS đầu, bảo mật chống path traversal vault, nâng cấp bảo mật reqwest/teloxide, telemetry độ trễ thoại 4 mốc (p50/p95), top_k tool calling.
  - Nhánh `test/perf-threshold-baseline`: Module cognitive/redaction, anti-hallucination, xử lý crash n_batch của LLM, kiểm thử tải và đo lường CUDA/vision, hệ thống Foot-plant IK và avatar locomotion.
  - Nhánh `main`: Hạ tầng cargo-deny, chuẩn hóa hình học widget window (`useWidgetWindow.ts`).

### R2. Tích hợp & Nâng cấp Tối ưu (Harmonization & Feature Integration)
- Kế thừa và bảo toàn các thay đổi giá trị đang hoàn thiện trên working tree hiện tại của `test/perf-threshold-baseline`.
- Tích hợp có chọn lọc các tính năng vượt trội từ `mac-v2`:
  - Cơ chế tính toán và truyền tải Viseme phoneme đồng bộ cho Avatar 3D.
  - Tối ưu hóa chuỗi xử lý TTS phát mẩu đầu tiên (First-Chunk Latency).
  - Bản vá an ninh chống tràn đường dẫn (path traversal guard) và cập nhật bảo mật dependency.
  - Cải tiến telemetry giám sát độ trễ hội thoại.
- Giải quyết triệt để các xung đột logic, xung đột kiểu dữ liệu giữa Rust backend (`liva-native-core`) và Vue 3 frontend (`liva-ui`).

### R3. Dọn dẹp Nợ Kỹ thuật & Chuẩn hóa Mã nguồn (Refactoring & Code Hygiene)
- Loại bỏ mã nguồn dư thừa, các file thử nghiệm chết hoặc không còn tham chiếu.
- Đảm bảo tuân thủ tiêu chuẩn code an toàn, không sử dụng các lệnh `unwrap()` thiếu an toàn trong các luồng runtime trọng yếu.
- Duy trì tính nhất quán của hợp đồng giao tiếp IPC giữa Tauri và `liva-native-core`.

### R4. Kiểm chứng Độc lập & Đảm bảo Chất lượng (Autonomous Verification)
- Toàn bộ workspace Rust biên dịch thành công không có lỗi hoặc cảnh báo nghiêm trọng.
- Toàn bộ unit tests và integration tests của `liva-native-core` phải vượt qua (pass 100%).
- Frontend `liva-ui` và desktop client phải vượt qua kiểm tra TypeScript và test suite hiện có.

## Acceptance Criteria

### Tính năng & Kiến trúc
- [ ] Báo cáo đối chiếu các nhánh git được tổng hợp với danh sách đầy đủ các cải tiến được chọn lọc.
- [ ] Tính năng Lip-sync theo Viseme phoneme từ `mac-v2` được tích hợp trơn tru vào hệ thống xử lý giọng nói và Avatar 3D.
- [ ] Các bản vá an ninh bảo vệ MCP vault và cập nhật phụ thuộc từ `mac-v2` được áp dụng thành công.
- [ ] Các tính năng mới về Cognitive, Router/Agent Graph, Foot-plant IK trên `test/perf-threshold-baseline` hoạt động ổn định.

### Chất lượng & Kiểm thử Tự động
- [ ] Lệnh `cargo check --workspace` kết thúc với exit code 0.
- [ ] Lệnh `cargo test -p liva-native-core` vượt qua các bài kiểm thử đơn vị và tích hợp cốt lõi.
- [ ] Lệnh `npm run build:ui` (hoặc `npm run test -w liva-ui`) thực thi thành công không có lỗi type check hoặc build error.
- [ ] Không có file mã nguồn nào bị phá vỡ giao thức kết nối hoặc gây hồi quy (regression) cho các tính năng đã hoạt động.

## Follow-up — 2026-09-06T06:38:47Z

[CRITICAL USER DIRECTIVE UPDATE]
Người dùng đã đưa ra chỉ thị chiến lược rõ ràng:
1. Nền tảng mục tiêu: Tối ưu hoá dự án LIVA phù hợp tuyệt đối với WINDOWS (Windows 10/11 x64, MSVC toolchain, PowerShell, Tauri Windows, Windows path conventions, audio WASAPI/DirectSound, CUDA Windows).
2. Phân định ranh giới 2 nhánh:
   - Nhánh macOS (`mac` / `mac-v2`): Dành riêng cho hệ điều hành macOS (không kéo các đoạn code/script đặc thù darwin-arm64, host_statistics64, start_all.sh vào nhánh này).
   - Nhánh hiện tại: Tập trung vào `main` và môi trường Windows.
3. Nguyên tắc hợp nhất:
   - Chỉ chắt lọc từ `mac-v2` các tính năng logic dùng chung / cross-platform mang lại giá trị cao (như phoneme Viseme lip-sync, giảm độ trễ TTS first-chunk, bản vá an ninh path traversal guard trên Windows backslash, cập nhật dependencies an toàn).
   - Đảm bảo toàn bộ scripts (`.ps1`), paths, Tauri build, cargo test, và dev workflow chạy hoàn hảo trên Windows.
   - Giữ vững mã nguồn tiến tới chuẩn hóa và đồng bộ mượt mà với `main`.

## 2026-09-06T10:14:28Z

Nghiên cứu chuyên sâu toàn diện các công trình nghiên cứu khoa học tiên tiến (arXiv, NeurIPS, ICRA, ACL) và các kho mã nguồn mở hàng đầu trên GitHub trong 4 lĩnh vực cốt lõi: Bộ nhớ dài hạn đa tầng (Memory/GraphRAG), Thoại song công độ trễ cực thấp (Full-Duplex Voice/WebRTC), Avatar 3D VRM & Thị giác đa phương thức (Vision/IK), và Kiến trúc đồ thị tác tử AI cục bộ (Local Agent Graphs/Tools). Tổng hợp thành Bản thiết kế kiến trúc nâng cấp chi tiết (Architectural Blueprint) và Lộ trình triển khai khả thi cho LIVA trên môi trường Windows.

Working directory: e:\Project\LIVA
Integrity mode: development

## Requirements

### R1. Khảo sát & Phân tích Bài báo Khoa học Tiên phong (Academic Literature Review)
- Thu thập và phân tích các nghiên cứu mới nhất (2024–2026) về:
  - **Hệ thống Bộ nhớ Cục bộ**: Hierarchical Memory, Temporal GraphRAG, HippoRAG, Memory Consolidation & Decay.
  - **Hội thoại Song công (Full-Duplex Voice)**: End-to-end Voice-to-Voice models, Turn-taking VAD, Semantics-guided speech synthesis, Audio Latency Optimization.
  - **Tương tác Đa phương thức & 3D Avatar**: Real-time Screen Region-of-Interest (ROI) grounding, Audio-driven Facial Blendshapes (Viseme/Emotion), Expressive Kinematics.
  - **Tác tử AI Tự chủ Cục bộ (Local Edge Agents)**: Structured tool calling, Directed Acyclic Graph (DAG) orchestration, Local Small Language Model (SLM) reasoning routing.
- Mỗi bài báo phải có trích dẫn chuẩn (Tiêu đề, Tác giả, Năm, Link arXiv/DOI) và bài học cốt lõi rút ra cho LIVA.

### R2. Đánh giá & Đối soát Các Kho Mã Nguồn Mở Xuất sắc (Open-Source GitHub Benchmark)
- Khảo sát các repository mã nguồn mở đỉnh cao có khả năng kế thừa hoặc tham chiếu:
  - Bộ nhớ: Letta/MemGPT, Zep, Cognee, LanceDB, SQLite-vec.
  - Giọng nói & Âm thanh: Kokoro-TTS, Piper, VieNeu, Silero VAD, FastVAD, Whisper.cpp, LiveKit/WebRTC.
  - 3D & Avatar: Open-LLM-VTuber, Three-VRM extensions, MediaPipe Holistic, VRChat-compatible OSC pipelines.
  - Agent & Runtime: llama.cpp, Ollama, LangGraph Rust bindings, Rig.rs.
- Đánh giá khả năng tích hợp kỹ thuật vào Rust native engine (`liva-native-core`) và Vue 3/Three.js (`liva-ui`), bao gồm: License (MIT/Apache 2.0), mức tiêu thụ tài nguyên (RAM/VRAM), và tính ổn định trên Windows 10/11 x64.

### R3. Thiết kế Bản Thiết Kế Kiến Trúc & Lộ Trình Nâng Cấp (Architectural Blueprint & Roadmap)
- Xây dựng sơ đồ kiến trúc tổng thể (Mermaid diagram) cho phiên bản LIVA tương lai, giải quyết triệt để các giới hạn hiện tại của dự án.
- Thiết kế chi tiết từng phân hệ: Luồng dữ liệu, Hợp đồng giao tiếp (IPC/WebSocket), và Chiến lược quản lý tài nguyên máy tính cá nhân (RAM ≤ 4GB, VRAM ≤ 6GB).
- Xây dựng lộ trình triển khai theo giai đoạn (Phased Implementation Roadmap) với phân tích rủi ro kỹ thuật cụ thể.
- Xuất bản tài liệu hoàn chỉnh tại `docs/03-danh-gia/LIVA_UPGRADE_RESEARCH_AND_BLUEPRINT_2026.md` và đồng bộ vào Obsidian Vault (`teamwork_projects/obsidian_llm_wiki/vault/Research/`).

## Acceptance Criteria

### Tính Đầy đủ & Chất lượng Học thuật
- [ ] Tối thiểu 10+ bài báo khoa học chất lượng cao được phân tích sâu với liên kết arXiv/DOI xác thực (không có hallucinated papers).
- [ ] Tối thiểu 10+ GitHub repositories mã nguồn mở uy tín được khảo sát chi tiết kèm thông số kỹ thuật, giấy phép bản quyền và đánh giá độ tương thích với Rust/Vue.
- [ ] Phân tích so sánh định lượng (benchmark, latency, memory footprint) giữa các giải pháp công nghệ.

### Giá trị Kiến trúc & Tính Khả thi
- [ ] Có sơ đồ kiến trúc Mermaid trực quan mô tả luồng phối hợp giữa 4 phân hệ (Memory, Voice Duplex, 3D Avatar, Agent Graph).
- [ ] Đề xuất kiến trúc phù hợp 100% với mục tiêu chạy cục bộ trên Windows x64 mà không làm phình tài nguyên hoặc phụ thuộc vào hạ tầng đám mây bắt buộc.
- [ ] Tài liệu kết quả được lưu đầy đủ vào cả thư mục `docs/03-danh-gia/` và Obsidian Vault của dự án.

## 2026-09-06T12:31:09Z

Nghiên cứu và nâng cấp độ tin cậy của LIVA Native Core: Khắc phục triệt để lỗ hổng tràn context ở đường truyền Vision bằng cơ chế kiểm soát trần token/slice ảnh, đồng thời đưa `DynamicPromptAssembler` vào luồng xử lý chính để chủ động điều phối ngân sách prompt, loại bỏ hoàn toàn nguy cơ vượt context gây panic hoặc `abort()` tiến trình lõi.

Working directory: E:\Project\01_AI_Agents\LIVA
Integrity mode: development

## References
- Báo cáo định lượng sự cố `GGML_ASSERT` / `abort()`: `docs/03-danh-gia/05-nang-cap-toan-dien.md:355-450`
- Mã nguồn bộ lắp ráp ngân sách prompt: `liva-native-core/src/llm/prompt/dynamic_prompt.rs`
- Mã nguồn xử lý Vision và sinh ảnh: `liva-native-core/src/llm/engine.rs` (`answer_with_image`) và `liva-native-core/src/websocket.rs:1361`

## Requirements

### R1. Bảo vệ trần token cho đường truyền Vision (Vision Context Guard)
Đảm bảo mọi yêu cầu suy luận qua đường truyền hình ảnh (`answer_with_image`) đều được giới hạn số lượng token ảnh hoặc kiểm tra kích thước prompt kết hợp sau khi giải mã lát cắt (slice). Tuyệt đối không cho phép tổng số token vượt quá `n_ctx` (4096), ngay cả khi người dùng gửi ảnh có độ phân giải cao hoặc nhiều lát cắt.

### R2. Tích hợp DynamicPromptAssembler vào luồng sinh văn bản (Dynamic Prompt Budgeting)
Kết nối bộ điều phối ngân sách prompt (`DynamicPromptAssembler` / `PromptBudget`) vào pipeline xử lý hội thoại chính, đảm bảo hệ thống tự động cắt tỉa hoặc nén thông tin bổ trợ (skills, tools catalog, system persona) để luôn chừa đủ dung lượng context cho câu trả lời và lịch sử hội thoại, không để prompt cơ sở chiếm áp đảo `n_ctx`.

### R3. Bảo toàn đường cơ sở và chuẩn hóa kiểm thử hồi quy (Zero Regression & Robustness)
Mọi thay đổi phải vượt qua toàn bộ các cổng kiểm định chất lượng hiện có của dự án (kiểm thử đơn vị Rust, clippy không cảnh báo, cargo fmt, typecheck UI, và kiểm thử E2E). Bổ sung các ca kiểm thử hồi quy tự động nhằm xác nhận hành vi an toàn khi nhận ảnh kích thước lớn và prompt phức tạp mà không làm sập tiến trình lõi.

## Acceptance Criteria

### Tính ổn định của Vision (Vision Stability)
- [ ] Ảnh có kích thước lớn hoặc chia nhiều lát cắt (>7 slices) không làm sập (`abort()` / `GGML_ASSERT`) tiến trình `liva-native-core`, hệ thống trả lời bình thường hoặc thông báo lỗi tường minh có kiểm soát.
- [ ] Tổng token prompt văn bản + token ảnh sau xử lý mtmd được kiểm soát nghiêm ngặt trong giới hạn an toàn của `n_ctx`.

### Tích hợp Quản lý Ngân sách Prompt (Prompt Budget Integration)
- [ ] `DynamicPromptAssembler` được gọi thực tế trong luồng sinh completion của hệ thống thay vì chỉ xuất hiện trong test file.
- [ ] Khi danh sách tools/skills hoặc persona nền phình to, bộ điều phối tự động cắt tỉa theo đúng hạn ngạch được thiết lập mà không làm hỏng cú pháp prompt.

### Bề mặt Kiểm thử và Cổng Chất lượng (Quality Gates)
- [ ] `cargo test --no-fail-fast` vượt qua 100% (toàn bộ các test suite hiện tại và test mới đều pass).
- [ ] `cargo clippy --all-targets` đạt 0 warning và 0 error.
- [ ] `cargo fmt --all -- --check` đạt chuẩn format sạch (exit code 0).
- [ ] `node scripts/e2e-gateway-ci.mjs` đạt 8/8 thành công.
- [ ] Không có bất kỳ thay đổi nào làm suy giảm các chỉ số coverage hiện tại của `liva-ui`.

## 2026-09-09T04:55:19Z

Nghiên cứu nâng cấp, tối ưu, rà soát toàn diện dự án LIVA; ưu tiên hoàn thiện mô hình nhận diện Wake Word "Hey Liva" (hoạt động nhạy, chính xác cả khi đứng độc lập lẫn trong câu, triệt tiêu kích hoạt sai) và hoàn thiện đường ống thoại Full-Duplex Real-Time cục bộ (Edge/Local Offline, ONNX Runtime, VAD, Barge-in ngắt lời tức thì) với giới hạn RAM nghiêm ngặt (< 2GB) và loại bỏ hoàn toàn các công cụ gây tràn bộ nhớ (GitNexus / nặng graph).

Working directory: e:\Project\01_AI_Agents\LIVA
Integrity mode: development

## Requirements

### R1. Bộ đệm an toàn RAM & Ổn định hệ thống (Strict RAM Guardrails)
- Tuyệt đối cấm khởi chạy GitNexus hoặc bất kỳ công cụ lập chỉ mục đồ thị (graph indexer) nền nào làm tràn RAM hệ thống.
- Giới hạn mọi tiến trình biên dịch và kiểm thử Rust ở mức tối đa 2 luồng: `cargo check -j 2`, `cargo test -j 2 -- --test-threads 2`.
- Kiểm tra dung lượng RAM khả dụng (Pre-flight check: RAM free >= 4GB) trước khi chạy các bộ benchmark hoặc nạp mô hình lớn.
- Không thêm crate kích hoạt xung đột ONNX Runtime backend (ví dụ cấm crate `livekit-wakeword` gây override `ort/alternative-backend`).

### R2. Nâng cấp & Hoàn thiện Wake Word ("Hey Liva")
- Nghiên cứu các kiến trúc Wake Word SOTA mã nguồn mở (openWakeWord, sherpa-onnx KWS, streaming micro-conformer/TC-ResNet ONNX).
- Giải quyết dứt điểm vấn đề "Hey Liva" đứng đơn lẻ (clip ngắn 0.8s - 1.2s) hiện bị STT trả về chuỗi rỗng và classifier hiện tại cho điểm thấp trên giọng người thật.
- Cung cấp mô hình ONNX Wake Word tối ưu kích thước nhẹ (< 15MB), chạy inference trên CPU với độ trễ < 150ms.
- Tỷ lệ kích hoạt sai (False Positive Rate / FPPH) < 1 lần/giờ trong môi trường có tạp âm/TV và Recall >= 90% với từ khóa "Hey Liva".

### R3. Hoàn thiện Đường ống Thoại Real-Time Local (Full-Duplex Offline Voice Pipeline)
- Tối ưu hóa chu trình: Audio Input (16kHz) → AEC & Denoise (GTCRN/WebRTC) → VAD (Silero) → Streaming ASR/STT → Local LLM Engine → Streaming TTS (Piper/Vieneu) → Audio Output.
- Hỗ trợ cơ chế ngắt lời tức thì (Barge-in / Duplex Interruption): Khi trợ lý đang nói qua TTS, người dùng cất giọng thì VAD/core phải lập tức dừng phát audio playback và chuyển sang lắng nghe trong vòng < 200ms.
- Đảm bảo toàn bộ pipeline thoại chạy hoàn toàn Local Offline qua ONNX Runtime với tổng RAM tiêu thụ của pipeline thoại < 2GB.

### R4. Rà soát & Tối ưu Hiệu năng `liva-native-core`
- Rà soát các luồng xử lý WebSocket (`websocket.rs`), frame audio buffer (`webrtc/pipeline.rs`, `webrtc/vad.rs`) để triệt tiêu hiện tượng memory leak, buffer overrun hoặc lock contention giữa các async task Tokio.
- Kiểm tra và đảm bảo không can thiệp, không khôi phục mã nguồn Node.js/Python đã bị khai tử trong kế hoạch Native Rust.
- Duy trì tính toàn vẹn của hợp đồng IPC WebSocket (`OP_WAKE_PROBE` và các opcode thoại) giữa Tauri frontend và Rust backend.

## Verification Plan & Acceptance Criteria

### Resource & Safety Verification
- [ ] Xác nhận không có tiến trình GitNexus hoặc graph indexer nào được triệu gọi trong toàn bộ quá trình thực thi.
- [ ] Dung lượng RAM của tiến trình LIVA Native Core khi chạy thường trực với wake-word listener không vượt quá 350MB; khi đang kích hoạt full voice pipeline không vượt quá 2GB.

### Wake Word Benchmark
- [ ] Bộ kịch bản kiểm thử tự động (dựa trên `scripts/e2e-wake-probe.mjs` hoặc Rust test probe) kiểm tra thành công:
  - 10/10 lần nhận diện đúng với clip "Hey Liva" trần (clip ngắn < 1.5s).
  - 10/10 lần nhận diện đúng với câu dài bắt đầu bằng "Hey Liva...".
  - 10/10 lần từ chối chính xác các câu trò chuyện nền tiếng Việt/tiếng Anh ngẫu nhiên (Zero false alarm trên tập negative benchmark).
- [ ] Độ trễ từ lúc kết thúc phát âm từ khóa đến khi phát tín hiệu Wake kích hoạt <= 200ms.

### Real-Time Duplex Voice Verification
- [ ] Kiểm thử luồng Full-Duplex với kịch bản Barge-in: khi hệ thống đang xuất âm thanh TTS, giả lập tín hiệu giọng nói người dùng vào mic -> hệ thống gửi tín hiệu cancel playback và chuyển state sang listening trong <= 200ms.
- [ ] Toàn bộ bộ crate của `liva-native-core` vượt qua kiểm tra biên dịch và kiểm thử tự động:
  - `cargo check -j 2` hoàn thành không lỗi.
  - `cargo test -j 2 -- --test-threads 2` vượt qua toàn bộ các unit/integration test.

## 2026-09-09T08:56:43Z

Đóng gói, tối ưu và thẩm định toàn diện dự án LIVA để sẵn sàng phát hành sản phẩm thương mại (production release): tạo bộ cài đặt Windows NSIS Offline Installer chuẩn (~230MB), xác thực tính toàn vẹn cấu hình bộ cài, rà soát luồng khởi tạo môi trường sạch (clean first-run setup), hoàn thiện cơ chế an toàn dữ liệu người dùng và kiểm thử smoke-test toàn chu trình với giới hạn RAM nghiêm ngặt (không GitNexus, build -j 2).

Working directory: e:\Project\01_AI_Agents\LIVA
Integrity mode: development

## Requirements

### R1. Rào chắn tài nguyên & An toàn máy trạm (Strict RAM Guardrails)
- Tuyệt đối không sử dụng GitNexus hoặc bất kỳ background graph indexer nào làm cạn kiệt bộ nhớ hệ thống.
- Mọi tiến trình biên dịch phát hành (release build) và kiểm thử phải tuân thủ giới hạn 2 luồng: `cargo check -j 2`, `cargo build --release -j 2`, `cargo test -j 2 -- --test-threads 2`.
- Xác nhận RAM trống khả dụng (Pre-flight check: RAM free >= 4GB) trước khi bắt đầu chu trình build và đóng gói installer.

### R2. Kiểm định Cấu hình Bộ cài & Đóng gói NSIS Installer
- Kiểm tra toàn bộ hợp đồng cấu hình bộ cài đặt theo `scripts/check-installer-config.mjs` (targets đúng `["nsis"]`, `installMode: "currentUser"`, `webviewInstallMode: "offlineInstaller"`, các file resources bắt buộc như `vec0.dll`, `data/models-manifest.json`).
- Đảm bảo `data/liva-config.json` và cơ sở dữ liệu ký ức SQLite KHÔNG bị đóng gói vào thư mục cài đặt, bảo đảm dữ liệu người dùng không bị xoá khi gỡ/nâng cấp ứng dụng.
- Thực hiện đóng gói sản phẩm hoàn chỉnh bằng lệnh `npm run installer:windows` tạo file cài đặt `LIVA_1.0.0_x64-setup.exe` tại `target/release/bundle/nsis/`.

### R3. Thẩm định Luồng Khởi động Môi trường Sạch (Clean First-Run Smoke Test)
- Kiểm thử luồng khởi chạy ứng dụng lần đầu trên một thư mục `LIVA_HOME` hoàn toàn mới:
  1. Hộp thoại khởi tạo thiết bị và giao khoá khôi phục mã hoá (`LIVA — SAO LƯU khoá mã hoá`) hiển thị chính xác.
  2. Tạo khoá thiết bị an toàn, tạo cấu hình `liva-config.json` trong AppData của người dùng.
  3. Cửa sổ `LIVA Widget` hiển thị nổi trên màn hình; cửa sổ `LIVA Dashboard` giữ trạng thái ẩn ban đầu (`visible: false`).
  4. Màn hình chuẩn bị mô hình (`LIVA — Chuẩn bị lần đầu`) kích hoạt với danh sách mô hình kiểm chứng từ `data/models-manifest.json`.

### R4. Tối ưu Hoá Môi trường Production & Dọn dẹp Tài nguyên
- Rà soát toàn bộ cấu hình môi trường sản xuất: tắt các log chẩn đoán nội bộ quá chi tiết (debug trace verbosity), chỉ ghi nhận log sự kiện và lỗi có cấu trúc (structured tracing) với cơ chế xoay vòng tệp (log rotation).
- Đảm bảo các mô hình AI ONNX cốt lõi (Wake Word v3, VAD, Denoise, STT, TTS) có đường dẫn tương đối nhất quán được giải mã chính xác qua `resolve_resource_path` của Tauri khi ứng dụng được cài đặt vào thư mục Program/AppData.
- Kiểm tra và xác nhận tính toàn vẹn của mã băm SHA-256 cho toàn bộ các model được khai báo trong `data/models-manifest.json`.

## Verification Plan & Acceptance Criteria

### Verification Resources
- `npm run check:installer`: Kịch bản kiểm tra toàn diện cấu hình `tauri.conf.json`, license, icons và tài nguyên bundle.
- `npm run build:ui`: Xây dựng production bundle cho frontend UI (`liva-ui/dist`).
- `npm run test:installer`: Bộ kiểm thử tự động cho script kiểm tra bộ cài.
- `node scripts/models.mjs doctor`: Kiểm tra tính toàn vẹn của tất cả model và đường dẫn.
- `docs/02-van-hanh/release-v1.0.0-smoke-test.md`: Tiêu chuẩn nghiệm thu phát hành sản phẩm.

### Acceptance Criteria

#### 1. Installer Configuration & Build Quality
- [ ] `npm run check:installer` chạy thành công với 0 lỗi (exit code 0).
- [ ] `npm run build:ui` biên dịch thành công production frontend không có lỗi TypeScript / Vite bundling.
- [ ] Lệnh đóng gói `npm run installer:windows` tạo thành công file installer NSIS tại `target/release/bundle/nsis/LIVA_1.0.0_x64-setup.exe`.
- [ ] Dung lượng file cài đặt `.exe` nằm trong khoảng tối ưu (khoảng 220MB - 260MB), có mã SHA-256 được tính toán và ghi nhận rõ ràng.

#### 2. Clean Environment First-Run Behavior
- [ ] Mô phỏng khởi động trên môi trường home trống:
  - Ứng dụng tạo đầy đủ cấu trúc thư mục người dùng mà không bị crash.
  - Luồng cấp phát khoá khôi phục mã hoá hoạt động đúng theo kịch bản bảo mật.
  - Cửa sổ Widget khởi tạo đúng kích thước và Dashboard giữ ẩn khi chưa được gọi.
  - Công cụ `models.mjs doctor` xác nhận manifest hợp lệ và các liên kết model khớp mã băm SHA-256.

#### 3. Core Engine & Codebase Health
- [ ] `cargo check -j 2 --release -p liva-native-core -p liva-desktop`: 0 lỗi, 0 cảnh báo nghiêm trọng.
- [ ] `cargo test -j 2 --release -p liva-native-core --lib -- --test-threads 2`: Toàn bộ unit tests vượt qua (PASS).
- [ ] Xác nhận không có tiến trình GitNexus hoặc graph indexer nào được triệu gọi trong suốt chu trình.

## 2026-09-12T17:00:57Z

Xây dựng trọn bộ Hồ sơ Đề án Pitching chi tiết, ngắn gọn (Bộ Pitch Deck 10 cấu phần, kịch bản thuyết trình 5 phút và kịch bản 7 phút phản biện Q&A) cho dự án LIVA dưới định vị "Agentic Harness for Banking & Corporate Treasury Automation" tại Demo Day INNOSTART 2026 (16/09/2026).

Working directory: ~/teamwork_projects/liva_banking_harness
Integrity mode: development

## Requirements

### R1. Bộ Pitch Deck 10 Cấu Phần Chuẩn INNOSTART 2026
Xây dựng nội dung chi tiết, chuẩn xác, dựa trên framework Y Combinator/Sequoia và thế mạnh kỹ thuật cốt lõi của LIVA:
1. **Problem**: Điểm nghẽn đối soát sổ phụ, trễ hạn cảnh báo dòng tiền (T+1 đến T+3), và rào cản cấm đưa dữ liệu nhạy cảm lên Cloud AI theo Nghị định 13/2023/NĐ-CP & Thông tư 09/2020/TT-NHNN.
2. **Customer & Insight**: Khối Ngân hàng Doanh nghiệp (Corporate Banking) và các CFO, Kế toán trưởng doanh nghiệp (Treasury/Cashflow management).
3. **Solution**: LIVA Banking Harness — Trợ lý Agentic AI vận hành Local-first/On-premise bằng Rust, đọc hiểu mọi định dạng sao kê (CSV, OFX, PDF, Excel), đối soát tự động và giám sát dòng tiền 24/7.
4. **Value Proposition**: Tiết kiệm 75% thời gian đối soát thủ công, dự báo rủi ro thâm hụt thanh khoản trước 24-48 giờ, bảo mật Zero Cloud Leakage.
5. **Product/MVP**: Lõi Native Core Rust (RAM ≤ 4GB, VRAM ≤ 6GB), mã hóa AES-256-GCM, cơ chế PolicyEngine với Two-Phase Confirmation và bộ lọc tuân thủ che mờ PII (CCCD, Số tài khoản).
6. **Competitive Advantage**: So sánh chi tiết với RPA truyền thống (cứng nhắc, dễ lỗi khi đổi mẫu) và AI Cloud SaaS (vi phạm quy định an toàn dữ liệu ngân hàng).
7. **Business Model**: B2B Enterprise License cho ngân hàng, B2B Subscription cho doanh nghiệp SMB/CFO, phí tích hợp tùy biến Core Banking/ERP và bảo trì SLA.
8. **Traction & Validation**: Báo cáo trung thực trạng thái MVP hiện tại: Đã kiểm chứng xử lý dữ liệu sao kê mô phỏng 50.000 dòng với độ trễ < 0.5ms, độ chính xác đối soát 99.8%, phản hồi tích cực từ chuyên gia tài chính.
9. **Feasibility**: Kiến trúc tích hợp Non-invasive qua chuẩn file, IPC và API nội bộ an toàn, không xáo trộn Core Banking hiện hữu; lộ trình tham gia Sandbox ngân hàng.
10. **Vision**: Từ trợ lý kiểm soát dòng tiền tiến tới Nền tảng Điều phối Agentic AI bảo mật cao số 1 cho ngành tài chính - ngân hàng tại Việt Nam.

### R2. Kịch Bản Thuyết Trình 05 Phút (Pitch Script)
- Biên soạn kịch bản chi tiết theo từng phút (Minute 1: Problem & Hook -> Minute 2: Solution & Demo -> Minute 3: Tech Core & Advantage -> Minute 4: Business Model & Traction -> Minute 5: Ask & Vision).
- Dung lượng chuẩn từ 650 đến 750 từ tiếng Việt, ngắt nhịp tự nhiên, làm nổi bật các thuật ngữ then chốt thu hút ban giám khảo.

### R3. Bộ Câu Hỏi Phản Biện Q&A 07 Phút (Ban Giám Khảo & Nhà Đầu Tư)
- Xây dựng tối thiểu 08 câu hỏi hóc búa nhất xoay quanh: Độ chính xác của AI cục bộ so với Cloud LLM, tính pháp lý & an toàn dữ liệu ngân hàng, nguy cơ AI ảo giác tự động thao tác sai, và chiến lược thâm nhập thị trường (Go-To-Market).
- Cung cấp câu trả lời sắc sảo, bảo vệ vững chắc lợi thế công nghệ của LIVA.

## Acceptance Criteria

### Tính Đầy Đủ & Khách Quan
- [ ] Đủ 10/10 cấu phần theo đúng quy chế INNOSTART 2026 Demo Day.
- [ ] Thời lượng kịch bản chuẩn xác cho 05 phút nói (khoảng 650 - 750 từ tiếng Việt).
- [ ] Tối thiểu 08 câu hỏi phản biện kèm câu trả lời đắt giá, có chiều sâu kỹ thuật và nghiệp vụ tài chính.
- [ ] Trung thực tuyệt đối về trạng thái MVP và năng lực đã kiểm chứng của LIVA (không phóng đại các số liệu chưa có).
- [ ] Cấu trúc tài liệu sạch, logic, sẵn sàng chuyển giao thành slide trình chiếu và tài liệu phát tay cho BGK.

## 2026-09-12T20:12:26Z

Nâng cấp và kiểm tra toàn diện dự án LIVA (Rust Native Core + Tauri UI), rà soát kiến trúc, hiệu năng, kiểm thử tự động và xử lý triệt để nợ kỹ thuật nhằm đưa toàn bộ hệ sinh thái về trạng thái vận hành tối ưu, an toàn và ổn định nhất.

Working directory: e:\Project\01_AI_Agents\LIVA
Integrity mode: development

## Requirements

### R1. Native Core Hardening & Optimization
Rà soát và tối ưu hóa hệ thống backend `liva-native-core`:
- Kiểm tra toàn bộ vòng đời bộ nhớ, async pipelines, LLM runtime, STT/TTS engine và SQLite WAL connection pool.
- Triệt tiêu các điểm nghẽn hiệu năng, nguy cơ rò rỉ bộ nhớ hoặc unbounded cache/debug artifacts.
- Đảm bảo xử lý lỗi an toàn (graceful error handling), không crash/panic vô điều kiện trong runtime.

### R2. Desktop UI & Tauri IPC Integration
Kiểm tra và nâng cấp tầng giao diện người dùng desktop (`liva-ui`, `liva-desktop`):
- Đảm bảo tính nhất quán và toàn vẹn của hợp đồng WebSocket/IPC giữa Tauri frontend và Native Core backend.
- Xử lý mượt mà reactive state (bao gồm avatar 3D, chat stream, voice dialogue và trạng thái gợi ý chuyên gia).
- Loại trừ mọi lỗi console, cảnh báo type không an toàn hoặc gãy vỡ layout trên desktop client.

### R3. Comprehensive Verification & Quality Gates
Thiết lập và vượt qua toàn bộ các cổng kiểm định chất lượng tự động:
- Kiểm thử Rust Core với đầy đủ unit tests, integration tests và static analysis.
- Kiểm thử Frontend với TypeScript typecheck, ESLint và Vitest coverage.
- Kiểm thử E2E Gateway và socket flows thực tế với hệ thống đang chạy.
- Tuân thủ nghiêm ngặt giới hạn tài nguyên: biên dịch tuần tự với `-j 2` và `--test-threads 2`.

### R4. Technical Debt & Security Compliance
Xử lý nợ kỹ thuật và kiểm tra an toàn hệ thống:
- Rà soát `tech-debt-ledger.json` và `VAN-DE-CAN-XU-LY.md` để giải quyết các vi phạm còn tồn tại.
- Kiểm toán bảo mật: không lộ bí mật/khóa API, tuân thủ lưu trữ dữ liệu an toàn (DPAPI/device key), cô lập quyền truy cập.
- Đồng bộ hóa tài liệu dự án (`PROJECT.md`, `TEST_READY.md`) khớp 100% với hiện trạng mã nguồn.

## Verification Resources
- Rust Test Runner: `cargo test --workspace -j 2 -- --test-threads 2`
- Rust Linter: `cargo clippy --workspace --all-targets -j 2 -- -D warnings`
- Rust Formatter: `cargo fmt --all -- --check`
- Frontend Typecheck: `npx vue-tsc --noEmit -p tsconfig.app.json` (tại `liva-ui`)
- Frontend Lint: `npx eslint . --max-warnings 0`
- Frontend Unit Test: `npm run test:coverage -w liva-ui`
- E2E Gateway Socket CI: `node scripts/e2e-gateway-ci.mjs`
- E2E Test Suite: `node scripts/e2e-test-suite.mjs`
- Memory & Environment Doctor: `node scripts/e2e-memory.mjs` & `npm run doctor`
- Documentation & Skills Integrity: `node scripts/docs-check.mjs` & `npm run skills:audit`

## Acceptance Criteria

### Quality & Build Gates
- [ ] `cargo check --workspace -j 2` và `cargo fmt --all -- --check` vượt qua 100% không có lỗi.
- [ ] `cargo clippy --workspace --all-targets -j 2 -- -D warnings` đạt 0 cảnh báo.
- [ ] `cargo test --workspace -j 2 -- --test-threads 2` vượt qua toàn bộ các test suites (0 test fail).
- [ ] `npx vue-tsc --noEmit` và `npx eslint . --max-warnings 0` đạt 0 lỗi trên toàn bộ mã TypeScript/Vue.
- [ ] `npm run test:coverage -w liva-ui` vượt qua toàn bộ test suites frontend.

### Integration & Runtime Stability
- [ ] `node scripts/e2e-gateway-ci.mjs` đạt 8/8 kịch bản kết nối socket thực tế.
- [ ] `node scripts/e2e-test-suite.mjs` và `node scripts/e2e-memory.mjs` đạt kết quả kiểm tra xanh 100%.
- [ ] Không xuất hiện cảnh báo giả (false alarm) hoặc thông báo lỗi giả định trong log khởi động hệ thống.
- [ ] Cổng `npm run doctor` và `npm run skills:audit` báo cáo tình trạng toàn vẹn hợp lệ.

### Guardrails & Safety Constraints
- [ ] Giới hạn Git: Chỉ dừng ở mức staging (`git add`), tuyệt đối không tự ý chạy `git commit` hay `git push`.
- [ ] Giới hạn tài nguyên: Luôn dùng `-j 2` cho các lệnh build/check của Cargo để bảo vệ RAM hệ thống.
- [ ] Nguyên tắc kiến trúc: Tuyệt đối không phục hồi hoặc chạy mã nguồn cũ Node.js/Python; toàn bộ backend nằm trong Rust core.

## 2026-09-13T05:20:20Z

Nâng cấp toàn diện hệ thống trợ lý ảo cá nhân LIVA theo Bản thiết kế Kiến trúc 2026 (LIVA-ARCH-BLUEPRINT-2026) trên cả 4 phân hệ cốt lõi: Bộ nhớ đa tầng Temporal GraphRAG, Động cơ thoại song công Full-Duplex độ trễ thấp, Avatar 3D VRM động học cộng dồn & thị giác màn hình tối ưu, và Đồ thị tác tử cục bộ DAG định tuyến SLM đạt chuẩn production.

Working directory: e:\Project\01_AI_Agents\LIVA
Integrity mode: development

## Requirements

### R1. Bộ nhớ Đa tầng & Temporal GraphRAG (Hierarchical Memory)
- Xây dựng cơ chế truy xuất tri thức liên kết đa chặng (multi-hop associative retrieval) dựa trên đồ thị tri thức (L3 Knowledge Graph) với thời gian phản hồi P95 < 10 ms mà không tiêu tốn token của LLM trong pha duyệt đồ thị.
- Tích hợp mô hình suy giảm trí nhớ thời gian (Temporal Memory Decay) và tái củng cố ký ức (Dynamic Reinforcement) để giữ bộ nhớ ngữ cảnh dài hạn ổn định, chống trôi nhận thức.
- Xóa bỏ triệt để hiện tượng nghẽn luồng ghi SQLite (`SQLITE_BUSY`) và nguy cơ rơi rụng lượt thoại (Silent Drop Turn) thông qua cơ chế hàng đợi ghi bất đồng bộ tuần tự hóa (DbActor / MPSC write queue).
- Đa luồng hóa tính toán vector embedding thông qua cơ chế chia sẻ `Arc` hoặc worker pool, loại bỏ khóa Mutex toàn cục gây tắc nghẽn STT/Chat.

### R2. Động cơ Thoại Song công Cực thấp Độ trễ (Full-Duplex Voice Engine)
- Triển khai cổng ngắt lượt thoại thích ứng hai giai đoạn (Two-Stage Adaptive Turn-Taking Gate) kết hợp mô hình ngữ nghĩa (Smart Turn v3.2), rút ngắn độ trễ phát hiện dứt câu (SpeechEnd Delay) từ 704 ms xuống 200 – 450 ms mà không bị ngắt lời oan khi người dùng ngập ngừng tiếng Việt.
- Tích hợp triệt tiêu tiếng vọng âm thanh loa ngoài (AEC3 / WASAPI loopback) và khử nhiễu nhân quả (GTCRN STFT-domain) để đảm bảo trợ lý nhận diện chính xác lệnh ngắt (barge-in) của người dùng ngay cả khi máy tính đang phát nhạc hoặc loa ngoài phát âm lượng lớn.
- Phân mảnh dòng TTS theo mệnh đề (Clause Streaming TTS) kết hợp bộ đệm jitter phía client (150 ms) và phát gói tin âm vị (`OP_VISME`), đạt SLA độ trễ toàn trình lượt thoại Fast Voice P90 < 480 ms.

### R3. Avatar 3D VRM Động học Cộng dồn & Thị giác Màn hình Tối ưu (Avatar & Vision)
- Tái cấu trúc chuỗi động học chuyển động của Avatar 3D VRM theo nguyên lý cộng dồn gia số (deterministic additive offsets), bảo toàn tự nhiên chuyển động nhịp thở và đảo mắt khi di chuyển hoặc nói chuyện.
- Hiệu chỉnh chu kỳ bước chân theo quãng đường di chuyển thực tế (distance-based stride calibration) thay vì chu kỳ thời gian tĩnh, triệt tiêu hoàn toàn hiện tượng trượt chân (foot-slide).
- Cô lập hoàn toàn tác vụ nhận diện khuôn mặt MediaPipe FaceLandmarker sang Web Worker, đảm bảo không gây giật khung hình giao diện chính (giữ vững 60 FPS).
- Tối ưu hóa phân hệ thị giác màn hình: sử dụng SIMD Diff chỉ chụp và cắt vùng biến đổi (ROI), co tỉ lệ thích ứng (Co-scale Fallback về 720p khi ROI > 35%), giữ số lượng visual tokens ở mức 144 – 384 tokens và thời gian suy luận VLM dưới 350 ms.

### R4. Đồ thị Tác tử Cục bộ & Định tuyến SLM (Local Agent Graph & SLM Routing)
- Xây dựng đồ thị trạng thái thực thi tác tử bất đồng bộ (Tokio StateGraph DAG) bền vững với cơ chế lưu vết điểm kiểm tra (checkpointing) vào SQLite WAL, cho phép khôi phục trạng thái tác vụ an toàn khi tiến trình khởi động lại.
- Tích hợp bộ định tuyến nhúng (RouteLLM embedding router) để phân luồng câu hỏi tự động giữa mô hình nhanh cục bộ (SLM 3B-4B) và mô hình suy luận sâu (7B+ / Expert Model), chỉ đánh thức mô hình lớn khi thật sự cần thiết.
- Cơ chế truy xuất công cụ theo ngữ nghĩa Top-K (Semantic Tool Retrieval), chỉ nạp các tool schemas liên quan vào context window để tiết kiệm bộ nhớ và tránh phân tâm mô hình.

### R5. Ràng buộc Tài nguyên & An toàn Hệ thống (Governance & Safety Constraints)
- Giới hạn cứng tài nguyên phần hardware trên Windows 10/11 x64: Tổng mức chiếm dụng RAM toàn hệ thống LIVA ≤ 4.0 GB (mục tiêu ~3,000 MB); VRAM đồ họa ≤ 6.0 GB (tiến trình LIVA Core ≤ 5,100 MB). Tự động dỡ mô hình VLM/LLM nặng khi chuyển sang chế độ Voice-only hoặc phát hiện máy tính chạy ứng dụng đồ họa cao (Dynamic Governor Policy).
- Tuân thủ quy tắc biên Git: Giới hạn lệnh git ở mức staging (`git add`), tuyệt đối không tự động `git commit`, `git push`, hoặc chạy các thao tác thay đổi nhánh từ xa.
- Xây dựng và kiểm thử tuần tự với tài nguyên giới hạn: Sử dụng `-j 2` cho `cargo check`/`cargo build`/`cargo test` và `-- --test-threads 2`.
- Cập nhật và đồng bộ tài liệu kiến trúc với Obsidian Vault (`teamwork_projects/obsidian_llm_wiki/vault`).

## Acceptance Criteria

### 1. Kiểm thử Chất lượng & Độ toàn vẹn Mã nguồn (Quality Gates)
- [ ] 100% các cổng kiểm thử cốt lõi đều đạt màu xanh:
  - `cargo fmt --all -- --check` đạt exit code 0.
  - `cargo clippy --workspace --all-targets -- -D warnings` đạt 0 warning / 0 error.
  - `cargo test --workspace -j 2 -- --test-threads 2` vượt qua toàn bộ các bài kiểm thử đơn vị và tích hợp (không có bài test nào fail hoặc ignored bất thường).
  - `npx vue-tsc --noEmit -p tsconfig.app.json` đạt 0 lỗi typecheck.
  - `npx eslint . --max-warnings 0` đạt 0 cảnh báo.
  - `npm run test:coverage -w liva-ui` đạt màu xanh trên toàn bộ test suite frontend.
  - `npm run doctor` và `npm run skills:audit` xác nhận cấu hình và skills hoàn toàn chuẩn hóa.

### 2. Tiêu chí Đo lường Hiệu năng & Ổn định (Performance & Reliability SLAs)
- [ ] **Không rơi rụng lượt thoại**: Bài kiểm thử áp lực ghi đồng thời (concurrent write stress) chứng minh 100% lượt thoại được lưu trữ trọn vẹn vào SQLite WAL thông qua hàng đợi DbActor, không xuất hiện lỗi `SQLITE_BUSY`.
- [ ] **Độ trễ truy xuất Đồ thị Tri thức**: Phép đo benchmark truy vấn liên kết đa chặng L3 qua HippoRAG PPR trên bộ đệm In-Memory CSR Graph Cache đạt thời gian P95 < 10.0 ms.
- [ ] **Độ trễ ngắt lượt thoại**: Cổng ngắt lượt hai giai đoạn cắt câu chính xác trong khoảng 200 – 450 ms khi người dùng dừng nói.
- [ ] **Chen ngang (Barge-in)**: Mô phỏng phát âm thanh loa ngoài và giọng nói người dùng chen vào chứng minh bộ AEC3 và VAD triệt tiêu được tiếng loa và phát hiện giọng người dùng trong vòng < 100 ms từ khi bắt đầu nói.
- [ ] **Động học Avatar**: Kiểm thử kiểm tra không còn hiện tượng gán đè trực tiếp làm triệt tiêu nhịp thở (breath oscillation) và bước chân avatar được tính toán theo tỉ lệ quãng đường thực tế.
- [ ] **Tuân thủ Trần Tài nguyên**: Giám sát tiến trình LIVA trong suốt quá trình chạy kiểm thử không vượt quá 4.0 GB RAM hệ thống và 5,100 MB VRAM.

## 2026-09-13T12:12:17Z

Tách và tái cấu trúc hệ thống LIVA thành dự án độc lập LIVA Banking Harness tại thư mục mới, chuyên biệt cho trợ lý đối soát ngân hàng và quản trị dòng tiền doanh nghiệp tham gia InnovationStart 2026, loại bỏ hoàn toàn module và tài nguyên 3D Avatar, trang bị giao diện 2D Banking Dashboard chuyên nghiệp.

Working directory: e:\Project\01_AI_Agents\LIVA_Banking
Integrity mode: demo

## Requirements

### R1. Thiết lập dự án độc lập và dọn sạch 3D Avatar
Tạo dự án độc lập tại `e:\Project\01_AI_Agents\LIVA_Banking` kế thừa nền tảng cốt lõi của LIVA. Loại bỏ triệt để toàn bộ mã nguồn, thư viện và tài nguyên liên quan đến 3D Avatar (BabylonJS, ThreeJS, VRM models, animation loader, blendshape lip-sync, face tracking worker), giúp giảm tải dung lượng và tối ưu hóa RAM cho môi trường máy trạm ngân hàng.

### R2. Giao diện người dùng 2D Banking & Treasury Dashboard
Xây dựng giao diện Desktop/Web 2D hiện đại, chuyên nghiệp dành cho nghiệp vụ ngân hàng và tài chính doanh nghiệp:
- Bảng điều khiển dòng tiền và trạng thái thanh khoản (Cash flow & Liquidity Dashboard).
- Khu vực kéo thả "Hot-Folder" nhận diện và tải lên sao kê ngân hàng (VCB, TCB, BIDV...).
- Bảng đối chiếu giao dịch (Reconciliation Matrix) hiển thị trạng thái khớp (Matched), lệch số liệu (Discrepancy), và chờ duyệt (Pending HITL).
- Khung tương tác trợ lý tài chính (Voice/Chat) phục vụ tra cứu số dư, dòng tiền và phân tích bất thường.

### R3. Lõi xử lý đối soát siêu tốc và an toàn dữ liệu On-Premise
Tích hợp và tối ưu module Rust Native Core cho nghiệp vụ ngân hàng:
- Bộ bóc tách sao kê ngân hàng định dạng CSV/Excel/PDF với tốc độ cao.
- Động cơ đối soát giao dịch (Reconciliation Engine) so khớp tự động giữa sao kê ngân hàng và sổ cái nội bộ với độ trễ tối thiểu và 0% ảo giác số học.
- Cơ chế bảo mật dữ liệu cục bộ: Mã hóa cơ sở dữ liệu AES-256-GCM, tuân thủ Nghị định 13/2023/NĐ-CP, kiểm soát audit log không rò rỉ dữ liệu tài chính ra Internet.

### R4. Bộ hồ sơ và kịch bản Demo Day InnovationStart 2026
Tích hợp toàn bộ tài liệu thuyết trình, pitch deck và kịch bản demo (từ `teamwork_projects/liva_banking_harness`) vào dự án mới, đồng thời cung cấp bộ dữ liệu sao kê mẫu (mock statements của VCB, TCB, BIDV) và script kiểm thử tự động để phục vụ trình diễn live 5 phút trước Hội đồng Ban Giám khảo.

## Acceptance Criteria

### Tính độc lập và Tinh gọn mã nguồn
- [ ] Dự án mới tại `e:\Project\01_AI_Agents\LIVA_Banking` hoàn toàn độc lập, có thể build và chạy mà không cần tham chiếu đến repo LIVA gốc.
- [ ] Không còn bất kỳ package 3D nào trong `package.json` (three, @babylonjs, @pixiv/three-vrm) và không còn file model `.vrm`, `.glb` trong assets của dự án mới.
- [ ] Lệnh `cargo check -j 2` cho native core và `npm run build` cho frontend chạy thành công không có lỗi biên dịch.

### Chức năng Nghiệp vụ Ngân hàng & UI
- [ ] Giao diện 2D hiển thị hoàn chỉnh Dashboard tài chính, bảng sao kê giao dịch và khung chat trợ lý mà không phát sinh bất kỳ lỗi console liên quan đến 3D canvas/WebGPU/WebGL.
- [ ] Chức năng Hot-Folder / Upload file sao kê mẫu hoạt động trơn tru, hiển thị kết quả phân tích và đối soát giao dịch trực quan.

### Kiểm thử & Tự động hóa
- [ ] Có kịch bản kiểm thử tự động (test script) chứng minh khả năng bóc tách và đối soát sao kê với kết quả chính xác 100% về mặt số học.
- [ ] Toàn bộ tài liệu Pitching (Pitch Deck, Pitch Script 5 phút, Q&A Defense) được cấu trúc gọn gàng trong thư mục `docs/pitching` của dự án mới.

## 2026-09-13T12:34:21Z

[USER DESIGN UPDATE / CRITICAL DIRECTIVE]
Người dùng vừa gửi bản thiết kế giao diện thực tế chuẩn (Mockup UI) cho LIVA Banking và yêu cầu thiết kế lại Dashboard hoàn toàn theo mẫu này:

Tệp ảnh mockup người dùng cung cấp:
`C:/Users/Admin/.gemini/antigravity/brain/35a07cdc-fdc0-4378-bee5-2d8187863969/.user_uploaded/media_1789302786301.png`

Nội dung đặc tả giao diện cần áp dụng vào Milestone 2 (2D Banking UI):
1. Brand & Header: Tiêu đề `liva://reconciliation.local` có badge `MVP THỰC TẾ`, Brand `LIVA Reconciliation`, top bar hiển thị profile ("Nguyễn Minh Trí"), bell thông báo, thanh tìm kiếm.
2. Sidebar điều hướng:
   - Header TỔNG QUAN
   - TỔNG QUAN (active)
   - ĐỐI SOÁT (chấm xanh active status)
   - GIAO DỊCH
   - THU CHI
   - NGÂN QUỸ
   - BÁO CÁO
   - CÀI ĐẶT
3. Khối thẻ ngân hàng (Hàng trên):
   - Thẻ Vietcombank: Số dư 1,450,230,000 VND, Đã đối soát 1,449,850,000 VND, Chưa khớp 380,000 VND, Last sync.
   - Thẻ Techcombank: Số dư 785,600,000 VND, Đã đối soát 785,100,000 VND, Chưa khớp 500,000 VND.
   - Khối KẾT QUẢ ĐỐI SOÁT TỰ ĐỘNG: Gauge bán nguyệt 99.8% Tỷ lệ khớp, Đã khớp 1,842/1,845 GD, Tự động 99.2%, Bằng tay 0.6%, Chưa khớp 3 (0.2%).
4. Khối biểu đồ (Hàng giữa):
   - Biểu đồ XU HƯỚNG ĐỐI SOÁT (Line chart 30 ngày: Matched vs Unmatched).
   - Biểu đồ PHÂN BỔ TRẠNG THÁI (Donut chart: Khớp 99.8% vs Chưa khớp 0.2%).
5. Bảng SỔ GIAO DỊCH CHÍNH TÌM THỰC (Real-time Transaction Ledger) ở hàng dưới cùng, tích hợp Hot-Folder tải file sao kê.

Yêu cầu chuyển tiếp ngay lập tức chỉ thị này cho Project Orchestrator và Worker phụ trách Milestone 2 để đảm bảo giao diện được hiện thực hóa chuẩn xác 1:1 theo thiết kế này.


