# LIVA System — Memory Architecture (LIVA-UHM)

> **Lưu ý:** Tài liệu này được trích xuất và tổng hợp từ `AI_CONTEXT.md` (Version: 2026-05-09). Nó mô tả chi tiết về hệ thống kiến trúc bộ nhớ Hợp nhất (Consolidated Brain) của dự án LIVA.

## 1. Tổng quan Kiến trúc (Consolidated Brain)
LIVA sử dụng hệ thống kiến trúc bộ nhớ được gộp chung lại (Consolidated Brain), loại bỏ các dependencies cũ như `LanceDB` và `flexsearch`. 
Thay vào đó, tất cả dữ liệu được gom chung về **1 file `node:sqlite` duy nhất**, kết hợp với C-Extension `sqlite-vec` (cho Vector Search) và `FTS5` (cho Full-Text Search).

Hệ thống hoạt động với luồng ghi được **Debounced** để tối ưu hóa hiệu năng và I/O.

## 2. Các tầng bộ nhớ (Memory Tiers: L0 - L3)
Hệ thống lưu trữ chia làm 4 cấp độ (Tiers) theo vòng đời dữ liệu:

- **L0: TurboQuantStore (VRAM)**
  - Vùng nhớ làm việc tức thời (Working memory).
  - Sử dụng Quantized vector memory (4-bit KV cache).
  - Chạy in-process trên VRAM.
. 
- **L1: StructuredMemory (SQLite)**
  - Tầng Turn Layer nền tảng.
  - Lưu trữ trực tiếp các raw conversational turns vào bảng `turn_layer_nodes`.
  - Quản lý Key-value facts và các Event bricks (Bao gồm hệ quy chiếu kép: Φ Factual + Ψ Relational).
  - Có cơ chế quản lý vòng đời bằng TTL và xử lý theo FIFO.

- **L2: VectorMemory (sqlite-vec)**
  - Tầng Event Layer chứa các câu chuyện (narratives) đã được củng cố.
  - Chứa semantic vector search (nằm chung trong file `.sqlite`).

- **L3: PersonalKnowledge (KV)**
  - Lưu trữ các insight cốt lõi (Core insights).
  - Thói quen, sở thích của người dùng (User preferences).
  - Vùng nhớ chiến lược dài hạn (Strategic memory).

## 3. Các Daemons và Thành phần Cốt lõi
Các hệ thống ngầm và service chính đảm nhiệm việc thao tác và tự động hóa bộ nhớ:

- **SemanticRouter** (`src/memory/SemanticRouter.ts`):
  - Chịu trách nhiệm phân loại intent và định tuyến (Routing).
  - Tính toán Cosine similarity thông qua `sqlite-vec`. 
  - Phản hồi cực nhanh `<100ms`, hỗ trợ 5 routes (bao gồm `tool_recall`) với ngưỡng nhận diện (adaptive threshold) linh hoạt.
  
- **ReflectionDaemon** (`src/memory/ReflectionDaemon.ts`):
  - Hoạt động bất đồng bộ 100%. Đảm nhiệm việc bóc tách thông tin kép (Dual-Perspective Φ/Ψ).
  - Xử lý các batch dữ liệu sử dụng Zod Dual Schema.
  - Delay ghi bằng debounce (12s micro-batch) sau mỗi lượt hội thoại.

- **ConsolidationCron** (`src/memory/ConsolidationCron.ts`):
  - Tiến trình củng cố dữ liệu chạy lúc hệ thống Sleep (Sleep-time consolidation).
  - Khi hệ thống idle (mặc định 30 phút), cold-start, hoặc trigger thủ công, tiến trình sẽ tổng hợp từ tầng L1 sang L2 (thành `AXIOM` và temporal `ANCHOR` vectors).

- **HeraCompass** (`src/memory/HeraCompass.ts`):
  - DB lưu trữ các Error insights phục vụ cơ chế tự phục hồi (Self-Healing).
  - Dùng FTS5 để Full-text search theo điểm hữu dụng (utility scoring). Khi Agent bị lỗi, HeraCompass sẽ truy xuất các lesson quá khứ để LLM sửa lỗi.

- **SensoryManager** (`src/memory/SensoryManager.ts`):
  - Quản lý và tổng hợp các đầu vào đa phương tiện (Multi-modal input aggregation).
  - Có cơ chế dọn rác (GC) và TTL (Giới hạn vòng đời). Đặc biệt có luồng `sanitizeSensoryData()` để cắt bớt input (<2000 ký tự), bỏ thẻ HTML, ngăn chặn prompt injection.

- **RamCacheManager** (`src/memory/RamCacheManager.ts`):
  - Quản lý Bounded FIFO message cache.
  - Xử lý xóa data chuẩn theo luật bảo mật (GDPR purge).

## 4. Luồng xử lý dữ liệu Memory
1. **Routing:** 
   *User Input* -> `SemanticRouter` sẽ định tuyến truy vấn đến đúng tier bộ nhớ (xử lý dưới 100ms qua `sqlite-vec cosine` + `FTS5`).
2. **Turn Extraction:** 
   Sau khi AI Response, `ReflectionDaemon` sẽ thêm turn hội thoại vào hàng đợi để bóc tách thông tin Φ/Ψ theo cụm (debounced 12s).
3. **Macro Synthesis:** 
   Trong khoảng thời gian hệ thống rảnh rỗi, `ConsolidationCron` tự động chuyển đổi thông tin từ L1 thành các L2 Vector và L3 KV.

## 5. An toàn và Toàn vẹn Dữ liệu
- **EncryptionEngine:** Tập trung xử lý mã hoá AES-256-GCM cho tất cả các bản ghi nhạy cảm.
- **Atomic Writes:** TUYỆT ĐỐI áp dụng pattern ghi file `.tmp` sau đó gọi hàm `rename()` để tránh lỗi corrupt file khi bị crash giữa chừng.
- **Safe I/O:** Không bao giờ dùng `setTimeout` giả để chờ DB xả WAL, luôn luôn dùng event native `await db.close()`. Mặc định chạy SQLite với `PRAGMA journal_mode = WAL` và `synchronous = NORMAL`.
