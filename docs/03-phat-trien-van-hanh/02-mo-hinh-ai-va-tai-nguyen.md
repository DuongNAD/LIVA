---
title: "Quản lý Mô hình AI và Kiểm định Tài nguyên"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - scripts/models.mjs
---

# Quản lý Mô hình AI và Kiểm định Tài nguyên

## 1. Phân loại Mô hình AI trong Hệ thống LIVA

LIVA sử dụng sự kết hợp giữa các mô hình ngôn ngữ lớn (GGUF thông qua `llama.cpp`) và các mô hình xử lý tín hiệu chuyên biệt gọn nhẹ (ONNX Runtime):

```
┌─────────────────────────────────────────────────────────────┐
│                     HỆ THỐNG MÔ HÌNH AI                     │
├─────────────────────────────────────────────────────────────┤
│ 1. MÔ HÌNH NGÔN NGỮ LỚN & THỊ GIÁC (GGUF / llama.cpp)       │
│    - Chat / Lập luận: Qwen2.5-7B-Instruct (Q4_K_M, ~4.5 GB) │
│    - Thị giác máy tính: Qwen3-VL-2B (Q4_K_M, ~1.5 GB)       │
│    - Lưu trữ tại: Thư mục mô hình ngoài (ví dụ E:\AI_Models) │
├─────────────────────────────────────────────────────────────┤
│ 2. MÔ HÌNH XỬ LÝ TÍN HIỆU & EMBEDDING (ONNX Runtime)        │
│    - Nhúng vector ngữ nghĩa: multilingual-e5-small (int8)   │
│    - Phát hiện giọng nói: Silero VAD v5                     │
│    - Khử nhiễu môi trường: GTCRN Speech Denoiser           │
│    - Phân loại ngắt lượt: Smart Turn v3.2 Classifier       │
│    - Nhận dạng giọng nói: Parakeet TDT / CTC Streaming     │
│    - Tổng hợp âm thanh: VieNeu / Piper TTS tiếng Việt       │
│    - Lưu trữ tại: Thư mục models/ trong dự án               │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Quy trình Tải và Cài đặt Mô hình (`setup:models`)

Để thiết lập đầy đủ các mô hình ONNX cơ sở:
```powershell
npm run setup:models
```
Lệnh trên thực thi `node scripts/models.mjs fetch`:
- Tự động kết nối tới kho lưu trữ chính thức của dự án.
- Tải về các tệp mô hình ONNX vào đúng vị trí trong thư mục `models/`.
- Kiểm tra tính toàn vẹn của tệp ngay sau khi tải.

---

## 3. Kiểm định Sức khỏe và Bác sĩ Mô hình (`doctor`)

Sau khi tải, chạy công cụ bác sĩ để xác nhận tính toàn vẹn của toàn bộ tài nguyên:
```powershell
# Chạy bác sĩ kiểm tra qua kịch bản Node.js
npm run doctor

# Hoặc chạy kiểm định toàn diện từ mã nguồn Rust
cargo run -p liva-tools -- doctor
```

Kết quả trả về bảng trạng thái cho từng thành phần:
- Kích thước tệp đĩa.
- Tính khả dụng của GPU CUDA Execution Provider trên máy tính.
- Độ trễ khởi động nạp phiên ONNX (Session Creation Time).

---

## 4. Cơ chế Xác thực Tin cậy SHA-256 (Trust Verification)

Nhằm ngăn chặn việc nạp các tệp trọng số bị hỏng trong quá trình tải hoặc bị giả mạo:
1. Mỗi mô hình được gắn kèm một mã băm SHA-256 cố định trong danh mục `scripts/models.mjs` và bộ nhớ đệm `Trust Cache`.
2. Khi khởi động hệ thống, hàm `verify_model_integrity` tính toán mã băm SHA-256 của tệp trên đĩa cứng và so sánh với mã gốc.
3. Nếu mã băm không trùng khớp, hệ thống từ chối nạp mô hình vào VRAM, ghi log lỗi nghiêm trọng và yêu cầu người dùng tải lại tệp.
