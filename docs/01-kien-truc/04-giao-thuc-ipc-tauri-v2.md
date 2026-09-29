---
title: "Giao thức Giao tiếp Nội trình Tauri v2 IPC"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - liva-desktop/src-tauri/src/lib.rs
  - crates/liva-core-types/src/lib.rs
  - liva-ui/src/platform/TauriAdapter.ts
---

# Giao thức Giao tiếp Nội trình Tauri v2 IPC

## 1. Khai tử WebSocket Loopback và Ưu thế của Native IPC

Trong các phiên bản kiến trúc ban đầu, giao diện người dùng giao tiếp với lõi điều phối thông qua máy chủ WebSocket loopback cổng `8002`. Kiến trúc cũ này bộc lộ nhiều điểm yếu:
- Chiếm dụng cổng TCP nội bộ, tiềm ẩn nguy cơ xung đột cổng với các phần mềm khác trên Windows.
- Nguy cơ bảo mật khi các tiến trình không có thẩm quyền trên cùng máy tính có thể quét và gửi yêu cầu giả mạo vào cổng loopback.
- Phải xử lý bổ sung lớp mã hóa gói tin và bắt tay xác thực (handshake token) làm tăng độ trễ giao tiếp.

**Kiến trúc hiện tại của LIVA đã loại bỏ 100% WebSocket loopback**. Giao diện Vue 3 chạy trên WebView2 kết nối trực tiếp với lõi Rust `liva-native-core` thông qua cơ chế **In-Process IPC Channels** của Tauri v2.

### Lợi ích cốt lõi:
- **Zero Open Ports**: LIVA không mở bất kỳ cổng mạng TCP/UDP nào trên hệ điều hành. Tường lửa Windows không hiển thị cảnh báo chặn mạng.
- **Tốc độ truyền dữ liệu bộ nhớ trực tiếp (Zero-Copy Transfer)**: Các gói tin âm thanh thoại, dữ liệu viseme điều khiển cơ mặt 3D avatar và chuỗi token suy luận được trao đổi trực tiếp qua bộ nhớ RAM giữa các luồng trong cùng một tiến trình.
- **Kiểm soát ngữ cảnh chặt chẽ**: Mỗi lệnh gọi IPC gắn liền với nhãn cửa sổ (Window Label) của Tauri, cho phép hệ thống phân quyền tự động dựa trên vị trí phát sinh lệnh.

---

## 2. Các Mẫu Lệnh IPC (IPC Command Patterns)

Giao thức IPC giữa `liva-ui` và `liva-desktop/src-tauri` được chuẩn hóa thành 3 mẫu chính:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        CÁC MẪU LỆNH TAURI V2 IPC                       │
├────────────────────────────────────────────────────────────────────────┤
│ 1. REQUEST-RESPONSE ĐƠN NHẤT (invoke: native_ipc_call)                 │
│    - Sử dụng cho các tác vụ ngắn: cấu hình, kiểm tra trạng thái,       │
│      truy vấn thông tin bộ nhớ, lấy danh sách kỹ năng.                 │
├────────────────────────────────────────────────────────────────────────┤
│ 2. DÒNG SỰ KIỆN VĂN BẢN (invoke: native_ipc_call_stream)               │
│    - Dành cho sinh văn bản từ LLM và tiến trình tác vụ nhiều bước.    │
│    - Rust mở kênh mpsc và phát sự kiện window.emit("ipc-stream:<id>") │
├────────────────────────────────────────────────────────────────────────┤
│ 3. DÒNG THỜI GIAN THỰC NHỊ PHÂN (tauri::ipc::Channel<VoiceIpcEvent>)   │
│    - Dành cho thoại hai chiều: viseme 3D Avatar, VAD state, audio.     │
│    - Đăng ký qua lệnh voice_subscribe.                                │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1. Request-Response qua `native_ipc_call`
Frontend gửi yêu cầu đồng bộ:
```typescript
const response = await invoke('native_ipc_call', {
  command: 'config:get',
  payload: { key: 'audio.wake_word' }
});
```
Tại tầng Rust (`liva-desktop/src-tauri/src/lib.rs`), hàm `authorize_tauri_principal` kiểm tra thẩm quyền của cửa sổ, sau đó điều phối tới `liva_native_core::handle_command_as`.

### 2.2. Streaming LLM Token qua `native_ipc_call_stream`
Khi người dùng gửi câu hỏi cần câu trả lời dài dạng streaming:
1. Frontend sinh mã ngẫu nhiên `req_id = "req-" + uuid()`.
2. Lắng nghe sự kiện: `listen("ipc-stream:" + req_id, (event) => { appendToken(event.payload); })`.
3. Gọi lệnh: `invoke("native_ipc_call_stream", { command: "chat:completion", payload: {...}, req_id })`.
4. Rust tạo kênh truyền `mpsc::channel(100)` gắn với `LlmActor`. Khi từng token được sinh ra, task chuyển tiếp phát trực tiếp vào cửa sổ qua `window.emit`.

### 2.3. Dòng thoại hai chiều thời gian thực qua `voice_subscribe`
Để đồng bộ khẩu hình 3D avatar (viseme) và phản hồi giọng nói với độ trễ thấp nhất:
- Cửa sổ widget nổi (`liva-widget`) đăng ký kênh nhận sự kiện:
  ```typescript
  const channel = new Channel<VoiceIpcEvent>();
  channel.onmessage = (event) => {
    if (event.type === 'viseme') updateBlendShapes(event.visemes);
    if (event.type === 'transcript') displayUserSpeech(event.text);
  };
  await invoke('voice_subscribe', { channel });
  ```
- Phía Rust lưu trữ `Channel` và bắn trực tiếp các sự kiện từ đường ống WebRTC/TTS vào UI mà không cần qua hàng đợi trung gian.

---

## 3. Phân quyền và Xác thực Ngữ cảnh Gọi (Principal & Permissions)

LIVA phân quyền lệnh dựa trên ngữ cảnh cửa sổ gốc:

| Window Label | Principal | Phạm vi quyền hạn cho phép |
|---|---|---|
| `widget` | `CommandPrincipal::TauriWidget` | Giao tiếp thoại, điều khiển avatar, hiển thị thông báo, nhận lệnh nhanh từ người dùng. Bị chặn truy cập các lệnh cấu hình nhạy cảm. |
| `dashboard` | `CommandPrincipal::TauriDashboard` | Quản lý toàn diện: duyệt bộ nhớ, xem đồ thị tri thức, cấu hình tham số hệ thống, duyệt hành động HITL chờ phê duyệt, xem sổ cái kiểm toán. |
| `setup` | `CommandPrincipal::TauriSetup` | Chỉ được phép chạy các lệnh cài đặt ban đầu: tải mô hình AI, thiết lập mật khẩu kho khóa Stronghold, kiểm tra phần cứng. |

Nếu một cửa sổ cố tình gọi lệnh ngoài danh sách quyền hạn được gán (ví dụ: `widget` cố tình gọi lệnh xóa dữ liệu `db:delete`), hệ thống sẽ lập tức từ chối và trả về mã lỗi `PermissionDenied`.

---

## 4. An toàn Kiểu Dữ liệu với Specta

Để đảm bảo không xảy ra hiện tượng lệch định dạng dữ liệu (type mismatch) giữa frontend TypeScript và backend Rust:
- Tất cả các cấu trúc thông điệp trong `crates/liva-core-types` được trang trí bằng thuộc tính `#[derive(specta::Type, Serialize, Deserialize)]`.
- Quy trình kiểm thử và CI tự động xuất tệp định nghĩa TypeScript tương ứng, đảm bảo kiểm tra kiểu tĩnh (Static Type Checking) hoàn chỉnh từ khâu biên dịch.
