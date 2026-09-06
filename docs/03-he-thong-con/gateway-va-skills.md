---
title: "Gateway Control Plane, Kênh Liên Lạc & Skills Runtime"
updated: 2026-09-03
commit: 35f5f26d
status: living
owns: []
covers:
  - liva-native-core/src/skills
---
# Hệ Thống Con: Gateway Control Plane, Kênh Liên Lạc & Skills Runtime

[⬆ Mục lục](../README.md)


Tài liệu thiết kế và kiến trúc cho Gateway Control Plane, hệ sinh thái Adapter Đa kênh và ClawHub Skills Runtime trong LIVA Native Rust Core.

---

## 1. Gateway Control Plane & Chuẩn Hóa Tin Nhắn (`messaging` & `gateway`)

### 1.1. Ingress Message Normalizer (`messaging::normalized`)
- Mọi tin nhắn đến từ bất kỳ kênh nào (Telegram, WhatsApp, Discord, Slack, Tauri IPC, WebSocket) đều được chuẩn hóa thành `IncomingMessage` mang định danh duy nhất (`MessageId`), định danh phiên (`SessionId`), thông tin người gửi (`MessageSender`), thời gian UTC, nội dung (`ContentPayload`) và danh sách tệp đính kèm (`Attachment`).
- Tin nhắn gửi đi được chuẩn hóa qua `OutgoingMessage` kèm mức độ khẩn cấp (`DeliveryUrgency`) và biên nhận phát hành (`DeliveryReceipt`).

### 1.2. Session & Memory Isolation Router (`messaging::session`)
- Quản lý phiên hội thoại hỗ trợ 3 phạm vi ký ức (`MemoryScope`):
  - `GlobalMain`: Ký ức dài hạn toàn cục dùng chung.
  - `Isolated`: Phiên cô lập, tạm thời cho tác vụ thăm dò hoặc probe.
  - `CustomSession(id)`: Luồng ngữ cảnh chuyên biệt theo từng kênh / người dùng.
- Cơ chế dọn dẹp phiên hết hạn an toàn đa luồng (thread-safe TTL eviction).

### 1.3. WebSocket Gateway Control Plane & Ghép Đôi Node (`gateway`)
- Giao thức JSON-RPC 2.0 full-duplex phân quyền theo vai trò (`operator`, `node`, `client`).
- Hỗ trợ Pub/Sub topic event bus, stream dữ liệu theo khối và lệnh gọi RPC trực tiếp giữa các nút đồng hành.
- Giao thức ghép đôi thiết bị đồng hành (Companion Node Pairing) dựa trên chữ ký mật mã Ed25519 / HMAC và mã xác thực ngắn 6 chữ số (Short-Code Challenge).

---

## 2. Hệ Sinh Thái Adapter Đa Kênh (`channels`)

### 2.1. Base Adapter Trait (`channels::adapter`)
- Trait `ChannelAdapter` định nghĩa vòng đời kết nối bất đồng bộ: `connect`, `disconnect`, `poll_stream`, `send_message`, `handle_webhook`, và `status`.
- Hỗ trợ cơ chế lùi số mũ tự động có ngẫu nhiên hóa (Exponential Backoff with Jitter) khi mạng bị gián đoạn.

### 2.2. Các Bộ Chuyển Đổi Kênh
- **Telegram (`telegram.rs`)**: Hỗ trợ Teloxide long-polling, xử lý luồng tin nhắn debounced, voice PTT và lọc whitelist người dùng.
- **WhatsApp (`whatsapp.rs`)**: Xử lý webhook Meta Cloud API, kiểm tra chữ ký HMAC SHA-256 (fail-closed), giải mã media và voice note.
- **Discord (`discord.rs`)**: Kết nối Discord Gateway v10 WebSocket, hỗ trợ thread, mention và định dạng markdown.
- **Slack (`slack.rs`)**: Hỗ trợ Slack Socket Mode và Web API, phân tích Block Kit sang Markdown và theo dõi thread timestamp (`thread_ts`).

---

## 3. Hệ Sinh Thái Kỹ Năng ClawHub & MCP (`skills`)

### 3.1. ClawHub `SKILL.md` Manifest Parser (`skills::manifest`)
- Đọc và phân tích định dạng chuẩn `SKILL.md` gồm YAML Frontmatter và hướng dẫn Markdown.
- Khai báo trigger, định nghĩa công cụ (`SkillToolDefinition`), yêu cầu quyền (`PermissionRequirement`) và phân loại mức độ rủi ro (`RiskLevel`: Safe, Moderate, Dangerous, Critical).

### 3.2. Bộ Theo Dõi Nạp Động (Live Hot-Reload Watcher - `skills::watcher`)
- Theo dõi hệ thống tệp thời gian thực với mã băm SHA-256 fingerprint diffing.
- Tự động phát hiện thay đổi và cập nhật chỉ mục tìm kiếm mà không cần khởi động lại tiến trình.

### 3.3. Động Cơ Đồng Ý Giữa Chu Kỳ (Mid-Execution Consent - `skills::consent`)
- Tạm dừng thực thi bất đồng bộ (`AWAITING_CONSENT`) khi gặp công cụ rủi ro cao.
- Phát yêu cầu phê duyệt tới giao diện UI hoặc kênh tin nhắn DM của người vận hành.
- Áp dụng chính sách Fail-Closed tự động từ chối khi hết thời gian chờ (timeout).

### 3.4. Cầu Nối Điều Phối Công Cụ (MCP Tool Dispatcher - `skills::dispatcher`)
- Cầu nối hợp nhất định tuyến lệnh gọi giữa các công cụ thuần Rust (`NativeToolHandler`), máy chủ ngoài MCP (`NativeMcpServer`) và hành động kỹ năng đã được sandbox hóa.
