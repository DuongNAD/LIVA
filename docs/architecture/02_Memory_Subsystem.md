# 02. Hệ Thống Bộ Nhớ H-MEM v18 (Memory Subsystem)

**Phiên bản: v26 Enterprise-Ready Cognitive OS**

Hệ thống bộ nhớ của LIVA được triển khai dưới kiến trúc **LIVA Unified Hybrid Memory (LIVA-UHM)** hay còn gọi là **H-MEM v18 (HiGMem Phase 3)**. Thay vì sử dụng các cơ sở dữ liệu vector nặng nề bên ngoài (như LanceDB) và công cụ tìm kiếm cồng kềnh trên RAM (như flexsearch), LIVA gom gọn hoàn toàn vào **1 file `node:sqlite` duy nhất** được tăng cường bằng các C-Extension bản xứ (`sqlite-vec` cho tìm kiếm vector và `FTS5` cho tìm kiếm toàn văn bản).

---

## 1. Bản Đồ 4 Tầng Bộ Nhớ (Memory Tiers: L0 - L3)

### L0: TurboQuantStore & WorkingBuffer
* **Vị trí**: RAM (Hot Cache) & File `turbo_quant_memory.jsonl`
* **Chức năng**: Vùng nhớ làm việc tức thời (Working Memory). Lưu trữ ngữ cảnh ngắn hạn đang diễn ra trong cuộc trò chuyện (Episode) hiện tại.
* **RAM Cache (`memCache`)**: Cho phép truy xuất tức thời lịch sử tin nhắn mà không mất phí Disk I/O. Cache tự động dọn dẹp khi vượt quá 200 tin nhắn (slice lấy 100 tin nhắn mới nhất) để bảo vệ Event Loop.
* **Nén Lượng tử**: Tin nhắn được mã hóa in-process dưới dạng các token định danh thông qua `CoreKernel` và lưu trữ xuống `turbo_quant_memory.jsonl`.
* **Cross-Session Warm-up**: Khi khởi động hệ thống (boot-time), LIVA tự động quét và nạp 10 lượt hội thoại gần nhất trong vòng 24h qua làm ngữ cảnh đệm (Warm-up) để AI phản hồi tự nhiên và tránh hiện tượng ảo giác thông tin.

### L1: EventRepository (Turn Layer)
* **Vị trí**: CSDL SQLite (`turn_layer_nodes` và `events` tables)
* **Chức năng**: Lưu trữ trực tiếp các raw conversational turns và các khối sự kiện (Event bricks) đã bóc tách.
* **Hệ quy chiếu kép (Dual-Perspective)**: Lưu trữ dữ kiện dưới 2 khía cạnh:
  * **Φ (Phi) Factual**: Các sự thật khách quan thu thập từ hội thoại.
  * **Ψ (Psi) Relational**: Các yếu tố phi cấu trúc như cảm xúc của người dùng, ý định tiềm ẩn và sắc thái mối quan hệ giữa người dùng và AI.
* **Debounced Memory Touch**: Các truy vấn đọc/ghi thông thường sẽ ghi nhận hành vi "chạm bộ nhớ" (Touch) vào Touch Queue trên RAM (sức chứa 1000 items, tự động flush sớm tại 900 hoặc định kỳ mỗi 15s). Cơ chế này gom cụm các lệnh UPDATE trường `last_accessed_at` của SQLite thành một giao dịch đơn, tránh lặp lại I/O làm hại ổ đĩa SSD.

### L2: VectorRepository (Event Layer)
* **Vị trí**: CSDL SQLite (bảng vector ảo `vec_idx` và bảng siêu dữ liệu `vectors_meta`)
* **Chức năng**: Chứa các "Câu chuyện" (Narratives) đã được củng cố. Các `AXIOM` (chân lý dài hạn) và `ANCHOR` (mốc thời gian).
* **Nén Vector INT8**: Hệ thống tích hợp thuật toán lượng tử hóa vector `vec_quantize_int8(embedding, 'unit')` của `sqlite-vec`. Các vector nhúng Float32 được nén thành dạng INT8 trong C++, giúp **tiết kiệm 75% dung lượng RAM** của chỉ mục vector mà vẫn duy trì khoảng cách cosine cực kỳ chính xác.
* **Hybrid Search với RRF (Reciprocal Rank Fusion)**: Kết hợp tìm kiếm ngữ nghĩa KNN (Cosine similarity qua `sqlite-vec`) và tìm kiếm văn bản Porter tokenization (qua `FTS5`) để ra kết quả RAG tối ưu. Điểm số được tính bằng công thức:
  $$\text{RRF\_Score} = \frac{1}{60 + \text{Rank}_{\text{semantic}}} + \frac{1}{60 + \text{Rank}_{\text{keyword}}}$$
* **Positional Drill-down Index**: Mỗi vector được đính kèm trường `source_event_ids` (mã danh định L1). Khi tìm kiếm ở L2, hệ thống có thể truy xuất ngược lại toàn bộ raw turns nguyên bản tại L1 để bổ trợ ngữ cảnh chi tiết nhất.
* **Dead Letter Queue (DLQ)**: Mọi thao tác xóa vector thất bại sẽ được chuyển vào bảng `vector_dlq` để retry ngầm định kỳ (tối đa 3 lần) nhằm đảm bảo tính toàn vẹn và tuân thủ GDPR.

### L3: PersonalKnowledge
* **Vị trí**: CSDL SQLite (bảng `facts` & bảng đồ thị `l3_nodes`, `l3_edges`)
* **Chức năng**: Lưu trữ tri thức cốt lõi dài hạn (thói quen, sở thích, thông tin cá nhân được xác nhận) và mạng lưới thực thể.
* **Ebbinghaus Memory Decay (Hao mòn bộ nhớ)**: 
  * Áp dụng đường cong quên lãng Spaced Repetition. Mỗi lần ký ức được truy xuất, độ bền bộ nhớ (`memory_strength`) được đặt lại về `1.0` (thông qua touch buffer ngầm).
  * Trong quá trình củng cố dữ liệu, hệ thống tự động suy giảm độ bền ký ức theo công thức:
    $$S(t) = S_0 \times e^{-0.1 \times \text{days\_since\_access}}$$
  * Việc tính toán hàm số mũ $e^x$ được thực hiện trên CPU V8 (NodeJS) thay vì SQLite, chia nhỏ theo batch 500 dòng một và giải phóng Event Loop bằng `setImmediate` để chống hiện tượng nghẽn luồng chính. Ký ức mờ nhạt có độ bền < 0.1 sẽ bị xóa hoặc chuyển vào Cold Storage.
* **Dynamic Knowledge Graph**: Lưu trữ thực thể (`l3_nodes`) và các mối liên kết (`l3_edges`). Hỗ trợ tìm kiếm đa bước (Multi-hop search) sử dụng **Recursive CTE** của SQLite để truy tìm mối quan hệ bắc cầu giữa các khái niệm.

---

## 2. Động Cơ và Daemons Tự Động Vận Hành (Background Daemons)

Để hệ thống bộ nhớ tự động củng cố mà không ảnh hưởng (block) Event Loop của ứng dụng chat chính, hệ thống sử dụng các Daemons chạy nền:

### 1. ReflectionDaemon (Trích xuất ngầm Φ/Ψ)
* **Vận hành**: Chạy nền độc lập hoàn toàn, kích hoạt bằng cơ chế **Debounce 12 giây** sau khi AI phản hồi.
* **Chức năng**: Sử dụng Zod Dual Schema để gọi LLM bóc tách tin nhắn hội thoại thành các dữ kiện Φ Factual và trạng thái Ψ Relational rồi ghi vào bảng `events` (L1).

### 2. ConsolidationCron (Củng cố ký ức)
Tiến trình củng cố dữ liệu chạy lúc hệ thống Sleep (Sleep-time consolidation), khi Cold-start hoặc kích hoạt thủ công:
* **VRAM Guard**: Tự động chặn và trì hoãn Consolidation nếu trạng thái của AgentLoop không phải là `IDLE` (tránh tranh chấp tài nguyên VRAM khi LLM đang suy luận/streaming).
* **Battery Throttling (Energy-Aware)**: Đọc trạng thái từ `data/hardware_state.json`. Nếu thiết bị đang dùng Pin (`is_battery: true`), hệ thống tự động nâng ngưỡng kích hoạt Consolidation lên gấp 5 lần (từ 10 lên 50 sự kiện) để giảm thiểu tiêu thụ điện năng.
* **RAPTOR Tree (Tóm tắt đệ quy)**: Thực hiện tóm tắt các nút lá (leaf nodes) hội thoại L1 theo cụm 5 phần tử thành các summary node cấp cao hơn (L2 Anchors). Quá trình này lặp lại đệ quy để xây dựng một cây tóm tắt ngữ cảnh nhiều cấp phục vụ tìm kiếm đa tầng.
* **Reconsolidation Engine (Giải quyết xung đột dữ kiện)**: Quét các AXIOM mới, đối chiếu với tri thức cũ trong L2 để phân loại:
  * *Independent*: Sự kiện mới hoàn toàn $\rightarrow$ Insert.
  * *Extendable*: Sự kiện bổ sung $\rightarrow$ Hợp nhất dữ liệu thông qua LLM JSON synthesis.
  * *Contradictory*: Sự kiện mâu thuẫn $\rightarrow$ Xóa/ghi đè bản ghi cũ.
* **Dynamic Taxonomy Auto-Expansion**: Tự động nhận diện và gom cụm các tag phân loại chưa rõ nguồn gốc (`Unknown_*`). Chuẩn hóa ngữ nghĩa và nâng cấp chúng thành Domain chính thức nếu tần suất xuất hiện $\ge 3$ lần, hoặc dọn rác (GC) nếu chúng bị bỏ quên quá 7 ngày.
* **WAL Checkpoint & Atomic Snapshot**: Gọi lệnh `wal_checkpoint(PASSIVE)` dọn dẹp WAL ngầm không gây block database và tạo bản sao lưu snapshot bằng SQLite `VACUUM INTO`.

### 3. ContradictionResolver (Kiểm soát mâu thuẫn Đồ thị)
* **Vận hành**: Chạy ngầm khi củng cố L3 Graph.
* **Chức năng**: Tạo chuỗi mô tả tự nhiên cho mối liên kết mới, tìm kiếm các liên kết tương tự ở L2 (cosine similarity > 0.85). Sử dụng một model LLM siêu nhẹ kiểm tra xem mối quan hệ mới có mâu thuẫn trực tiếp với mối quan hệ cũ không. Nếu có, nó sẽ đánh dấu quan hệ cũ thành `obsolete = 1` trong `l3_edges`.

### 4. ArchivingCron (Dọn dẹp & Dump Cold Storage)
* **Vận hành**: Chạy định kỳ mỗi 24 giờ ngầm dưới dạng Low-priority task.
* **Chức năng**: Quét các Vector L2 có tuổi thọ > 30 ngày và ít truy cập (access_count $\le$ 2) hoặc bị mờ nhạt (decay_weight < 0.5).
* **Cold Storage**: LLM tóm tắt các ký ức cũ này thành khái niệm cốt lõi dài hạn, lưu vào L3 Graph dưới dạng `ArchiveNode` đính kèm liên kết file cold storage (ví dụ: `archive_2026_05.jsonl`). Sau đó, xóa toàn bộ vector ngữ cảnh chi tiết ở L2 và các event tương ứng tại L1, rồi chạy lệnh `VACUUM` giải phóng dung lượng đĩa cứng.

### 5. SemanticCache (Cache phản hồi nhanh)
* **Vận hành**: Cache RAM (LRU) lưu trữ tối đa 500 phần tử có tuổi thọ 24 giờ.
* **Chức năng**: Nhận các câu hỏi/câu lệnh ngắn ($\le$ 20 từ) và so khớp khoảng cách **Levenshtein** với các key trong cache. Nếu độ tương đồng $\ge 0.95$, hệ thống trả thẳng kết quả cache mà không cần chạy qua luồng định tuyến LLM, tối ưu hóa độ trễ về mức 0ms.

---

## 3. Kiến Trúc Decoupled CPU Embedding

* Nhằm loại bỏ hiện tượng nghẽn cổ chai VRAM và xung đột với cơ chế `VRAMGuard`, toàn bộ quá trình tạo Vector Embedding cho Memory và Routing được chuyển hoàn toàn sang **CPU ONNX Runtime (`onnxruntime-node`)** thông qua `EmbeddingWorker`.
* Sử dụng model nén siêu nhẹ dạng ONNX, đảm bảo **VRAM tiêu thụ bằng 0MB**, không ảnh hưởng đến card đồ họa GPU khi đang chơi game hoặc render, và cho tốc độ trích xuất vector ổn định.

---

## 4. An Toàn Lưu Trữ (Atomic Writes & Disk Shield)

* **Atomic File Write**: LIVA áp dụng nguyên tắc an toàn tuyệt đối. Mọi thao tác ghi tệp tin trạng thái hoặc cấu hình đều được ghi vào một file tạm `.tmp`, sau khi ghi thành công mới gọi hàm hệ thống `rename()` đè lên file chính thức. Phương pháp này bảo vệ dữ liệu không bao giờ bị hư hỏng (corrupt) nếu ứng dụng bị crash hoặc mất nguồn điện đột ngột giữa chừng.
* **WAL Checkpointing**: Chạy SQLite ở chế độ `PRAGMA journal_mode = WAL` kết hợp `synchronous = NORMAL` để đạt hiệu năng ghi đĩa tối đa mà vẫn an toàn. Nhật ký WAL được kiểm soát checkpoint thụ động thường xuyên để tránh phình tệp dữ liệu.
* **Safe Connection Lifespan**: Khi đóng ứng dụng, Core Kernel sẽ gọi `MemoryManager.dispose()`, đảm bảo xả hết hàng đợi Touch Queue và Reflection queue trước khi đóng kết nối SQLite thông qua native `db.close()`.
