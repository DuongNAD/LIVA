---
title: "Tổng quan Kiến trúc và Tầm nhìn Hệ thống LIVA"
updated: 2026-09-29
commit: 25e4229
status: living
covers:
  - Cargo.toml
  - liva-native-core/src/lib.rs
  - liva-desktop/src-tauri/src/lib.rs
---

# Tổng quan Kiến trúc và Tầm nhìn Hệ thống LIVA

## 1. Tầm nhìn và Triết lý Thiết kế

LIVA (Local Intelligent Virtual Assistant) là hệ thống trợ lý AI cá nhân hoạt động theo triết lý **Local-First**, ưu tiên tối đa tính bảo mật dữ liệu, tốc độ phản hồi thời gian thực và sự ổn định trên phần cứng máy trạm phổ thông.

### Các nguyên lý cốt lõi:
- **Local-First & Quyền riêng tư tuyệt đối**: Toàn bộ dữ liệu hội thoại, đồ thị tri thức, thông tin ngữ cảnh và vector nhúng được lưu trữ và xử lý trực tiếp trên máy của người dùng. Không có luồng dữ liệu tự ý rò rỉ lên máy chủ đám mây bên ngoài.
- **Native Performance (Rust & C-FFI)**: Toàn bộ lõi điều phối, lưu trữ và suy luận AI được hợp nhất trong mã nguồn Rust bản địa, biên dịch tĩnh với C-FFI tối ưu hoá phần cứng AVX2/FMA, loại bỏ hoàn toàn các tầng trung gian Node.js và Python runtime.
- **Zero Loopback TCP**: Giao tiếp giữa giao diện người dùng (WebView2 / Tauri) và lõi xử lý Rust diễn ra 100% qua bộ đệm nội hàm tiến trình (in-process IPC Channels), không chiếm dụng bất kỳ cổng mạng loopback nào trên hệ điều hành.
- **Hardware Budget Guardrails**: Giới hạn trần tài nguyên nghiêm ngặt: bộ nhớ RAM $\le 4.0\text{ GB}$ và bộ nhớ VRAM $\le 5.1\text{ GB}$, đảm bảo hệ thống vận hành êm ái khi chạy nền cùng các tác vụ văn phòng và đồ hoạ nặng.

---

## 2. Cấu trúc Multi-Crate Workspace

Hệ thống mã nguồn của LIVA được tổ chức thành Rust Workspace gồm đúng 7 crate thành viên chuyên biệt:

```
LIVA/
├── Cargo.toml                          # Workspace root manifest
├── liva-desktop/src-tauri/             # Crate: liva-desktop (Vỏ ứng dụng Tauri v2)
├── liva-native-core/                   # Crate: liva-native-core (Lõi điều phối Facade)
├── crates/
│   ├── liva-core-types/                # Crate: liva-core-types (Domain models & IPC schemas)
│   ├── liva-storage/                   # Crate: liva-storage (SQLite WAL, Radix Trie, sqlite-vec)
│   ├── liva-llm/                       # Crate: liva-llm (Actor-based LLM engine)
│   ├── liva-tools/                     # Crate: liva-tools (CLI suite chẩn đoán và benchmark)
│   └── liva-cua/                       # Crate: liva-cua (Computer-Use Agent Win32 automation)
└── liva-ui/                            # Giao diện người dùng Vue 3 + Three.js
```

### Phân công trách nhiệm của 7 Crate:

| Crate | Đường dẫn | Trách nhiệm chính |
|---|---|---|
| `liva-desktop` | `liva-desktop/src-tauri` | Khởi tạo cửa sổ Desktop (Tauri v2), khay hệ thống (System Tray), đăng ký hook chuột cấp thấp (`WH_MOUSE_LL`) và ánh xạ lệnh IPC sang native core. |
| `liva-native-core` | `liva-native-core` | Điều phối toàn cục (`AppState`), StateGraph vòng đời tương tác, đường ống âm thanh WebRTC thoại hai chiều, phân tích thị giác Qwen3-VL, tích hợp Telegram và máy chủ MCP. |
| `liva-core-types` | `crates/liva-core-types` | Định nghĩa kiểu miền dữ liệu thuần túy (Domain Types), phân cấp lỗi chuẩn `thiserror`, quyền hạn bảo mật và lược đồ IPC tương thích Specta. |
| `liva-storage` | `crates/liva-storage` | Cơ chế lưu trữ SQLite WAL đa kết nối, actor ghi đơn luồng `DbActor` vi lô (micro-batching), cây tiền tố Radix Trie truy hồi tức thì, và bảng ảo vector biên dịch tĩnh C `sqlite-vec`. |
| `liva-llm` | `crates/liva-llm` | Động cơ suy luận ngôn ngữ dạng Actor với hàng đợi ưu tiên 3 cấp (High/Normal/Low), định tuyến mô hình, phân bổ ngân sách token và cơ chế ngắt lệnh sớm qua Tokio blocking task. |
| `liva-tools` | `crates/liva-tools` | Bộ công cụ dòng lệnh (CLI) chẩn đoán hệ thống, kiểm tra tính toàn vẹn cơ sở dữ liệu (`db_probe`), đo kiểm độ trễ thoại (`voice_bench`), LLM (`llm_bench`) và giám định tổng thể (`doctor`). |
| `liva-cua` | `crates/liva-cua` | Tự động hóa giao diện desktop native Win32 (Computer-Use Agent), điều khiển chuột/bàn phím ảo qua Win32 API, chuyển đổi tọa độ DPI-aware và chốt ngắt khẩn cấp phần cứng (`VK_ESCAPE`). |

---

## 3. Ranh giới Tài nguyên và Định mức Phần cứng

Nhằm ngăn ngừa hiện tượng cạn kiệt bộ nhớ (OOM) hoặc gián đoạn giao diện desktop, LIVA thiết lập các rào chắn phần cứng được giám sát chủ động:

```
┌─────────────────────────────────────────────────────────────┐
│                 BỘ ĐỆM TÀI NGUYÊN HỆ THỐNG                  │
├──────────────────────────────┬──────────────────────────────┤
│ Mức RAM hoạt động            │ Trần tối đa: 4.0 GB          │
│ Tiêu thụ thực tế đo đạc      │ ~ 2.27 GB (Đỉnh tải: 3.0 GB) │
├──────────────────────────────┼──────────────────────────────┤
│ Mức VRAM GPU vật lý          │ Trần phân bổ: 5.1 GB / 6 GB  │
│ Dành riêng Windows DWM / OS  │ 1.0 GB                       │
│ Tiêu thụ AI thực tế          │ ~ 4.46 GB                    │
└──────────────────────────────┴──────────────────────────────┘
```

1. **Bộ điều phối tài nguyên (`Governor`)**: Theo dõi mức độ sử dụng CPU và GPU hệ thống (thông qua NVML). Nếu tác vụ ngoài (như ứng dụng đồ họa 3D hoặc trò chơi toàn màn hình) đẩy tải CPU/GPU vượt ngưỡng 80%, LIVA chủ động hạ mức ưu tiên tiến trình xuống `BELOW_NORMAL_PRIORITY_CLASS` và giảm bớt số layer offload GPU.
2. **Bộ điều phối thị giác (`VisualGovernor`)**: Mô hình VLM phân tích màn hình (chiếm ~750 MB VRAM) được đặt ở chế độ ngủ đông khi người dùng chỉ tương tác bằng giọng nói. Khi có yêu cầu chụp màn hình (`vision:ask`), VLM được kích hoạt tạm thời và tự động xả bộ đệm sau 15 giây không hoạt động.

---

## 4. Mô hình Bảo mật và Vòng đời Tác vụ

Mọi hành động có khả năng tác động đến hệ thống tệp, gửi tin nhắn ra bên ngoài hoặc điều khiển chuột bàn phím đều tuân thủ chính sách:
- **Xác nhận hai pha (Two-Phase Confirmation / HITL)**: Hành động nhạy cảm được đưa vào trạng thái chờ duyệt (Pending Action) trên giao diện Tauri, người dùng phải chủ động nhấn xác nhận trước khi bộ điều phối chuyển lệnh cho động cơ thực thi.
- **Sổ cái kiểm toán bất biến (Action Audit Ledger)**: Mỗi thao tác can thiệp hệ điều hành hoặc truy cập dữ liệu đều được ghi lại vào bảng `action_audit_ledger` trong SQLite WAL thông qua `DbActor`, có gắn mốc thời gian và mã định danh phiên.
