---
title: "Tài liệu Kỹ thuật Crate: liva-llm"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-llm/Cargo.toml
  - crates/liva-llm/src/actor.rs
  - crates/liva-llm/src/lib.rs
---

# Tài liệu Kỹ thuật Crate: liva-llm

## 1. Tổng quan và Mục đích Thiết kế

`crates/liva-llm` cài đặt động cơ suy luận ngôn ngữ dạng **Actor-based LLM Engine** độc lập cho LIVA. Mục tiêu hàng đầu của crate này là:
- Đảm bảo các tác vụ sinh văn bản nặng không bao giờ làm nghẽn luồng xử lý bất đồng bộ của Tokio hoặc làm đứng giao diện máy tính.
- Cung cấp cơ chế hàng đợi ưu tiên 3 cấp độ (`High`, `Normal`, `Low`).
- Cho phép ngắt sớm (preemption & early cancellation) tác vụ sinh văn bản đang chạy khi có lệnh ưu tiên cao hơn (ví dụ: người dùng nói chen ngang hoặc nhấn nút hủy).

---

## 2. Kiến trúc Actor và Hàng đợi Ưu tiên (`actor.rs`)

### 2.1. Phân cấp Độ ưu tiên (`Priority`)
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
}
```
- **`High`**: Lệnh ngắt khẩn cấp, tín hiệu dừng hệ thống (`Shutdown`), hoặc yêu cầu phản xạ lập tức khi phát hiện nguy cơ an toàn.
- **`Normal`**: Lượt đối thoại tương tác trực tiếp với người dùng qua giọng nói hoặc giao diện chat.
- **`Low`**: Tác vụ chạy nền (tổng kết nhật ký, trích xuất thực thể cho đồ thị tri thức, suy luận phân kỳ kiểm tra kỹ năng).

### 2.2. Cơ chế Xếp hàng và Thoát trước (Preemption)
Trong `LlmActor::run`:
1. Mỗi vòng lặp, Actor chủ động rút sạch toàn bộ thông điệp đang chờ trong `receiver.try_recv()` và phân loại vào 3 hàng đợi `VecDeque<LlmCommand>` tương ứng (`high`, `normal`, `low`).
2. Lệnh có mức ưu tiên cao nhất luôn được lấy ra thực thi trước.
3. Khi bắt đầu sinh văn bản, tác vụ tính toán nặng của `llama.cpp` được chuyển sang luồng OS chuyên biệt thông qua `tokio::task::spawn_blocking`.
4. Trong lúc luồng OS đang chạy suy luận, luồng điều phối của Actor tiếp tục giám sát kênh `receiver.recv()` bằng cấu trúc `tokio::select!`.
5. Nếu xuất hiện lệnh mới có mức ưu tiên cao hơn hoặc lệnh `Shutdown`, Actor ngay lập tức kích hoạt:
   ```rust
   cancel_token.store(true, Ordering::SeqCst);
   ```
   Hàm callback sinh token bên trong luồng C/Rust kiểm tra cờ này sau mỗi token và dừng sinh ngay lập tức, giải phóng tài nguyên GPU/CPU mà không cần chờ chạy hết context.

---

## 3. Giao diện Công khai `LlmActorHandle`

Người dùng và các hệ thống con tương tác với LLM thông qua handle luồng an toàn (`Clone + Send + Sync`):

```rust
impl LlmActorHandle {
    pub async fn generate_text(
        &self,
        prompt: String,
        priority: Priority,
    ) -> anyhow::Result<String>;

    pub async fn generate_text_stream(
        &self,
        prompt: String,
        priority: Priority,
        token_tx: mpsc::Sender<String>,
    ) -> anyhow::Result<String>;

    pub async fn generate_text_stream_cancellable(
        &self,
        prompt: String,
        priority: Priority,
        token_tx: mpsc::Sender<String>,
        cancel_token: Arc<AtomicBool>,
    ) -> anyhow::Result<String>;

    pub async fn shutdown(&self) -> anyhow::Result<()>;
}
```

---

## 4. Tích hợp Định tuyến và Phân bổ Ngân sách Token

Phối hợp với các module hỗ trợ trong `liva-native-core/src/llm/`:
- **Cân đối Ngân sách Prompt (Dynamic Prompt Budgeting)**: Tự động tính toán số token khả dụng của cửa sổ ngữ cảnh (ví dụ: 4096 hoặc 8192 token), phân bổ tỷ lệ hợp lý giữa:
  - System Prompt & Hướng dẫn kỹ năng (20%).
  - Lịch sử hội thoại gần nhất (30%).
  - Đoạn trích bộ nhớ RAG / HippoRAG (35%).
  - Ngân sách dự trữ cho câu trả lời sinh ra (15%).
- **Kiểm định Mô hình và Bộ nhớ đệm Đáng tin cậy (`Trust Cache`)**: Băm SHA-256 các tệp trọng số GGUF/ONNX trước khi nạp vào bộ nhớ để chống can thiệp tệp độc hại ngoài ý muốn.
