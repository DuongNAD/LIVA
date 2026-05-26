# 02. Hệ Thống Bộ Nhớ H-MEM v18 (Memory Subsystem)

**Phiên bản: v26 Enterprise-Ready Cognitive OS**

Hệ thống bộ nhớ của LIVA loại bỏ hoàn toàn các dependencies cũ như `LanceDB` và `flexsearch` để gom gọn toàn bộ dữ liệu vào **1 file `node:sqlite` duy nhất**, kết hợp với các C-Extension hiệu năng cao: `sqlite-vec` (Vector Search) và `FTS5` (Full-Text Search).

---

## 1. Bản đồ Phân lớp Bộ nhớ (Memory Tiers: L0 - L3)

Hệ thống lưu trữ của LIVA được phân rã thành 4 cấp độ (Tiers) theo vòng đời và đặc tính truy xuất dữ liệu:

```
┌────────────────────────────────────────────────────────┐
│ L0: TurboQuantStore (RAM Cache / VRAM)                 │
│ ──> Cửa sổ chat tức thời (memCache, max 200 msgs)      │
└───────────────────────────┬────────────────────────────┘
                            │ (Flush khi phân tách Episode)
                            ▼
┌────────────────────────────────────────────────────────┐
│ L1: EventRepository (Turn Layer - SQLite)              │
│ ──> Raw turns (turn_layer_nodes) + Phi/Psi Events      │
└───────────────────────────┬────────────────────────────┘
                            │ (ConsolidationCron / RAPTOR)
                            ▼
┌────────────────────────────────────────────────────────┐
│ L2: VectorRepository (Event Layer - sqlite-vec + FTS)  │
│ ──> AXIOMs & ANCHORs (INT8, Hybrid Search RRF)         │
└───────────────────────────┬────────────────────────────┘
                            │ (ArchivingCron / Active Forgetting)
                            ▼
┌────────────────────────────────────────────────────────┐
│ L3: PersonalKnowledge (Facts KV & GraphRepository)     │
│ ──> Facts (Ebbinghaus Decay) + Dynamic Knowledge Graph │
└────────────────────────────────────────────────────────┘
```

### L0: TurboQuantStore & WorkingBuffer (RAM/VRAM)
* **Vùng nhớ làm việc tức thời** (Working Memory). Xử lý ngữ cảnh ngắn hạn đang diễn ra trong cuộc trò chuyện (Episode) hiện tại.
* **RAM Cache (`memCache`)**: Lưu trữ lịch sử hội thoại trên RAM nhằm loại bỏ hoàn toàn Disk I/O trong lúc chat. Có cơ chế tự dọn rác (GC) khi cache vượt quá 200 tin nhắn (chặt bớt còn 100 tin nhắn gần nhất) để chống phình RAM.
* **Quantized Memory**: Nén và mã hóa in-process bằng Token-minting auth system (`CoreKernel` + `QuantizedMemoryStore`) ghi vào `turbo_quant_memory.jsonl` với cơ chế tạo vector nền (background embedding) để tránh block Event Loop.
* **Cross-Session Warm-up**: Khi khởi động, LIVA tự động nạp 10 lượt hội thoại gần nhất của 24h qua làm ngữ cảnh đệm, ngăn chặn lỗi ảo giác (anti-hallucination).

### L1: EventRepository (Turn Layer - SQLite)
* **Turn Layer**: Lưu trữ trực tiếp các raw conversational turns vào bảng `turn_layer_nodes` và các event bricks vào bảng `events` dưới hệ quy chiếu kép:
  * **Φ (Phi) Factual**: Thông tin thực tế (facts, entities).
  * **Ψ (Psi) Relational**: Trạng thái cảm xúc, ý định và mối quan hệ (sentiment, intent, relational).
* **Debounced Memory Touch**: Khi một phần tử bộ nhớ được truy xuất, nó sẽ được đưa vào hàng đợi Touch Queue (max 1000 items, early flush tại 900, mặc định tự xả sau mỗi 15 giây) để cập nhật trường `last_accessed_at`, giảm thiểu hiện tượng Write Amplification trên ổ SSD.

### L2: VectorRepository (Event Layer - sqlite-vec)
* **Event Layer**: Chứa các "Câu chuyện" (Narratives) đã được củng cố thành `AXIOM` (chân lý) và `ANCHOR` (mốc thời gian).
* **INT8 Quantization**: Tự động chuyển đổi và lưu trữ vector dưới dạng INT8 giúp giảm 75% RAM footprint mà không làm giảm độ chính xác của tìm kiếm ngữ nghĩa.
* **Hybrid Search (RRF)**: Kết hợp kết quả tìm kiếm ngữ nghĩa (KNN qua `sqlite-vec`) và tìm kiếm văn bản (BM25 qua `FTS5`porter tokenizer) bằng công thức Reciprocal Rank Fusion (RRF):
  $$\text{Score} = \sum \frac{1}{60 + \text{Rank}}$$
* **Positional Drill-down Index**: Mỗi vector L2 chứa liên kết `source_event_ids` trỏ ngược về các bản ghi L1 tương ứng, cho phép truy xuất chi tiết luồng hội thoại gốc.
* **DLQ (Dead Letter Queue)**: Cơ chế bù giao dịch (compensating transactions) tự động thử lại các lệnh xóa vector bị lỗi (tối đa 3 lần) trước khi đưa vào hàng đợi lỗi.

### L3: PersonalKnowledge (Facts KV & Knowledge Graph)
* **Facts KV Store**: Lưu trữ các Core Insights (thói quen, sở thích, cấu hình cá nhân) dưới dạng Key-Value trong bảng `facts`.
  * **Ebbinghaus Memory Decay**: Áp dụng Spaced Repetition (lặp lại ngắt quãng). Mỗi lần đọc sẽ đưa `memory_strength` về `1.0`. Khi Consolidation chạy, hệ thống sẽ tính độ phai nhạt ký ức theo công thức:
    $$S(t) = S_0 \times e^{-\lambda \times \text{days\_since\_access}}$$
    Chạy hoàn toàn bất đồng bộ (chặn 500 bản ghi kèm lệnh yield `setImmediate` để bảo vệ Event Loop) và tự động xóa bỏ/lưu trữ các ký ức mờ nhạt có strength < 0.1.
* **Dynamic Knowledge Graph** (`GraphRepository`):
  * Lưu trữ thực thể và mối quan hệ động trong bảng `l3_nodes` và `l3_edges`.
  * Hỗ trợ tìm kiếm đa bước (Multi-hop Traversal) thông qua **Recursive CTE** của SQLite để tìm kiếm mối quan hệ phức tạp.

---

## 2. Các Động cơ và Daemons vận hành ngầm (Background Daemons)

Hệ thống hoạt động bất đồng bộ 100% để đảm bảo giao diện chat không bị giật lag (block Event Loop):

### 1. SemanticRouter (`src/memory/SemanticRouter.ts`)
* Định tuyến nhanh các yêu cầu của người dùng sang 5 routes (bao gồm `tool_recall`) thông qua vector search và FTS5 với phản hồi `<100ms`.

### 2. ReflectionDaemon (`src/memory/ReflectionDaemon.ts`)
* Chạy nền với cơ chế **Debounce 12s**. 
* Tự động phân tích câu trả lời của AI để bóc tách thông tin cấu trúc Φ/Ψ dựa trên Zod Dual Schema và lưu tạm vào bảng `events` (L1).

### 3. ConsolidationCron (`src/memory/ConsolidationCron.ts`)
Tiến trình củng cố ký ức chạy khi máy tính nhàn rỗi (Idle 30 phút), Cold-start, hoặc trigger thủ công:
* **VRAM Guard & State Checking**: Tuyệt đối không chạy khi LLM đang streaming/thinking để tránh tranh chấp tài nguyên GPU/VRAM.
* **Energy-Aware Battery Throttling**: Tự động phát hiện nếu thiết bị đang chạy bằng PIN (`is_battery: true` trong `hardware_state.json`), nhân ngưỡng kích hoạt consolidation lên gấp 5 lần (từ 10 lên 50 events) để tiết kiệm pin.
* **Recursive Summarization (RAPTOR Tree)**: Tóm tắt đệ quy các leaf nodes hội thoại L1 lên các mức cao hơn (L2 Anchors) tạo cấu trúc cây phân mảnh hỗ trợ tra cứu ngữ cảnh dài hạn.
* **Reconsolidation Engine**: Quét các `AXIOM` mới đối chiếu với L2/L3 để xác định: `Independent` (thêm mới), `Extendable` (hợp nhất nâng cấp thông tin), hoặc `Contradictory` (ghi đè mâu thuẫn).
* **Dynamic Taxonomy Management**: Tự động gom nhóm các tag phân loại chưa xác định `Unknown_*` bằng kỹ thuật chuẩn hóa ngữ nghĩa, nâng cấp lên domain chính thức nếu có từ 3 axioms trở lên, hoặc xóa bỏ nếu không còn sử dụng.
* **WAL Checkpoint (PASSIVE) & Atomic Snapshot Backup**: Thực thi dọn dẹp log WAL an toàn và sao lưu dự phòng định kỳ (`VACUUM INTO`).

### 4. ContradictionResolver (`src/memory/ContradictionResolver.ts`)
* Chạy ngầm khi củng cố đồ thị L3 Graph.
* Tìm kiếm các mối quan hệ tương đồng ở L2 (cosine similarity > 0.85), gọi LLM router siêu nhẹ (Gemma/Mini) kiểm tra mâu thuẫn trực tiếp và đánh dấu các liên kết cũ thành `obsolete = 1`.

### 5. ArchivingCron (`src/memory/ArchivingCron.ts`)
* Mô phỏng cơ chế dọn dẹp ký ức chủ động (Active Forgetting) lúc ngủ.
* Định kỳ (24 giờ) quét các vector cũ hơn 30 ngày ít truy cập (access_count <= 2) hoặc bị mờ nhạt (decay_weight < 0.5).
* Gom nhóm theo domain, dùng LLM tóm tắt thành các khái niệm cốt lõi (Core Concepts), lưu vào L3 dưới dạng `ArchiveNode` chứa pointer tham chiếu đến file cold storage `.jsonl` trên ổ cứng.
* Xóa hoàn toàn bản ghi chi tiết cũ tại L1/L2 và gọi lệnh `VACUUM` để giải phóng dung lượng đĩa cứng.

### 6. SemanticCache (`src/memory/SemanticCache.ts`)
* Bộ đệm RAM lưu trữ tới 500 câu lệnh ngắn (tối đa 20 từ) kết hợp so khớp chính xác và tìm kiếm mờ (Fuzzy Match) qua khoảng cách **Levenshtein** (ngưỡng tương đồng >= 0.95) để trả kết quả ngay tức thì, bỏ qua LLM routing.

---

## 3. Quy trình Xử lý Dữ liệu Memory (Data Lifecycle)

```
[User Input]
     │
     ├──> [SemanticCache] (Fuzzy Match Levenshtein) ── (Hit) ──> [Return Action]
     │
     └──> [SemanticRouter] (FTS5 + sqlite-vec < 100ms)
               │
               ▼
   [Hybrid RAG Context Lookup]
   (Vector KNN + FTS BM25 merged via RRF)
               │
               ▼
     [AI Generation Loop] (LLM Response)
               │
               ▼ (addMessage)
   [TurboQuantStore & RAM Cache] (L0)
               │
               ▼ (ReflectionDaemon - Debounce 12s)
   [EventRepository / Turn Layer] (L1 Φ/Ψ)
               │
               ▼ (ConsolidationCron - Sleep/Idle)
   ┌────────────────────────────────────────────────────────┐
   │ 1. RAPTOR Recursive Summarization                      │
   │ 2. Reconsolidation & Contradiction Checking            │
   │ 3. L2 Vector & L3 Graph / Facts Insertion              │
   └───────────────────────────┬────────────────────────────┘
                               │
                               ▼ (ArchivingCron - 24h interval)
             [Cold Storage .jsonl & VACUUM]
```

---

## 4. An toàn và Toàn vẹn Dữ liệu (Disk Safety)

* **Atomic Writes**: Luôn áp dụng ghi tệp tạm `.tmp` và đổi tên `rename()` để ngăn ngừa lỗi corrupt database khi mất nguồn đột ngột.
* **Safe Shutdown**: Khi hệ thống tắt (`CoreKernel.shutdown()`), tiến trình sẽ xả sạch hàng đợi `ReflectionDaemon` và `StructuredMemory.flushTouchQueue()` trước khi đóng kết nối SQLite thông qua native `db.close()` sự kiện.
