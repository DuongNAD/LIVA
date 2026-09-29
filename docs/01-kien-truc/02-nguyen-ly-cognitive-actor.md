---
title: "Nguyên lý Vận hành Động cơ Cognitive Actor"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-llm/src/actor.rs
  - crates/liva-storage/src/lib.rs
  - liva-native-core/src/agent/graph.rs
---

# Nguyên lý Vận hành Động cơ Cognitive Actor

## 1. Mô hình Đồng quy Native Actor (Actor Concurrency)

LIVA giải quyết bài toán đồng quy trong hệ thống AI cục bộ thông qua mô hình **Actor Concurrency** dựa trên kênh truyền thông điệp có giới hạn (bounded MPSC channel). Thiết kế này triệt tiêu hoàn toàn hiện tượng nghẽn luồng giao diện người dùng và tình trạng xung đột khóa (lock contention) trên các tài nguyên tính toán nặng.

Hai Actor then chốt của hệ thống:
1. **`LlmActor` (`crates/liva-llm`)**:
   - Vận hành vòng lặp xử lý thông điệp riêng biệt trên luồng Tokio bất đồng bộ.
   - Quản lý hàng đợi ưu tiên 3 cấp: `High` (ngắt lượt, hủy lệnh khẩn cấp, shutdown), `Normal` (hội thoại trực tiếp với người dùng), `Low` (tổng hợp nền, suy luận phân kỳ, hợp nhất bộ nhớ).
   - Tách biệt công việc tính toán nặng sang luồng OS chuyên biệt thông qua `tokio::task::spawn_blocking`, đồng thời duy trì biến cờ nguyên tử `cancel: Arc<AtomicBool>` để chủ động ngắt tiến trình sinh token ngay khi có lệnh có độ ưu tiên cao hơn gửi tới.
2. **`DbActor` (`crates/liva-storage`)**:
   - Pinned vào một luồng hệ điều hành độc lập dành riêng cho các thao tác ghi dữ liệu.
   - Nhận các lệnh ghi qua bounded MPSC channel (dung lượng 1024), áp dụng cơ chế vi lô (micro-batching) tối đa 50 thao tác hoặc 5ms.
   - Thực thi lô trong một giao dịch đơn nhất `BEGIN IMMEDIATE; ... COMMIT;`, hoàn toàn loại bỏ lỗi khóa tranh chấp `SQLITE_BUSY` của SQLite WAL.

---

## 2. Chu trình Nhận thức 6 Bước (Perception-to-Action Loop)

Mỗi lượt tương tác của người dùng (bằng văn bản, giọng nói hoặc hình ảnh) đều đi qua chu trình nhận thức chuẩn hóa gồm 6 bước:

```
┌────────────────────────────────────────────────────────────────────────┐
│                      CHU TRÌNH NHẬN THỨC CỦA LIVA                      │
├────────────────────────────────────────────────────────────────────────┤
│ 1. PERCEPTION (Thu nhận & Tiền xử lý đa giác quan)                     │
│    - Giọng nói: Silero VAD (200ms) + Smart Turn v3.2 ngắt lượt         │
│    - Màn hình: WGC / BitBlt chụp khung hình DPI-aware + diff vùng ROI │
│    - Lệnh văn bản: Phân tích cú pháp và kiểm tra định danh người gọi   │
├────────────────────────────────────────────────────────────────────────┤
│ 2. ACTIVE RECALL INTERCEPTION (Đánh chặn nhớ chủ động)                 │
│    - Tra cứu trượt trên cây tiền tố Radix Trie (< 0.1 ms)              │
│    - Khớp sự kiện/thách thức: Trả lời tức thì với chi phí 0 token LLM  │
├────────────────────────────────────────────────────────────────────────┤
│ 3. CONTEXT ENRICHMENT (Làm giàu ngữ cảnh đa tầng)                      │
│    - Truy xuất kết hợp: Scoped Vector ANN + FTS5 Full-Text Search      │
│    - Lan truyền đồ thị HippoRAG PPR (Personalized PageRank)            │
│    - Cắt tỉa ngân sách ngữ cảnh (Dynamic Token Budgeting)              │
├────────────────────────────────────────────────────────────────────────┤
│ 4. ROUTER: REFLEX vs. DELIBERATIVE (Định tuyến phản xạ & lập luận)     │
│    - Phản xạ nhanh (Reflex): Ánh xạ mẫu lệnh hệ thống trực tiếp        │
│    - Lập luận sâu (Deliberative): StateGraph kích hoạt LLM Actor       │
├────────────────────────────────────────────────────────────────────────┤
│ 5. POLICY & CONSENT (Chính sách an toàn & Xác nhận hai pha)            │
│    - Kiểm tra thẩm quyền gọi lệnh (Principal & Action Policy)          │
│    - Can thiệp nhạy cảm (CUA/FS/Web): Tạm dừng chờ duyệt HITL         │
├────────────────────────────────────────────────────────────────────────┤
│ 6. EXECUTION & CONSOLIDATION (Thực thi và Củng cố bộ nhớ)              │
│    - Thực thi công cụ (MCP Tool / Win32 Synthetic Input)               │
│    - Đẩy kết quả hội thoại về DbActor để lưu trữ và suy giảm Ebbinghaus│
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Cổng Ngắt lượt Thích ứng (Adaptive Turn Taking)

Trong giao tiếp giọng nói hai chiều thời gian thực (Audio Duplex), việc xác định đúng thời điểm người dùng kết thúc câu nói là yếu tố quyết định để tránh trợ lý nói chen ngang khi người dùng chỉ đang ngập ngừng suy nghĩ:

1. **Bộ phân loại `Smart Turn v3.2`**:
   - Kết hợp năng lượng âm thanh Silero VAD với mô hình ngôn ngữ mini xác định xác suất kết thúc câu $p$.
2. **Quyết định 3 trạng thái (`AdaptiveTurnDecision`)**:
   - `ImmediateCutoff` ($p > 0.92$): Người dùng đã dứt câu hoàn toàn. Ngắt lượt ngay sau khoảng lặng chuẩn $T_{\text{gate}} \le 225\text{ ms}$ (200ms VAD silence + 12ms ONNX inference).
   - `HesitationWait` ($0.50 \le p \le 0.92$): Người dùng đang ngập ngừng (ví dụ: "Tôi muốn tìm... ờ... tài liệu..."). Hệ thống tự động kéo giãn khoảng đệm chờ lên $200 - 450\text{ ms}$ để cho phép người dùng nói tiếp.
   - `Incomplete` ($p < 0.50$): Câu nói chưa trọn vẹn, tiếp tục tích lũy khung âm thanh mà không ngắt lượt.
3. **Prefill token sớm (`speculative_eval`)**:
   - Khi bộ nhận diện giọng nói (STT) tích lũy được đoạn văn bản tại mốc 140ms, hệ thống có thể chuyển tiếp trước đoạn trích này sang SLM để khởi động nạp KV cache ngầm, rút ngắn đáng kể thời gian sinh token đầu tiên (TTFT).

---

## 4. Ranh giới Can thiệp Con người (Human-in-the-Loop - HITL)

LIVA phân loại mọi hành động thành ba cấp độ kiểm soát rủi ro:
- **Tier 1 (Đọc - Không rủi ro)**: Đọc thông tin thời tiết, tra cứu đồ thị tri thức, tìm kiếm vector, kiểm tra trạng thái hệ thống. Được thực thi tự động lập tức.
- **Tier 2 (Ghi nội bộ - Rủi ro thấp)**: Lưu trữ ghi chú cá nhân, đánh dấu sự kiện lịch, cập nhật tri thức mới vào SQLite. Thực thi tự động và ghi nhận sổ cái kiểm toán.
- **Tier 3 (Tác động ngoại vi - Rủi ro cao)**: Thao tác gửi tin nhắn Telegram/Messenger, xóa tệp tin trên ổ cứng, thực thi script hệ thống hoặc chạy macro chuột bàn phím (CUA). Bắt buộc phải kích hoạt cơ chế xác nhận hai pha (Two-Phase Confirmation), tạo popup xác nhận trên khay giao diện desktop và chờ người dùng phê duyệt rõ ràng.
