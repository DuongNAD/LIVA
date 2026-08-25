---
title: "Desktop Tauri — cửa sổ, capability và native IPC boundary"
updated: 2026-08-25
commit: ac693be8
status: living
owns:
  - bang-tauri-command
  - cau-hinh-cua-so
  - desktop-tauri-runtime
  - desktop-capability-boundary
covers:
  - liva-desktop/src-tauri/Cargo.toml
  - liva-desktop/src-tauri/tauri.conf.json
  - liva-desktop/src-tauri/tauri.macos.conf.json
  - liva-desktop/src-tauri/tauri.windows.conf.json
  - liva-desktop/src-tauri/src/main.rs
  - liva-desktop/src-tauri/src/lib.rs
  - liva-desktop/src-tauri/capabilities/widget.json
  - liva-desktop/src-tauri/capabilities/dashboard.json
  - liva-desktop/src-tauri/capabilities/setup.json
  - liva-desktop/src-tauri/tests/capability_policy.rs
  - liva-native-core/src/authorization.rs
  - liva-native-core/src/boot.rs
  - liva-native-core/src/websocket.rs
  - liva-ui/public/setup.html
  - liva-ui/public/setup.js
---
# Desktop Tauri — cửa sổ, capability và native IPC boundary

[⬆ Mục lục](../README.md) · [Frontend runtime](frontend.md) ·
[Persistence](persistence.md) · [Threat model](../05-chat-luong/threat-model.md)

## 1. Kết luận as-built

Tauri là vỏ desktop mỏng nhưng có bốn trách nhiệm đặc quyền:

1. dựng cùng `AppState` và background services với standalone core;
2. quản lý ba loại cửa sổ;
3. cấp capability/identity theo exact window label;
4. giữ secret vault per-machine và bridge native IPC.

`liva-desktop/src-tauri/src/lib.rs#run` là entry chuẩn. Nó gọi
`liva-native-core/src/boot.rs#build_app_state`, sau đó trong Tauri async runtime gọi
`boot::spawn_background_services`. Desktop và standalone vì vậy dùng cùng gateway, model
autoload, memory projection, TTS cleanup, Telegram và governor.

## 2. Cửa sổ

| Label | Nguồn | Hành vi |
|---|---|---|
| `widget` | `/widget.html` | fullscreen-sized transparent overlay, always-on-top, không decoration/taskbar |
| `dashboard` | `/dashboard.html` | 1200×800, tối thiểu 900×600, visible, không always-on-top |
| `setup` | `setup.html` | tạo động khi thiếu model bắt buộc hoặc người dùng gọi setup |

Widget và dashboard khai trong `tauri.conf.json`. Setup được tạo bởi
`liva-desktop/src-tauri/src/lib.rs#mo_cua_so_setup`, chỉ có một instance và dùng asset tĩnh từ
`liva-ui/public`.

## 3. Capability theo cửa sổ

Chỉ ba capability được bật, mỗi capability gắn đúng một window:

| Capability | Có | Cố ý không có |
|---|---|---|
| widget | native IPC, WS ticket, ghost/zones, mở dashboard, event listen | vault, setup, dialog, process |
| dashboard | native IPC, WS ticket, vault write-only, dialog, process lifecycle | ghost/zones, mở dashboard |
| setup | native IPC/stream, mở dashboard, event listen, close | WS ticket, vault, ghost, dialog, process |

`capability_policy.rs` khóa cả allow và deny lists. Capability Tauri chỉ cho phép command đi vào
host; command core còn qua `liva-native-core/src/authorization.rs#authorize_command`.

## 4. Catalog native command

| Command | Cửa sổ dự kiến | Chức năng |
|---|---|---|
| `toggle_ghost_mode` | widget | bật/tắt click-through trực tiếp |
| `set_eco_mode` | không capability hiện hành | đổi nhịp hit-test; handler tồn tại nhưng UI không được cấp |
| `update_interactive_zones` | widget | cập nhật vùng nhận click |
| `open_dashboard` | widget/setup | hiện hoặc tạo dashboard |
| `open_setup` | chưa được capability cấp | mở setup |
| `vault_secret_present` | dashboard | chỉ trả presence |
| `store_vault_secret` | dashboard | ghi secret vào Stronghold private helper |
| `delete_vault_secret` | dashboard | xóa secret |
| `issue_websocket_session` | widget/dashboard | cấp ticket 256-bit, TTL 30 giây, dùng một lần |
| `native_ipc_call` | cả ba | command/response vào unified core |
| `native_ipc_call_stream` | cả ba | command + event stream theo request ID |

Handler có mặt không đồng nghĩa mọi cửa sổ được gọi. `set_eco_mode` và `open_setup` hiện là bề mặt
không được capability nào cấp; muốn dùng phải có yêu cầu sản phẩm và negative test trước khi mở
quyền.

## 5. Native IPC authorization

`liva-desktop/src-tauri/src/lib.rs#native_ipc_call` và
`liva-desktop/src-tauri/src/lib.rs#native_ipc_call_stream` lấy `window.label()`, ánh xạ qua
`authorize_tauri_principal`, rồi gọi `handle_command_as`.

Chuỗi `command` từ renderer không được tin. Widget/dashboard/setup có allow-list core khác nhau;
label lạ fail-closed. Stream dùng channel bounded 100 và emit `ipc-stream:<req_id>` về đúng cửa
sổ gọi.

## 6. WebSocket session

`liva-desktop/src-tauri/src/lib.rs#issue_websocket_session` chỉ chấp nhận widget/dashboard.
Authority được gateway background service trao cho Tauri state khi bind xong. Ticket:

- ngẫu nhiên 32 byte, serialize hex 64 ký tự;
- lưu SHA-256 digest, không lưu plaintext;
- hết hạn sau 30 giây;
- tiêu thụ một lần;
- chỉ hợp lệ từ peer loopback.

Frontend Widget phải xin ticket trước **mỗi** kết nối. Không có ticket thì principal là remote;
query tự khai `principal=` bị từ chối ngay handshake.

## 7. Setup và preflight model

Sau khi services chạy, host gọi preflight model. Nếu thiếu model bắt buộc, setup window hiển thị
`setup:status`, `setup:paths`, `setup:fetch`; tải stream progress và có thể resume.

Dashboard System còn gọi `get_preflight_status`, lệnh chỉ cấp cho principal Dashboard local. Core
trả đúng vector `preflight::thu_thap()` mà CLI `--preflight` dùng, với schema
`name/available/status/consequence`; UI không tự viết lại logic dò GPU/model/vec0. Việc chuyển
`preflight` từ module riêng của binary sang module public của core là để giữ contract một nguồn.

Setup là HTML/JS tĩnh và dùng `window.__TAURI__`, nên `withGlobalTauri=true` vẫn cần cho entry này.
CSP hiện không có `unsafe-inline`, chỉ cho self/data/asset và loopback dev/gateway cần thiết.
Muốn bỏ global Tauri phải chuyển setup sang module import hoặc Vue trước, không được tắt mù.

## 8. Ghost mode và lifecycle

Rust thread hit-test:

- cache scale factor/vị trí cửa sổ;
- gọi `liva-desktop/src-tauri/src/lib.rs#check_cursor_in_zones`;
- chỉ đổi `set_ignore_cursor_events` khi trạng thái đổi;
- poll 30/100/500 ms gần vùng, chậm hơn ở xa; eco mode nhân nhịp.

Thread chưa có shutdown handle riêng; nó sống theo process. Background Tokio services cũng được
giữ đến process shutdown bởi runtime Tauri.

## 9. Đóng gói

Bundle NSIS current-user gồm WebView2 offline installer và
`data/models-manifest.json`; model nặng tải ở lần chạy đầu. `frontendDist` trỏ tới
`../../liva-ui/dist`.

⚠️ **Từ `286204d7` (25/08/2026), tài nguyên phụ thuộc nền KHÔNG còn nằm trong
`tauri.conf.json`.** File gốc chỉ giữ phần chung; phần theo nền tách ra hai file
Tauri tự nạp chồng:

| File | Chứa |
|---|---|
| `tauri.windows.conf.json` | `sqlite-vec-windows-x64/vec0.dll` → `vec0.dll`, và `cuda-redist` → `./` |
| `tauri.macos.conf.json` | `sqlite-vec-darwin-arm64/vec0.dylib` → `vec0.dylib` |

Lý do tách: bản dựng macOS trước đó cố nhặt `vec0.dll` — một artifact Windows
không tồn tại trên cây phụ thuộc darwin — nên bundle gãy. Thêm tài nguyên mới
phải hỏi "cái này có phụ thuộc nền không?" trước khi ghi vào file gốc.

📌 Ràng buộc trust theo nền của chính `vec0`: [Phát triển trên macOS](../02-van-hanh/07-macos-dev.md)

## 10. Acceptance

```powershell
cargo test -p liva-desktop
cargo check -p liva-desktop
npm run build -w liva-ui
npm run test:installer
npm run test:gateway
```

Security acceptance tối thiểu: capability chỉ gắn một window; widget không có vault/process;
setup không có WS/vault; command core kiểm principal; ticket replay/expired/non-loopback/self-
declared bị từ chối; CSP không có inline/eval.
