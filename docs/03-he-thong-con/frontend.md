---
title: "Frontend runtime — widget, dashboard và transport Vue"
updated: 2026-08-07
commit: bd11c84
stale-ok: dec1c14
status: living
owns:
  - bang-man-hinh-dashboard
  - frontend-runtime-as-built
  - frontend-transport-contract
covers:
  - liva-ui/package.json
  - liva-ui/vite.config.ts
  - liva-ui/widget.html
  - liva-ui/dashboard.html
  - liva-ui/index.html
  - liva-ui/src/widget-main.ts
  - liva-ui/src/dashboard-main.ts
  - liva-ui/src/main.ts
  - liva-ui/src/WidgetApp.vue
  - liva-ui/src/DashboardApp.vue
  - liva-ui/src/App.vue
  - liva-ui/src/composables/useGateway.ts
  - liva-ui/src/platform/IPlatformAdapter.ts
  - liva-ui/src/platform/TauriAdapter.ts
  - liva-ui/src/platform/MockWebAdapter.ts
  - liva-ui/src/platform/index.ts
  - liva-ui/vitest.config.ts
---
# Frontend runtime — widget, dashboard và transport Vue

[⬆ Mục lục](../README.md) · [Desktop Tauri](desktop-tauri.md) ·
[Voice runtime](voice.md) · [Threat model](../05-chat-luong/threat-model.md)

## 1. Kết luận as-built

Bundle production có **hai entry Vue**:

| Entry | Root component | Vai trò |
|---|---|---|
| `widget.html` | `WidgetApp.vue` | avatar nổi, chat, voice, playback, draft-confirm, ghost mode |
| `dashboard.html` | `DashboardApp.vue` | shell quản trị và 11 màn hình subsystem |

`liva-ui/vite.config.ts#vendorChunkName` chia vendor lớn và cấu hình Rollup chỉ build hai entry
trên. `index.html → src/main.ts → App.vue` vẫn chạy nếu mở root Vite dev server, nhưng không nằm
trong bundle Tauri production. Nó là entry legacy/dev, không phải UI người dùng nhận trong
installer.

## 2. Bootstrap và platform adapter

`widget-main.ts` và `dashboard-main.ts` cùng:

1. tạo Vue app;
2. gọi `liva-ui/src/platform/index.ts#detectPlatform`;
3. provide `IPlatformAdapter` dưới key `platform`;
4. mount vào `#app`.

`liva-ui/src/platform/IPlatformAdapter.ts#IPlatformAdapter` là ranh giới cho window lifecycle,
vault write-only, event `gateway-ready` và native invoke. Hai implementation:

- `TauriAdapter`: dynamic import API Tauri, gọi command/cửa sổ/plugin thật;
- `MockWebAdapter`: không giữ secret value, mô phỏng gateway ở port 8002 và trả `null` cho native
  command.

Mock browser dùng để phát triển UI, không chứng minh capability desktop hay persistence thật.

## 3. Widget

`WidgetApp.vue` chịu các trách nhiệm:

- chọn/lazy-load avatar 2D hoặc VRM 3D;
- giữ chat state, markdown/rich text và draft gửi tin;
- capture mic qua `useVoicePipeline`, phát PCM qua `useSpeakerPlayback`;
- gửi vùng tương tác cho Rust để cửa sổ trong suốt click-through;
- mở Dashboard qua native command;
- duy trì WebSocket và exponential reconnect;
- bootstrap config, avatar, user profile và draft pending.

### 3.1 Identity WebSocket

Trong Tauri, `liva-ui/src/WidgetApp.vue:768` phải gọi
`issue_websocket_session` trước mỗi kết nối/reconnect, kiểm ticket 64 ký tự hex rồi mở
`/ws?session=<ticket>`. Ticket có TTL 30 giây và dùng một lần.

Không có ticket, server cố ý gán principal `WebSocketRemote`; các lệnh `get_config`,
`get_avatar_models`, `get_user_profile` và messaging widget sẽ bị từ chối. Regression test
`WidgetApp.test.ts` khóa URL có session. Browser dev không có capability Tauri nên kết nối remote
không ticket và chỉ được tập lệnh remote.

### 3.2 Ghost mode

Widget gửi bounding rectangles bằng `update_interactive_zones`. Rust hit-test con trỏ và chỉ cho
WebView nhận click trong vùng tương tác. Poll 150 ms phía Vue chỉ cập nhật geometry; quyết định
`set_ignore_cursor_events` nằm ở host Tauri.

## 4. Dashboard

`liva-ui/src/DashboardApp.vue#pageMap` chuyển component theo sidebar, không dùng Vue Router:

| ID | Component | Trách nhiệm |
|---|---|---|
| `avatar` | `AvatarGallery` | engine/model avatar |
| `ai` | `AISettings` | local/cloud model config và secret presence |
| `api` | `ApiManagementView` | credential write-only |
| `voice` | `VoiceManagementView` | profile/provider voice |
| `tasks` | `TaskManager` | CRUD task và plan |
| `memory` | `MemoryViewer` + `memory/{MemoryViewerHeader,MemoryViewerStats,MemoryViewerTabs}` | facts/conversation/delete; shell giữ data flow, phần trình bày đã tách |
| `skills` | `SkillsView` | list/toggle/test skill |
| `system` | `SystemView` | health thật + báo cáo preflight dùng chung với CLI |
| `vision` | `VisionView` | ask/watch màn hình |
| `profile` | `UserProfile` | hồ sơ người dùng |
| `settings` | `SettingsView` | consent, reset và UI settings |

`OnboardingForm` và GPU setup là overlay, không phải page ID. Root gọi `useGateway().init()` khi
mount và `destroy()` khi unmount; profile rỗng bật onboarding sau cửa sổ chờ 3,5 giây.

## 5. Transport của Dashboard

`liva-ui/src/composables/useGateway.ts#sendMsg` là dual transport:

- trong Tauri: `invoke("native_ipc_call")`; payload có `stream=true` dùng
  `native_ipc_call_stream` và event `ipc-stream:<req_id>`;
- trong browser: JSON WebSocket tới port 8002;
- response Tauri được chuẩn hóa qua `mapTauriResponse` để cập nhật cùng reactive state như WS.

`gatewayPrincipalForPath` chỉ quyết định bootstrap set của frontend. Nó **không cấp quyền**; host
Tauri và core authorization vẫn kiểm exact window label/principal.

`useGateway` giữ `preflightReport` sau phản hồi `get_preflight_status` ở cả Tauri và WebSocket,
đồng thời lọc schema từng item trước khi cho `SystemView` render. Màn hình chỉ ánh xạ
`available=true/false/null` thành sẵn sàng/mất năng lực/chưa kết luận; mọi chẩn đoán nằm ở Rust.

Widget không dùng `useGateway` làm transport thoại chính; nó cần WebSocket nhị phân để chuyển
audio và event streaming. Dashboard chủ yếu dùng native IPC.

## 6. Build, test và giới hạn

Vite cấm accidental Node built-ins (`fs`, `path`, `os`, `crypto`, `child_process`) trong frontend.
Build dùng base tương đối để Tauri `frontendDist` tải asset đúng.

Coverage gate hiện có ngưỡng tổng và per-file cho `WidgetApp.vue`, `useGateway.ts` và
`VisionView.vue`. `src/main.ts`/`App.vue` bị exclude vì không thuộc production bundle; comment cũ
“tested via integration” không nên được hiểu là production coverage.

Giới hạn còn lại:

- Widget là hotspot lớn, cần tiếp tục tách theo bounded slice;
- `App.vue`/`index.html` legacy cần quyết định giữ làm demo dev hay xóa trong một change riêng;
- dashboard page switching không có URL/deep-link;
- MockWebAdapter không mô phỏng authorization/persistence;
- session-ticket integration mới có component test và gateway E2E tách rời, chưa có WebDriver
  Tauri end-to-end.

## 7. Acceptance

```powershell
npm run test:coverage -w liva-ui
npm run build -w liva-ui
npx eslint liva-ui/src --max-warnings 0
node scripts/e2e-gateway-ci.mjs
```

Khi sửa kết nối Widget, bắt buộc kiểm: Tauri xin ticket trước socket, reconnect xin ticket mới,
browser vẫn remote, unmount không tạo socket muộn và bootstrap widget không bị 403.
