---
title: "Tài liệu Kỹ thuật Crate: liva-tools"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - crates/liva-tools/Cargo.toml
  - crates/liva-tools/src/main.rs
---

# Tài liệu Kỹ thuật Crate: liva-tools

## 1. Tổng quan và Mục đích Hợp nhất

Trước đây, LIVA tồn tại 26 tệp nhị phân con rải rác (`src/bin/*.rs`) để phục vụ việc thăm dò từng tính năng đơn lẻ (như `db_probe`, `voice_bench`, `llm_bench`, `wake_benchmark`). Việc phân mảnh này làm tăng thời gian biên dịch mã nguồn và khó khăn trong việc bảo trì công cụ kiểm thử.

Crate `crates/liva-tools` ra đời nhằm **hợp nhất toàn bộ tiện ích chẩn đoán, đo lường hiệu năng và kiểm tra sức khỏe hệ thống** vào một tệp thực thi duy nhất (`liva-tools`) được phân cấp lệnh rõ ràng bằng thư viện `clap`.

---

## 2. Các Nhóm Lệnh Chính

Cấu trúc lệnh của `liva-tools` được chia thành 4 nhóm nghiệp vụ chính:

```
liva-tools [COMMAND]
├── doctor        # Bác sĩ hệ thống: kiểm định môi trường, phần cứng, models
├── probe         # Thăm dò chẩn đoán từng thành phần con
│   ├── db        # Kiểm tra SQLite WAL, tính toàn vẹn 20 bảng và FFI
│   ├── onnx      # Kiểm tra tensor contract của các mô hình ONNX
│   ├── wakeword  # Thử nghiệm kích hoạt từ khóa đánh thức
│   ├── gtcrn     # Đo độ suy giảm tạp âm của bộ lọc âm thanh
│   ├── stt       # Nhận dạng chuỗi âm thanh streaming
│   ├── tts       # Tổng hợp âm thanh mẫu và trích xuất viseme
│   ├── router    # Phân loại định tuyến câu lệnh RouteLLM
│   └── os        # Kiểm tra thông tin hệ điều hành, âm lượng loa/mic
├── bench         # Bộ benchmark định lượng độ trễ và thông lượng
└── eval          # Đánh giá độ chính xác gọi công cụ và ghi nhớ tri thức
```

---

## 3. Hướng dẫn Sử dụng Chi tiết

### 3.1. Chạy Bác sĩ Hệ thống (`doctor`)
Kiểm tra tổng thể xem máy tính đã đáp ứng đủ điều kiện phần cứng và có đầy đủ các tệp mô hình hay chưa:
```powershell
cargo run -p liva-tools -- doctor
```
Lệnh này xác minh:
- Trạng thái trình điều khiển NVIDIA CUDA và dung lượng VRAM khả dụng.
- Sự hiện diện và tính hợp lệ SHA-256 của các mô hình trong thư mục `models/`.
- Quyền truy cập cơ sở dữ liệu và thư mục tạm.

### 3.2. Thăm dò Cơ sở Dữ liệu (`probe db`)
Kiểm tra tính toàn vẹn của tệp cơ sở dữ liệu SQLite, kích thước nhật ký WAL và bảng ảo `sqlite-vec`:
```powershell
cargo run -p liva-tools -- probe db
```
Kết quả hiển thị thông số: số lượng bản ghi trong từng bảng, kích thước tệp đĩa, và trạng thái nạp bảng ảo vector `vec_idx`.

### 3.3. Đo kiểm Hiệu năng Tổng thể (`bench`)
Đo đạc độ trễ và thông lượng thực tế trên phần cứng hiện tại:
```powershell
cargo run -p liva-tools -- bench
```
Bao gồm:
- **Độ trễ phản hồi âm thanh**: Đo đạc độ trễ chuyển đổi âm thanh của chuỗi AEC $\to$ GTCRN $\to$ Silero VAD $\to$ Smart Turn.
- **Độ trễ suy luận LLM**: Đo thời gian sinh token đầu tiên (TTFT - Time To First Token) và tốc độ sinh (tokens/giây).
- **Thông lượng ghi SQLite**: Đo số thao tác ghi hoàn tất mỗi giây thông qua `DbActor`.
