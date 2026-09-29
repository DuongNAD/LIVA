---
title: "Tài liệu Kỹ thuật Crate: liva-desktop (src-tauri)"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - liva-desktop/src-tauri/Cargo.toml
  - liva-desktop/src-tauri/tauri.conf.json
  - liva-desktop/src-tauri/src/lib.rs
---

# Tài liệu Kỹ thuật Crate: liva-desktop (src-tauri)

## 1. Tổng quan về Vỏ Ứng dụng Desktop Tauri v2

`liva-desktop/src-tauri` là crate đóng vai trò vỏ ứng dụng máy tính (Desktop Shell) cho LIVA, được xây dựng trên nền tảng **Tauri v2**. Crate này liên kết trực tiếp với thư viện `liva-native-core` trong cùng một tiến trình hệ điều hành mà không thông qua bất kỳ cổng mạng loopback nào.

### Các trách nhiệm then chốt:
- Quản lý kiến trúc đa cửa sổ (Multi-Window): Widget nổi trong suốt, Dashboard quản trị và màn hình cài đặt Setup.
- Tích hợp khay hệ thống (System Tray) với các tùy chọn chuyển đổi chế độ nhanh.
- Đăng ký hook chuột cấp thấp Windows (`WH_MOUSE_LL`) hỗ trợ cơ chế xuyên thấu thông minh (Ghost Mode).
- Đăng ký và phân phối các lệnh IPC từ giao diện WebView2 tới lõi Native Core.

---

## 2. Kiến trúc Đa Cửa sổ (Multi-Window Topology)

Cấu hình trong `tauri.conf.json` định nghĩa 3 cửa sổ độc lập phục vụ các trải nghiệm khác nhau:

```
┌─────────────────────────────────────────────────────────────┐
│                   HỆ THỐNG ĐA CỬA SỔ TAURI                  │
├─────────────────────────────────────────────────────────────┤
│ 1. CỬA SỔ WIDGET (label: "widget")                          │
│    - Cửa sổ nổi không viền (transparent: true, alwaysOnTop) │
│    - Hiển thị Avatar 3D (Three.js), viseme khẩu hình, mic   │
│    - Hỗ trợ Ghost Mode xuyên thấu khi chuột ra ngoài Avatar  │
├─────────────────────────────────────────────────────────────┤
│ 2. CỬA SỔ DASHBOARD (label: "dashboard")                    │
│    - Cửa sổ ứng dụng hoàn chỉnh (có thanh tiêu đề chuẩn)    │
│    - Quản lý bộ nhớ, đồ thị tri thức, sổ cái kiểm toán CUA  │
│    - Phê duyệt các hành động Human-in-the-Loop (HITL)       │
├─────────────────────────────────────────────────────────────┤
│ 3. CỬA SỔ SETUP (label: "setup")                            │
│    - Hướng dẫn thiết lập ban đầu: tải weights, tạo mật khẩu │
│    - Tự động đóng sau khi hoàn tất thiết lập ban đầu        │
└─────────────────────────────────────────────────────────────┘
```

---

## 3. Cơ chế Xuyên thấu Thông minh Ghost Mode (`WH_MOUSE_LL`)

Để trợ lý ảo 3D avatar có thể luôn hiển thị trên màn hình (`alwaysOnTop`) mà không làm cản trở người dùng nhấp chuột vào các tài liệu, trình duyệt hay biểu tượng bên dưới:
- **Hook chuột cấp thấp**: Hệ thống đăng ký hook Win32 `WH_MOUSE_LL` để theo dõi tọa độ con trỏ chuột theo thời gian thực.
- **Vùng tương tác (`interactive_zones`)**: Giao diện Vue 3 tính toán tọa độ hộp bao (Bounding Box) của Avatar và các nút điều khiển, sau đó gửi danh sách này xuống Rust qua lệnh `update_interactive_zones`.
- **Chuyển đổi trạng thái nhấp chuột**:
  - Khi con trỏ chuột nằm **ngoài** vùng tương tác: Rust đặt thuộc tính cửa sổ sang `WS_EX_TRANSPARENT`. Mọi cú click chuột của người dùng sẽ xuyên thẳng xuống các cửa sổ ứng dụng làm việc phía dưới mà không bị Avatar chặn lại.
  - Khi con trỏ chuột di chuyển **vào trong** vùng tương tác: Thuộc tính xuyên thấu được gỡ bỏ ngay lập tức, cho phép người dùng bấm vào Avatar hoặc các nút chức năng bình thường.

---

## 4. Danh mục Lệnh Tauri Đăng ký (`src/lib.rs`)

Các lệnh IPC được đăng ký trực tiếp vào `tauri::Builder`:

| Nhóm lệnh | Tên hàm gọi | Mô tả chức năng |
|---|---|---|
| **Điều khiển cửa sổ** | `toggle_ghost_mode` | Bật/tắt chế độ xuyên thấu chuột thủ công. |
| | `set_eco_mode` | Giảm tốc độ khung hình rendering 3D khi hệ thống tải nặng. |
| | `update_interactive_zones` | Cập nhật vùng tọa độ nhận click chuột cho Avatar. |
| | `open_dashboard` / `open_setup` | Mở các cửa sổ Dashboard hoặc Setup tương ứng. |
| **Kho khóa Stronghold** | `vault_secret_present` | Kiểm tra sự tồn tại của khóa bí mật trong kho lưu trữ an toàn. |
| | `store_vault_secret` | Lưu trữ an toàn mật khẩu hoặc token API. |
| | `delete_vault_secret` | Xóa khóa khỏi Stronghold Vault. |
| **Giao tiếp IPC Lõi** | `native_ipc_call` | Thực thi lệnh đồng bộ tới `liva-native-core`. |
| | `native_ipc_call_stream` | Kích hoạt luồng sinh token LLM dạng streaming qua sự kiện cửa sổ. |
| **Âm thanh Thoại** | `voice_subscribe` | Đăng ký kênh truyền nhận sự kiện viseme và âm thanh thời gian thực. |
| | `voice_mic_chunk` | Đẩy dữ liệu âm thanh từ microphone vào bộ đệm DSP. |
| | `voice_wake_probe` | Thử nghiệm nhận diện từ khóa wake word. |
| | `voice_interrupt` | Ngắt ngay lập tức câu trả lời của trợ lý khi người dùng nói chen vào. |
