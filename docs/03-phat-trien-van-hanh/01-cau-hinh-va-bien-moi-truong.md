---
title: "Cấu hình Hệ thống và Danh mục Biến Môi trường"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - .env.example
  - liva-native-core/src/paths.rs
---

# Cấu hình Hệ thống và Danh mục Biến Môi trường

## 1. Nguyên tắc Nạp Cấu hình trong LIVA

- **Không nạp tệp `.env` động lúc chạy (Runtime No-DotEnv)**: Nhằm đảm bảo an toàn tuyệt đối và tránh rò rỉ khóa bí mật, tiến trình nhị phân Rust không tự ý quét hay nạp tệp `.env` vào môi trường runtime.
- **Biến môi trường hệ thống**: Các biến môi trường được truyền trực tiếp từ tiến trình cha hoặc thiết lập qua PowerShell (`$env:LIVA_* = "..."`).
- **Tệp cấu hình tĩnh (`liva-config.json`)**: Các tùy chọn của người dùng (như thư mục chứa model AI, cấu hình giao diện) được lưu trong `%APPDATA%/LIVA/data/liva-config.json`.

---

## 2. Danh mục Biến Môi trường Thường dùng

### 2.1. Bảo mật và Mã hóa (Security & Cryptography)

| Tên biến | Kiểu giá trị | Mặc định | Ý nghĩa & Hướng dẫn |
|---|---|---|---|
| `LIVA_ENCRYPTION_KEY` | Hex 32-byte | Tự sinh (DPAPI) | Khóa mã hóa chính cho dữ liệu cá nhân trong SQLite. Nếu để trống, LIVA tự sinh khóa ngẫu nhiên và bảo vệ bằng Windows DPAPI (`.device_key`). Đặt biến này khi cần khôi phục dữ liệu trên máy tính mới. |
| `LIVA_STRONGHOLD_PASSWORD`| Chuỗi ký tự | Tự sinh (DPAPI) | Mật khẩu giải mã kho khóa Tauri Stronghold chứa API keys bên ngoài. |
| `LIVA_STRONGHOLD_SALT` | Chuỗi ký tự | Nhãn mặc định | Chuỗi muối kết hợp với Argon2id để dẫn xuất khóa giải mã Stronghold. |

### 2.2. Cơ sở Dữ liệu SQLite WAL (`liva-storage`)

| Tên biến | Kiểu giá trị | Mặc định | Ý nghĩa & Hướng dẫn |
|---|---|---|---|
| `LIVA_DB_PATH` | Đường dẫn tệp | `%APPDATA%/LIVA/data/liva.db` | Vị trí lưu tệp SQLite chính. Trong môi trường dev, có thể trỏ tới `data/liva.db`. |
| `LIVA_DB_IN_MEMORY` | `true` / `false` | `false` | Khi đặt là `true`, cơ sở dữ liệu sẽ chạy hoàn toàn trên RAM (thường dùng cho test CI). |
| `LIVA_DB_READERS` | Số nguyên | `4` | Số lượng kết nối đọc đồng thời trong nhóm kết nối SQLite WAL pool. |
| `LIVA_MEMORY_RETENTION_DAYS` | Số nguyên (ngày) | `0` (Tắt) | Số ngày lưu giữ lịch sử hội thoại trước khi dọn dẹp nền theo đường cong Ebbinghaus. |

### 2.3. Động cơ Ngôn ngữ và Phần cứng GPU/CPU (`liva-llm`)

| Tên biến | Kiểu giá trị | Mặc định | Ý nghĩa & Hướng dẫn |
|---|---|---|---|
| `LIVA_LLM_N_GPU_LAYERS` | Số nguyên | Tự tính toán | Số lớp mạng neural được đưa lên VRAM GPU. Trên GPU 6GB, khuyến nghị đặt từ `24` đến `32` lớp để vừa vặn trong ngân sách $\le 5.1\text{ GB}$ VRAM. |
| `LIVA_LLM_N_CTX` | Số nguyên | `4096` | Kích thước cửa sổ ngữ cảnh (Context Window). Có thể nâng lên `8192` nếu GPU có dung lượng VRAM lớn hơn. |
| `LIVA_LLM_THREADS` | Số nguyên | Số core vật lý / 2 | Số luồng tính toán CPU dành cho suy luận LLM khi không offload hết lên GPU. |
| `LIVA_TOKIO_WORKER_THREADS`| Số nguyên | Số logical core | Số luồng phục vụ runtime async Tokio của lõi native core. |

### 2.4. Đường ống Âm thanh và Nhận dạng Giọng nói (Voice & Audio)

| Tên biến | Kiểu giá trị | Mặc định | Ý nghĩa & Hướng dẫn |
|---|---|---|---|
| `LIVA_WAKE_MODE` | `none`, `vi`, `en`| `vi` | Chế độ nhận diện từ khóa kích hoạt: `vi` ("LIVA ơi"), `en` ("Hey LIVA") hoặc `none` (chỉ bấm mic). |
| `LIVA_VAD_THRESHOLD` | Float `0.0 - 1.0` | `0.5` | Ngưỡng xác suất phát hiện tiếng nói của Silero VAD. |
| `LIVA_SMART_TURN_GATE` | Số nguyên (ms) | `225` | Trần độ trễ quyết định ngắt lượt của cổng `Smart Turn v3.2`. |

---

## 3. Cấu hình Mẫu trên PowerShell (Windows)

Để khởi chạy phiên làm việc phát triển với các biến tùy chỉnh:
```powershell
# Thiết lập biến môi trường cục bộ trong phiên PowerShell hiện tại
$env:LIVA_DB_PATH = "$PWD\data\liva_dev.db"
$env:LIVA_LLM_N_GPU_LAYERS = "28"
$env:LIVA_WAKE_MODE = "vi"
$env:RUST_LOG = "info,liva_native_core=debug,liva_storage=debug"

# Khởi động ứng dụng kiểm tra
cargo run -p liva-tools -- doctor
```
