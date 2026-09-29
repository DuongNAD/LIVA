---
title: "Quy trình Sao lưu và Sổ tay Xử lý Sự cố"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - liva-native-core/src/persistence_backup.rs
---

# Quy trình Sao lưu và Sổ tay Xử lý Sự cố

## 1. Quy trình Sao lưu Cơ sở Dữ liệu Trực tuyến (Online SQLite Backup)

Trong kiến trúc SQLite WAL của LIVA, việc sao chép tệp cơ sở dữ liệu `liva.db` trực tiếp bằng các lệnh sao chép tệp thông thường khi ứng dụng đang chạy có thể dẫn đến việc bản sao bị hỏng do dữ liệu mới đang nằm trong tệp nhật ký `-wal`.

LIVA cung cấp cơ chế sao lưu trực tuyến an toàn thông qua module `liva-native-core/src/persistence_backup.rs`:
- Sử dụng trực tiếp API `sqlite3_backup_*` của SQLite để sao chép từng khối trang bộ nhớ từ tệp đang mở sang tệp đích độc lập.
- Quá trình sao lưu hoàn toàn không khóa chặn các luồng đang đọc hay ghi dữ liệu của `DbActor`.
- Sau khi hoàn tất sao lưu, hệ thống tự động sinh tệp manifest (ví dụ `backup_manifest.json`) chứa mã băm SHA-256 và mốc thời gian để phục vụ việc kiểm tra tính toàn vẹn.

---

## 2. Quy trình Phục hồi Dữ liệu (Disaster Recovery)

Khi cần phục hồi dữ liệu từ bản sao lưu:
1. **Dừng tiến trình LIVA**: Đóng ứng dụng desktop và đảm bảo không còn tiến trình `liva-desktop.exe` hay `liva-native-core.exe` chạy ngầm.
2. **Kiểm tra tính toàn vẹn**: So sánh mã băm SHA-256 của tệp sao lưu với bản ghi trong tệp manifest đi kèm.
3. **Thay thế tệp cơ sở dữ liệu**:
   - Sao lưu tệp hiện tại phòng ngừa: đổi tên `liva.db` thành `liva.db.old`.
   - Xóa các tệp phụ nếu có: `liva.db-wal` và `liva.db-shm`.
   - Đặt tệp sao lưu vào vị trí `liva.db`.
4. **Cung cấp khóa giải mã**: Đảm bảo biến môi trường `$env:LIVA_ENCRYPTION_KEY` trùng khớp với khóa đã dùng tại thời điểm tạo bản sao lưu.
5. **Khởi động và kiểm tra**: Chạy lệnh `cargo run -p liva-tools -- probe db` để xác nhận cấu trúc 20 bảng cơ sở dữ liệu và bảng ảo vector hoạt động bình thường.

---

## 3. Sổ tay Xử lý Sự cố Thường gặp (Troubleshooting Runbook)

### 3.1. Sự cố: Bộ nhớ RAM hoặc VRAM vượt ngưỡng an toàn (> 4.0 GB RAM / > 5.1 GB VRAM)
- **Dấu hiệu**: Hệ thống có hiện tượng giật lag, quạt tản nhiệt GPU quay tối đa, hoặc tiến trình bị hệ điều hành tắt đột ngột do thiếu bộ nhớ (Out-Of-Memory).
- **Nguyên nhân**:
  1. Đặt số layer offload GPU `LIVA_LLM_N_GPU_LAYERS` quá lớn so với dung lượng VRAM vật lý còn trống.
  2. Kích thước ngữ cảnh `LIVA_LLM_N_CTX` đặt mức quá cao (ví dụ: 16k hoặc 32k) làm phình KV cache.
  3. Mô hình thị giác Qwen3-VL không tự động xả VRAM sau khi phân tích màn hình.
- **Biện pháp khắc phục**:
  1. Giảm số layer GPU xuống mức an toàn (ví dụ: đặt `$env:LIVA_LLM_N_GPU_LAYERS = "24"`).
  2. Khống chế cửa sổ ngữ cảnh ở mức `$env:LIVA_LLM_N_CTX = "4096"`.
  3. Kiểm tra biến điều phối `VisualGovernor`: đảm bảo thời gian chờ tắt VLM (cooldown) là 15 giây.

### 3.2. Sự cố: Tranh chấp khóa cơ sở dữ liệu `SQLITE_BUSY`
- **Dấu hiệu**: Nhật ký hệ thống ghi nhận lỗi `database is locked` hoặc `code 5: SQLITE_BUSY`.
- **Nguyên nhân**: Một công cụ ngoại vi (như DB Browser for SQLite hoặc một script bên ngoài) đang mở tệp `liva.db` ở chế độ ghi độc quyền.
- **Biện pháp khắc phục**:
  1. Đóng toàn bộ các phần mềm xem cơ sở dữ liệu bên ngoài đang mở tệp `liva.db`.
  2. Đảm bảo mọi thao tác ghi trong mã nguồn đều được gửi qua kênh `DbActorHandle` thay vì mở kết nối ghi độc lập.
  3. Chạy lệnh checkpoint dọn sạch nhật ký WAL:
     ```powershell
     cargo run -p liva-tools -- probe db
     ```

### 3.3. Sự cố: Không nhận diện được giọng nói hoặc Micro không phản hồi
- **Dấu hiệu**: Trợ lý không kích hoạt khi nói "LIVA ơi" hoặc không hiển thị transcript âm thanh.
- **Nguyên nhân**: Windows chưa cấp quyền truy cập Microphone cho ứng dụng hoặc chọn sai thiết bị thu âm mặc định.
- **Biện pháp khắc phục**:
  1. Mở Windows Settings $\to$ **Privacy & Security** $\to$ **Microphone**, kiểm tra xem quyền truy cập cho ứng dụng desktop đã được bật hay chưa.
  2. Chạy công cụ kiểm tra âm thanh trực tiếp:
     ```powershell
     cargo run -p liva-tools -- probe stt
     ```
  3. Nói vào microphone và quan sát xem các gói âm thanh có được ghi nhận với năng lượng RMS $> 0$ hay không.
