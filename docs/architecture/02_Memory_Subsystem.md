# 02. Hệ Thống Bộ Nhớ H-MEM v18 (Memory Subsystem)

**Phiên bản: v26 Enterprise-Ready Cognitive OS**

Hệ thống bộ nhớ của LIVA đã tiến hoá thành kiến trúc **H-MEM v18 (HiGMem Phase 3)**. Thay vì sử dụng các cơ sở dữ liệu cồng kềnh (LanceDB) và công cụ tìm kiếm RAM-heavy (flexsearch), hệ thống được gom gọn hoàn toàn vào **1 file `node:sqlite` duy nhất** với sự hỗ trợ của các phần mở rộng C-Extension (`sqlite-vec` và `FTS5`).

## 1. Kiến Trúc 4 Tầng Bậc (Memory Tiers: L0 - L3)

### L0: TurboQuantStore & WorkingBuffer
- **Vị trí**: RAM & VRAM
- **Chức năng**: Vùng nhớ làm việc tức thời (Working Memory). Xử lý context ngắn hạn đang diễn ra trong cuộc trò chuyện (Episode) hiện tại.
- L0 tự động "Flush" vào bộ nhớ vĩnh viễn (L1) ngay khi một Cuộc hội thoại được đóng lại (Episode Boundary).

### L1: EventRepository (Turn Layer)
- **Vị trí**: `StructuredMemory.sqlite` (Table `turn_layer_nodes`)
- **Chức năng**: Lưu trữ trực tiếp các câu chat raw (Conversational Turns).
- Hoạt động với cơ chế quản lý vòng đời TTL, thu gom rác Garbage Collection (GC) thông qua `RamCacheManager` (đã được abstract hoá) và ghi bằng kỹ thuật "Debounced Writes".

### L2: VectorRepository (Event Layer)
- **Vị trí**: `StructuredMemory.sqlite` (Table vector qua `sqlite-vec`)
- **Chức năng**: Chứa các "Câu chuyện" (Narratives) đã được củng cố. Các `AXIOM` (Chân lý/Sự kiện) và `ANCHOR` (Mốc thời gian).
- Phục vụ quá trình RAG (Retrieval-Augmented Generation) thông qua tìm kiếm Cosine Similarity tại `SemanticRouter`.

### L3: PersonalKnowledge
- **Vị trí**: `StructuredMemory.sqlite` (Key-Value Store / FTS5)
- **Chức năng**: Lưu trữ các Core Insights (thói quen, sở thích, tên người dùng, config cá nhân). Đây là vùng nhớ chiến lược dài hạn ít bị thay đổi.

## 2. Daemons & Động Cơ Bất Đồng Bộ

Để hệ thống bộ nhớ tự động vận hành mà không ảnh hưởng (block) Event Loop của ứng dụng chat chính, hệ thống sử dụng các Daemons chạy nền:

### DualChannelSegmenter
- **Chức năng**: Topic-Aware Episode Boundary Detection. Khác biệt với việc cắt logic cơ học theo số Turn, LIVA có thể nhận biết lúc nào User đổi chủ đề.
- **Channel 1 (Zero-Cost Math)**: Tính toán sự khác biệt của Vector (`Cosine similarity < 0.65`) để dò Topic Shift.
- **Channel 2 (LLM Judge)**: Nếu hàm *First-Pass Filter* (nhận diện Entity/Capitalized words) phát hiện có khái niệm mới, LIVA bỏ ra ~300 tokens gọi LLM Judge xem câu hỏi mới có "Gây ngạc nhiên" (Surprise) không.
- Nếu phát hiện New Episode, hệ thống sẽ chốt chặn WorkingBuffer và gởi dữ liệu qua Engine Củng cố.

### ReconsolidationEngine
- **Chức năng**: Conflict-Aware Memory Reconsolidation.
- Engine quét các `AXIOM` vừa được sinh ra và đối chiếu với dữ liệu dài hạn cũ (L2/L3).
- Thực thi phân loại (Classify) 3 trạng thái:
  - `Independent`: Sự kiện hoàn toàn mới → Thêm mới.
  - `Extendable`: Sự kiện thêm thông tin → Kết hợp (Synthesize) cái cũ và mới thành sự kiện chi tiết hơn (Dùng LLM JSON output).
  - `Contradictory`: Sự kiện mâu thuẫn (Ví dụ: "Tôi đã chuyển nhà từ HN sang SG") → Xoá vector cũ, ghi đè vector mới.
- **Hardware-Aware Throttling**: Tính năng độc quyền đọc trực tiếp lệnh `nvidia-smi` để biết GPU Load và VRAM trống. Nếu hệ thống đang quá tải, Engine tự động giảm Batch size củng cố từ 50 xuống 5.

### ConsolidationCron & ReflectionDaemon
- **ReflectionDaemon**: Chạy nền (Debounce 12s), bóc tách hệ quy chiếu kép (Φ Factual - Dữ kiện thực tế, Ψ Relational - Quan hệ cảm xúc) theo cấu trúc Zod Dual Schema.
- **ConsolidationCron**: Chạy khi máy tính rơi vào trạng thái Idle (30 phút không dùng) hoặc Cold-start. Chịu trách nhiệm tổng hợp L1 thành L2.

## 3. Kiến Trúc Decoupled CPU Embedding
- Trước đây, việc tạo Vector Embedding dùng chung `llama-server` (GPU) gây thắt cổ chai VRAM và vướng cơ chế `VRAMGuard`.
- Hiện tại, quá trình tạo vector (cả hệ thống Memory và Routing) chuyển hoàn toàn cho `EmbeddingWorker` sử dụng **CPU ONNX runtime (`onnxruntime-node`)**.
- Lợi ích: VRAM bằng 0, không ảnh hưởng LLM, tốc độ cực kỳ ổn định.

## 4. An Toàn Lưu Trữ (Atomic & Shield)
- Tuyệt đối áp dụng pattern **Atomic Write**: Tạo file `.tmp` và `rename()` để chống tham nhũng (corrupt) database khi crash app giữa chừng. Bỏ `fs.cpSync` khi backup.
- **WriteValidationGate**: Bảo vệ toàn vẹn dữ liệu ghi.
- Hệ thống luôn chạy `node:sqlite` với chế độ `PRAGMA journal_mode = WAL` và `synchronous = NORMAL`.
