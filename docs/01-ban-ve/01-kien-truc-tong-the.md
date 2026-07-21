---
title: "Kiến trúc tổng thể"
updated: 2026-07-22
commit: b79233c
status: living
owns:
  - hai-profile-chay
  - so-do-kien-truc-tong-the
covers:
  - data/liva-config.json
  - liva-desktop/src-tauri/src/lib.rs
  - liva-native-core/src/main.rs
  - liva-native-core/src/mcp/server.rs
  - liva-native-core/src/tts/espeak.rs
  - liva-native-core/src/webrtc/frame.rs
  - liva-ui/vite.config.ts
  - liva-ui/src/App.vue
  - liva-ui/src/WidgetApp.vue
  - liva-ui/src/composables/useGateway.ts
  - liva-ui/src/composables/useVoicePipeline.ts
  - liva-ui/src/utils/speakerFrame.ts
  - liva-ui/src/utils/voiceFrame.ts
  - liva-ui/src/workers/hey_liva_weights.json
  - scripts/start_all.ps1
---
# Kiến trúc tổng thể LIVA

[⬆ Mục lục](../README.md) · [◀ Tổng quan hệ thống](00-tong-quan-he-thong.md) · [Giao thức IPC và WebSocket ▶](02-giao-thuc-ipc-va-websocket.md)

---

## 0. ĐỌC TRƯỚC TIÊN — LIVA có HAI PROFILE CHẠY khác nhau

> **Đây là điều quan trọng nhất trong toàn bộ tài liệu kiến trúc.** Mọi câu hỏi kiểu
> "tại sao VAD không hoạt động", "tại sao kết nối `ws://127.0.0.1:8002/ws` bị từ chối",
> "tại sao bot Telegram im lặng" đều quy về một nguyên nhân duy nhất: **bạn đang chạy
> profile nào**.

Cùng một `AppState` và cùng một hàm `handle_command` được **dựng hai lần độc lập** ở hai
điểm vào khác nhau:

- **Điểm vào A — binary standalone:** `liva-native-core/src/main.rs` → chạy tay
  `liva-native-core.exe`. Dựng đầy đủ mọi thành phần.
- **Điểm vào B — vỏ Tauri:** `liva-desktop/src-tauri/src/lib.rs` → chính là cái người dùng
  thật chạy qua `npm run dev`. Dựng một `AppState` **nghèo hơn**.

Nghịch lý cốt lõi: **profile chính thức (Tauri) là profile nghèo hơn.**

### 0.1 Bảng so sánh hai profile

| | `liva-native-core.exe` (chạy tay) | Tauri shell (`npm run dev` → cái người dùng thật chạy) |
|---|---|---|
| WS gateway 8002 | **CÓ** (`main.rs:463`, spawn ở `main.rs:315`) | **KHÔNG** — không gọi `start_websocket_server` |
| VAD / denoise / AEC / turn-shadow | **CÓ** (`main.rs:152-238`) | **`None` hard-code** (`liva-desktop/src-tauri/src/lib.rs:377-380`) |
| WakeGate | **CÓ** (`main.rs:608`) | **KHÔNG** |
| Telegram bot | **CÓ** (`main.rs:336-358`) | **KHÔNG** (grep `telegram` trong `src-tauri/src/` = 0 hit) |
| IPC stdin/stdout | **CÓ** (`main.rs:375-450`) | **KHÔNG** (dùng Tauri `invoke`) |

Đoạn code quyết định ở phía Tauri (`liva-desktop/src-tauri/src/lib.rs:370-384`) — bốn field
thoại bị đặt `None` ngay khi dựng state:

```rust
let state = Arc::new(AppState {
    db,
    crypto: liva_native_core::crypto::EncryptionEngine::new(&encryption_key),
    stt: tokio::sync::Mutex::new(stt_manager),
    tts: tokio::sync::Mutex::new(tts_manager),
    tts_player,
    llm: tokio::sync::Mutex::new(llm_manager),
    vad: tokio::sync::Mutex::new(None),
    denoiser: tokio::sync::Mutex::new(None),
    turn_shadow: tokio::sync::Mutex::new(None),
    aec: tokio::sync::Mutex::new(None),
    mcp_server,
    vision: tokio::sync::Mutex::new(vision_manager),
    embedder: tokio::sync::Mutex::new(embedder),
});
```

> **Cập nhật 22/07/2026 — field thứ 13 `embedder`.** Embedding đã tách khỏi model chat
> thành `liva-native-core/src/llm/embedder.rs` (`EmbeddingEngine`). Khác với bốn field
> thoại vốn là `None` cứng, vỏ Tauri **cũng nạp embedder** thật
> (`liva-desktop/src-tauri/src/lib.rs:359-368`): nếu thư mục model vắng thì chỉ log
> `warn!("Bo nho dai han TAT: …")` rồi để `None`, và RAG im lặng bỏ qua.

### 0.2 Kịch bản khởi động thật sự làm gì

`scripts/start_all.ps1:24` chỉ **kill** tiến trình đang giữ port 8002, rồi chạy `liva-ui`
(dòng 56) và `npx tauri dev --no-dev-server` (dòng 66). **Không dòng nào khởi động binary
lõi.** Trong khi đó vỏ Tauri vẫn `emit("gateway-ready", {"port":8002,"token":null})` kèm
comment sai sự thật *"Gateway is already running on port 8002 (started by start_all.ps1)"*
(`liva-desktop/src-tauri/src/lib.rs:476-480`).

⇒ Toàn bộ đường song công (barge-in, VAD, khử ồn, AEC, wake word phía Rust) thuộc nhóm
**[MỘT PHẦN]**: chỉ sống khi chạy tay binary standalone.

### 0.3 Hệ quả thực tế cần nhớ

| Câu hỏi thường gặp | Câu trả lời theo profile |
|---|---|
| Có mở cổng TCP nào không? | Tauri: **không mở cổng nào**. Standalone: `127.0.0.1:8002/ws` |
| UI nói chuyện với lõi kiểu gì? | Tauri: `invoke("native_ipc_call")` in-process. Standalone: WebSocket JSON + khung nhị phân |
| Có phát hiện im lặng / ngắt lời không? | Tauri: **không** (`vad = None`). Standalone: có |
| Bot Telegram có chạy không? | Tauri: **không**. Standalone: có nếu đặt `TELEGRAM_BOT_TOKEN` |
| Lệnh `handle_command` có khác nhau không? | **Không** — cùng một hàm, cùng 44 nhánh lệnh |

> **Quy ước đọc sơ đồ bên dưới:** nét liền = đường đang chạy thật trong luồng dev chuẩn
> (`npm run dev` → Vue UI ↔ Tauri IPC ↔ core in-process); **nét đứt = opt-in, chạy tay,
> hoặc chưa nối dây.**

---

## 1. Sơ đồ kiến trúc tổng thể

```mermaid
flowchart TB
    subgraph CLIENT["Client"]
        WIDGET["Tauri window: widget.html<br/>(overlay ghost, alwaysOnTop)"]
        DASH["Tauri window: dashboard.html<br/>(1200x800)"]
        UIAPP["liva-ui (Vue 3 + Vite 5173)<br/>useGateway / useVoicePipeline"]
        INDEXHTML["index.html + App.vue<br/>(khong nam trong build)"]
        MOBILE["mobile_client (Capacitor 8)<br/>mic gia = song sin, khong phat TTS"]
    end

    subgraph SHELL["Tauri v2 shell (liva-desktop)"]
        CMDS["8 lenh Tauri: native_ipc_call,<br/>native_ipc_call_stream, toggle_ghost_mode,<br/>update_interactive_zones, open_dashboard,<br/>read/write_vault_key, set_eco_mode"]
        HITTEST["Luong hit-test con tro<br/>(ghost mode)"]
        STRONGHOLD["Stronghold vault<br/>(Argon2id, mat khau hardcode)"]
        ECO["set_eco_mode<br/>(UI khong bao gio goi)"]
    end

    subgraph GATEWAY["Gateway WebSocket 8002 (chi binary standalone)"]
        WS["start_websocket_server<br/>127.0.0.1:8002/ws"]
        TEXTL["Lop TEXT: event JSON + IpcRequest"]
        BINL["Lop BINARY: VoiceFrame<br/>u8 op + u32 seq + u32 size"]
        ACTOR["WebRTCActor / PipelineHandle"]
    end

    subgraph CORE["Rust core (liva-native-core) - AppState"]
        HC["handle_command (44 lenh)"]
        LLM["LlamaRouterManager<br/>(Mutex duy nhat: chat/embed/vision/swap)"]
        STT["SttManager (Nemotron / Parakeet)"]
        TTS["TtsManager + TtsAudioPlayer<br/>(Kokoro / VieNeu / Piper)"]
        VISION["VisionManager + ScreenCapturer"]
        DB["DatabasePool r2d2<br/>writer(1) + readers(4), SQLite WAL"]
        CRYPTO["EncryptionEngine AES-GCM"]
        GOV["Governor game-aware<br/>GPU downshift 5s"]
        VAD["VAD / GTCRN denoise /<br/>SmartTurn shadow / AEC"]
        MCPSRV["NativeMcpServer<br/>(mcp:list_tools / mcp:call_tool)"]
        STDIO["IPC stdin/stdout (dong JSON)"]
    end

    subgraph MODELS["Model Assets (gitignored)"]
        MSTT["models/nemotron-asr (LFS)"]
        MTTS["models/kokoro-v1.0.onnx<br/>+ VieNeu"]
        MLLM["E:/AI_Models/*.gguf<br/>Qwen3-VL / gemma"]
        MVAD["silero VAD / GTCRN / turn"]
        MWAKE["hey_liva_weights.json<br/>(MLP thuan JS trong worker)"]
    end

    subgraph EXT["Dich vu ngoai"]
        TG["Telegram Bot API<br/>(TELEGRAM_BOT_TOKEN)"]
        PYVOICE["liva-voice 8765 (FastAPI)<br/>clone giong, edge-tts, GPT-SoVITS"]
        EDGE["Edge-TTS cloud (Azure)"]
        VAULT["Obsidian vault<br/>(LIVA_VAULT_PATH hardcode)"]
    end

    WIDGET --> UIAPP
    DASH --> UIAPP
    INDEXHTML -.-> UIAPP
    UIAPP -->|"IPC Tauri: invoke native_ipc_call"| CMDS
    UIAPP -->|"IPC Tauri stream: emit ipc-stream:req_id"| CMDS
    WIDGET --> HITTEST
    CMDS --> STRONGHOLD
    ECO -.-> HITTEST
    CMDS -->|"goi truc tiep in-process"| HC

    UIAPP -.->|"WebSocket JSON (chi che do web/dev)"| TEXTL
    UIAPP -.->|"PCM nhi phan VoiceFrame 9 byte (serializeVoiceFrame)"| BINL
    MOBILE -.->|"WebSocket JSON + VoiceFrame nhi phan (adb reverse)"| WS
    WS --> TEXTL
    WS --> BINL
    TEXTL --> HC
    BINL --> ACTOR
    ACTOR --> VAD
    ACTOR --> STT
    ACTOR -->|"PCM nhi phan OP_SPEAKER_OUT"| WS

    HC --> LLM
    HC --> STT
    HC --> TTS
    HC --> VISION
    HC --> DB
    HC --> CRYPTO
    HC --> MCPSRV
    GOV -->|"reload_llm_gpu_layers"| LLM
    STDIO --> HC
    VAD --> STT
    STT --> LLM
    LLM --> TTS

    STT --> MSTT
    TTS --> MTTS
    LLM --> MLLM
    VAD --> MVAD
    UIAPP --> MWAKE

    TG -->|"HTTPS long-poll (chi binary standalone)"| HC
    HC -->|"HTTP telegram:send_text"| TG
    MCPSRV -.-> VAULT
    UIAPP -.->|"HTTP 8765 - khong ai goi, bi CSP chan"| PYVOICE
    PYVOICE -.->|"HTTP cloud"| EDGE
```

---

## 2. Diễn giải từng khối

### 2.1 Khối `CLIENT` — các mặt tiền người dùng

| Nút sơ đồ | Thực chất là gì | Trạng thái |
|---|---|---|
| `WIDGET` — `widget.html` | Cửa sổ Tauri overlay trong suốt, `alwaysOnTop`, có Ghost Mode click-through | **[OK]** |
| `DASH` — `dashboard.html` | Cửa sổ Tauri 1200×800, mở bằng lệnh `open_dashboard` | **[OK]** |
| `UIAPP` — `liva-ui` | Vue 3 + Vite (dev 5173). Hai composable trục: `useGateway` (kênh lệnh) và `useVoicePipeline` (kênh mic) | **[OK]** |
| `INDEXHTML` — `index.html` + `App.vue` | **Không** nằm trong `rollupOptions.input` của `vite.config.ts:18-21` ⇒ không có trong bản build, chỉ chạy được ở `vite dev` | **[MỘT PHẦN]** |
| `MOBILE` — `mobile_client` | PoC Capacitor 8 + Vue 3 (Android), 1 commit duy nhất. Protocol `VoiceFrame` **đúng 9 byte** nhưng mic là sóng sin giả, không phát được TTS | **[MỘT PHẦN]** đóng băng |

Điểm cần nhớ: **cả hai cửa sổ Tauri đều nạp cùng một codebase `liva-ui`**, khác nhau ở entry
HTML. Tauri trỏ `frontendDist` sang `../liva-ui/dist`; app Vite riêng nằm trong
`liva-desktop/` (ngoài `src-tauri`) là **thư mục bỏ hoang** — script `build:desktop` build
đúng cái app vô dụng đó.

### 2.2 Khối `SHELL` — vỏ Tauri v2 (`liva-desktop/src-tauri`)

Vỏ Tauri chỉ có **3 file `.rs`**: `main.rs` (6 dòng), `lib.rs` (593 dòng), `build.rs` (3 dòng).
Nó phơi ra **8 lệnh Tauri**, trong đó chỉ **hai lệnh** là trục kiến trúc:
`native_ipc_call` (cầu chính UI → `handle_command`, gọi **trực tiếp in-process**,
`lib.rs:229-235`) và `native_ipc_call_stream` (biến thể streaming, phát
`window.emit("ipc-stream:{req_id}")`, `lib.rs:237-258`). Sáu lệnh còn lại phục vụ cửa sổ và
vault: `toggle_ghost_mode`, `update_interactive_zones`, `open_dashboard`,
`read_vault_key` / `write_vault_key` (Stronghold, mật khẩu hardcode) và `set_eco_mode`
(**code chết**, xem §3.5).

> 📌 Nguồn đầy đủ: [Frontend và vỏ Tauri](08-frontend-va-vo-tauri.md)

`HITTEST` là một **luồng riêng** liên tục đọc vị trí con trỏ để quyết định cửa sổ widget có
"ăn" sự kiện chuột hay không — đây là cơ chế Ghost Mode end-to-end và nó **chạy thật**.

### 2.3 Khối `GATEWAY` — WebSocket 8002

**Chỉ tồn tại trong binary standalone.** Cấu trúc hai lớp trên cùng một kết nối
`ws://127.0.0.1:8002/ws`:

- **Lớp TEXT** — JSON: sự kiện phát ra cho client + `IpcRequest` đi vào `handle_command`.
- **Lớp BINARY** — `VoiceFrame`: header 9 byte `[op u8][seq_id u32 LE][payload_size u32 LE]`
  rồi tới payload PCM. Đây là kênh mic lên / loa xuống.
- `WebRTCActor` / `PipelineHandle` — actor điều phối vòng đời phiên thoại song công.

> 📌 Nguồn đầy đủ (bảng opcode, chi tiết khung 9 byte, 44 lệnh `handle_command`): [Giao thức IPC và WebSocket](02-giao-thuc-ipc-va-websocket.md)

`start_websocket_server` đọc `LIVA_SERVER_PORT` (mặc định `8002`) và `LIVA_SERVER_HOST`
(mặc định `127.0.0.1`) rồi `TcpListener::bind` (`main.rs:463` trở đi; được spawn ở
`main.rs:315`). Vì vỏ Tauri không gọi
hàm này, **mọi mũi tên từ `liva-ui` vào cụm `GATEWAY` đều là nét đứt**.

### 2.4 Khối `CORE` — lõi Rust `liva-native-core` (`AppState`)

Đây là trái tim dự án. `AppState` (`lib.rs:33-52`) có **13 field**, trong đó **9 field** bọc
`tokio::sync::Mutex` (`stt`, `tts`, `llm`, `vad`, `denoiser`, `turn_shadow`, `aec`, `vision`,
`embedder`). Bốn field còn lại **không** bọc Mutex: `db: DatabasePool` (`lib.rs:34`),
`crypto: EncryptionEngine` (`lib.rs:35`), `tts_player: TtsAudioPlayer` (`lib.rs:38`) — ba cái
này tự khoá bên trong — và `mcp_server: Arc<NativeMcpServer>` (`lib.rs:44`), chỉ bọc `Arc`
vì server MCP là read-only.
~~"toàn bộ field trong `AppState` đều bọc `tokio::sync::Mutex`"~~ — câu này từng đúng ở bản
`AppState` cũ, nhưng nay sai; sửa 22/07/2026.

| Thành phần | Vai trò | Ghi chú then chốt |
|---|---|---|
| `handle_command` | Bộ định tuyến lệnh trung tâm — **44 nhánh** `match` + `_ => Err("Unknown command")` (`lib.rs:320-1601`, nhánh `_` ở `lib.rs:1599`) | Dùng chung cho **cả hai** profile. Đếm thực tế trên code: 44 nhánh, từ `"ping"` (`lib.rs:330`) tới `"mcp:call_tool"` (`lib.rs:1578`) |
| `LlamaRouterManager` | LLM qua `llama.cpp` (`llama-cpp-2`). Một `Mutex` **duy nhất** phục vụ cả chat / embed / vision / swap model | Điểm nghẽn tuần tự hoá lớn nhất của hệ thống |
| `SttManager` | ASR: Nemotron RNN-T (mặc định) hoặc Parakeet-vi (opt-in qua `LIVA_STT_VI_ENGINE`) | ONNX Runtime, CPU |
| `TtsManager` + `TtsAudioPlayer` | TTS: Piper / VieNeu (opt-in) / Kokoro | `models/kokoro-v1.0.onnx` **không tồn tại** mặc định |
| `VisionManager` + `ScreenCapturer` | Chụp màn hình thuần Rust qua Windows Graphics Capture, đưa ảnh vào Qwen3-VL | Cần build **release** |
| `DatabasePool` (r2d2) | SQLite WAL, tách **writer(1) + readers(4)** | 13 bảng thường + 2 bảng ảo |
| `EncryptionEngine` | AES-256-GCM | Chỉ **1 cột** được mã hoá: `facts.value` |
| Governor game-aware | Vòng lặp 5s, hạ `n_gpu_layers` khi phát hiện tải nặng, gọi `reload_llm_gpu_layers` | Win32; early-return nếu `n_gpu_layers` vốn đã bằng 0 |
| `VAD` / GTCRN denoise / SmartTurn shadow / AEC | Cụm xử lý tín hiệu tiếng nói | **`None` trong profile Tauri** |
| `NativeMcpServer` | Server MCP nội bộ | Đã nối vào dispatcher: `mcp:list_tools` (`lib.rs:1575`) và `mcp:call_tool` (`lib.rs:1578`); **chưa client UI nào gọi** — xem điểm 4 của §3 |
| `STDIO` | IPC dòng JSON qua stdin/stdout (`main.rs:375-450`) | Chỉ binary standalone |

Chuỗi xử lý cốt lõi trong sơ đồ: `VAD → STT → LLM → TTS` — đây là đường thoại; và
`handle_command → {LLM, STT, TTS, VISION, DB, CRYPTO}` — đây là đường lệnh.

> 📌 Nguồn đầy đủ cho từng khối: [Đường ống thoại](03-duong-ong-thoai.md) (bảng engine STT,
> bảng backend TTS, ngưỡng VAD/AEC) · [Hệ LLM và prompt](04-he-llm-va-prompt.md) (cấu hình LLM) ·
> [Thị giác, quan sát thụ động và governor](06-thi-giac-passive-va-governor.md) (ngưỡng governor) ·
> [Tầng dữ liệu và bảo mật](07-tang-du-lieu-va-bao-mat.md) (ERD SQLite, 15 bảng, sơ đồ mã hoá)

### 2.5 Khối `MODELS` — trọng số (toàn bộ gitignored)

Về mặt kiến trúc chỉ cần nhớ ba điều: trọng số STT/TTS/VAD nằm trong `models/` (riêng
`models/nemotron-asr` là nested git repo có LFS, **không** phải submodule — để yên), còn
GGUF của LLM/vision nằm **ngoài repo** ở `E:/AI_Models/` và được trỏ qua
`data/liva-config.json`. Kokoro vắng mặt mặc định ⇒ init TTS lỗi cho tới khi cấp file.
Riêng `hey_liva_weights.json` **không phải model Rust**: đó là MLP thuần JavaScript chạy
trong web worker phía UI — nên wake word có **hai bản**, bản JS trong `liva-ui` (nét liền,
chạy thật) và `WakeGate` phía Rust (chỉ có ở binary standalone).

> 📌 Nguồn đầy đủ: [Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md)

### 2.6 Khối `EXT` — dịch vụ ngoài

Bốn điểm chạm ra ngoài lõi, **tất cả đều [MỘT PHẦN]**: Telegram Bot API (long-poll, chỉ
binary standalone), `liva-voice` FastAPI 8765 (khởi động thủ công, **không code nào trong
repo gọi tới** và còn bị CSP chặn), Edge-TTS cloud do `liva-voice` gọi ra — **điểm chạm
Internet duy nhất** của hệ thống, và Obsidian vault (đích của `NativeMcpServer` — nay đã có
hai lệnh dispatcher `mcp:list_tools` / `mcp:call_tool` gọi tới, nhưng chưa client UI nào dùng).

> 📌 Nguồn đầy đủ: [Tích hợp ngoài](09-tich-hop-ngoai.md)

---

## 3. Năm điểm cần biết khi đọc sơ đồ

**Nét liền = đường đang chạy thật; nét đứt = opt-in, chạy tay, hoặc chưa nối dây.**

1. **Đường chạy chính thức là Tauri IPC, không phải WebSocket.** `useGateway.ts:210` kiểm
   `window.__TAURI_INTERNALS__`; nếu có thì `connect()` **return sớm**
   (`useGateway.ts:274-289`), không tạo WebSocket, mọi lệnh đi qua
   `invoke("native_ipc_call", {command, payload})` →
   `liva-desktop/src-tauri/src/lib.rs:229-235` → `handle_command`. Streaming thì qua
   `native_ipc_call_stream` + `window.emit("ipc-stream:{req_id}")` (`lib.rs:237-258`).

2. **Gateway 8002 chỉ tồn tại ở binary standalone.** Đây là lý do mọi mũi tên vào cụm
   `GATEWAY` là nét đứt từ phía `liva-ui`.

3. **Hợp đồng khung mic lên đã được vá (22/07/2026).** `useVoicePipeline.ts` nay import
   `serializeVoiceFrame` / `OP_MIC_IN` từ `liva-ui/src/utils/voiceFrame.ts`
   (`useVoicePipeline.ts:4`) và gửi ở `useVoicePipeline.ts:353`:
   ```ts
   wsRef.send(serializeVoiceFrame(OP_MIC_IN, micSeqId, new Uint8Array(buffer.buffer)));
   ```
   `voiceFrame.ts` đóng đúng header **9 byte** (`VOICE_FRAME_HEADER_SIZE = 9`,
   `view.setUint32(1, seqId >>> 0, true)`, `view.setUint32(5, payload.byteLength, true)`),
   khớp `VoiceFrame::decode` (`webrtc/frame.rs:32-56`) vốn đòi
   `[op u8][seq_id u32 LE][payload_size u32 LE]`.
   > **Bối cảnh lịch sử — bug đã sửa.** Trước đây chỗ này ghi
   > ~~`const msg = new Uint8Array(1 + pcmBuffer.byteLength); msg[0] = 0x01;`~~ tức header
   > **1 byte**; core đọc 4 byte PCM đầu làm `seq_id`, 4 byte kế làm `payload_size` → gần
   > như chắc chắn `>1 MiB` → `Err("Payload exceeds 1MB limit")` (`webrtc/frame.rs:41`) ⇒
   > mọi khung mic từ trình duyệt bị từ chối và barge-in không chạy được. Chuỗi suy luận đó
   > **nay chỉ còn giá trị lịch sử**; comment giải thích chính bug này còn nằm ngay trên
   > dòng gửi (`useVoicePipeline.ts:348-352`).

   Lưu ý đừng nhầm: hai chỗ `msg[0] = 0x02` còn lại (`useVoicePipeline.ts:196`, `:266`) là
   tiền tố 1 byte của nhánh **MessagePack**, một giao thức khác. Chiều **xuống** thì UI parse
   đúng 9 byte (`utils/speakerFrame.ts:5-13`, `App.vue:143-150`), và
   `mobile_client/src/services/WebSocketClient.ts:226-235` (`serializeVoiceFrame`) cũng tạo
   **đúng** 9 byte.

4. **`NativeMcpServer` đã nối vào dispatcher, nhưng chưa có client nào gọi.** Nó nằm trong
   `AppState` (`lib.rs:44`), khởi tạo ở cả `main.rs:171` và `src-tauri/src/lib.rs:349`.
   Từ 22/07/2026 `handle_command` có hai nhánh `mcp:*`: `"mcp:list_tools"` (`lib.rs:1575`,
   gọi thẳng `state.mcp_server.list_tools()`) và `"mcp:call_tool"` (`lib.rs:1578`).
   `list_tools()` (`mcp/server.rs:39`) vì thế có caller thật, cộng thêm test tích hợp
   `liva-native-core/tests/integration_tests.rs:539` ("2.7 — `mcp:list_tools` /
   `mcp:call_tool` phải đi qua `handle_command`").
   ~~"`handle_command` không có nhánh `mcp:*` nào; `list_tools()` có 0 caller kể cả test"~~
   — nhận định cũ, nay sai cả hai vế.
   Phần **vẫn đúng**: `grep "mcp:list_tools" liva-ui/src` = 0 hit ⇒ chưa client UI nào phát
   lệnh này, nên mũi tên `MCPSRV -.-> VAULT` vẫn để nét đứt.

5. **`set_eco_mode` là code chết ở phía UI.** Grep toàn repo (trừ `node_modules`): 2 hit,
   đều là định nghĩa (`lib.rs:82`) và đăng ký (`lib.rs:583`, trong
   `.invoke_handler(tauri::generate_handler![` ở `lib.rs:581`). `WidgetApp.vue:735` chỉ **xử lý
   sự kiện** `eco_mode_changed` từ WS, không hề `invoke('set_eco_mode')` ⇒ `EcoModeState`
   luôn `false`, nhánh eco trong luồng hit-test không bao giờ chạy.

---

## 4. Trạng thái từng thành phần trên sơ đồ

### 4.1 Thành phần hệ thống

Bảng này là **chú giải trạng thái cho sơ đồ §1**: mỗi nút trong sơ đồ ứng với một dòng ở đây,
kèm công nghệ và mức độ "đã nối dây" của nó. Cột tiến trình / cổng đã được lược bỏ khỏi bảng
này để tránh trùng — xem §4.2.

| Thành phần | Công nghệ & ghi chú kiến trúc | Trạng thái |
|---|---|---|
| `liva-native-core` (lõi) | Rust edition 2024 (≥1.85), `llama-cpp-2`, `ort` (ONNX Runtime), `tokio`, `r2d2` + SQLite. Nhúng **in-process** trong `LIVA.exe` | **[OK]** |
| `liva-desktop/src-tauri` (vỏ) | Tauri v2, Rust edition **2021** (lệch với core), version `25.0.0`. **Không mở cổng nào** | **[OK]** |
| `liva-ui` (giao diện) | Vue 3 + Vite, build 2 entry `widget.html` + `dashboard.html`; bản build nạp từ `frontendDist` | **[OK]** |
| WebView2 | `msedgewebview2.exe`, tiến trình con của `LIVA.exe`, 2 cửa sổ | **[OK]** |
| Gateway WebSocket | `tokio-tungstenite`, hai lớp TEXT/BINARY; chỉ có ở binary standalone | **[MỘT PHẦN]** |
| IPC stdin/stdout | Dòng JSON, chỉ binary standalone (`main.rs:375-450`) | **[MỘT PHẦN]** |
| STT | Nemotron RNN-T (ONNX, CPU); Parakeet-vi opt-in | **[OK]** (Parakeet **[MỘT PHẦN]**) |
| TTS | Piper VITS (vi + en), VieNeu (opt-in), Kokoro; shell-out `espeak-ng.exe` cho G2P | **[OK]** (Kokoro **[THIẾU]** — thiếu file model) |
| LLM router | `llama.cpp` GGUF; một `Mutex` duy nhất cho chat/embed/vision/swap | **[OK]** |
| Vision màn hình | Windows Graphics Capture thuần Rust + Qwen3-VL; **cần build release** | **[OK]** |
| VAD / denoise / AEC / turn-shadow | silero VAD v6, GTCRN, smart-turn v3.2 (ONNX); chỉ binary standalone | **[MỘT PHẦN]** |
| Wake word (UI) | MLP thuần JS trong web worker (`hey_liva_weights.json`), chạy trong WebView2 | **[MỘT PHẦN]** — mặc định `Off` |
| `WakeGate` (Rust) | Chỉ `liva-native-core.exe` (`main.rs:608`) | **[MỘT PHẦN]** |
| Governor game-aware | Win32 API, vòng lặp 5s, `reload_llm_gpu_layers` | **[OK]** (early-return khi `n_gpu_layers`=0) |
| Ghost Mode / hit-test | Win32, luồng riêng trong vỏ Tauri (`LIVA.exe`) | **[OK]** |
| Stronghold vault | Tauri plugin Stronghold, Argon2id; file trong `AppData/Local/com.liva.cognitive-os/` | **[MỘT PHẦN]** — mật khẩu hardcode |
| Lưu trữ | SQLite WAL, pool writer(1)+readers(4), `data/agents/liva_core/structured_memory.sqlite` | **[MỘT PHẦN]** — chỉ 4 bảng thường + 2 bảng ảo có writer |
| Mã hoá | AES-256-GCM, khoá từ `LIVA_ENCRYPTION_KEY` | **[MỘT PHẦN]** — chỉ 1 cột `facts.value` |
| `NativeMcpServer` | MCP nội bộ, đích là Obsidian vault; cấp phát ở cả hai điểm vào | **[MỘT PHẦN]** — đã nối dispatcher (`lib.rs:1575`, `lib.rs:1578`), chưa client UI nào gọi |
| Telegram bot | HTTPS long-poll `api.telegram.org`; shell-out `ffmpeg.exe` cho voice | **[MỘT PHẦN]** — chỉ binary standalone |
| `liva-voice` | Python + FastAPI `0.0.0.0:8765`, edge-tts / GPT-SoVITS, khởi động thủ công | **[MỘT PHẦN]** — không auth/CORS/rate-limit |
| `mobile_client` | Capacitor 8 + Vue 3 (Android), `adb reverse` | **[MỘT PHẦN]** — mic giả |
| `packages/liva-common` | Type TS dùng chung, không build | **[MỘT PHẦN]** — hợp đồng đã trôi khỏi core |

> 📌 Nguồn đầy đủ về LOC và phụ thuộc từng module: [Phụ thuộc module và tra cứu file](10-phu-thuoc-module-va-tra-cuu.md)

### 4.2 Tiến trình và cổng — bản rút gọn

Ở luồng dev chuẩn chỉ có **hai tiến trình bắt buộc**: `node`/Vite dev cho `liva-ui`
(`127.0.0.1:5173`) và **`LIVA.exe`** (Tauri v2 + core nhúng, **không mở cổng nào**, UI↔core
qua `invoke`). Mọi tiến trình còn lại đều tuỳ chọn hoặc chạy tay: `liva-native-core.exe`
standalone (mở `ws://127.0.0.1:8002/ws` + stdio IPC), `liva-voice` (`0.0.0.0:8765`),
`TelegramBotManager`, cùng hai binary shell-out `espeak-ng.exe` và `ffmpeg.exe`.

> 📌 Nguồn đầy đủ (lệnh khởi động, phụ thuộc, cách chạy đúng): [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md)

---

## 5. Ba lát cắt kiến trúc đáng chú ý

1. **Không có ranh giới tiến trình giữa UI và lõi ở profile chuẩn.** Vỏ Tauri gọi
   `handle_command` **trực tiếp in-process**. Điều đó cho độ trễ gần bằng 0 nhưng cũng nghĩa
   là một panic trong lõi sẽ hạ luôn cả cửa sổ ứng dụng, và không có sandbox giữa hai lớp.

2. **Một `Mutex` duy nhất cho LLM.** `LlamaRouterManager` phục vụ chat, embed, vision và
   swap model qua cùng một khoá ⇒ mọi tác vụ LLM xếp hàng tuần tự. Đây là điểm nghẽn cần
   biết trước khi thiết kế bất kỳ tính năng "chủ động" nào chạy nền.

3. **Đường dữ liệu và đường lệnh đã tách, nhưng đường bộ nhớ mới nối một phần.** Sơ đồ cho
   thấy `HC → DB` là nét liền; đếm câu `INSERT` thật trong Rust chỉ được **4 bảng thường**
   — `facts` (`db.rs:477`), `vectors_meta` (`db.rs:608`), `tasks` (`lib.rs:700`),
   `agent_checkpoints` (`agent/memory.rs:24`) — cộng **2 bảng ảo** `vec_idx` (`db.rs:655`)
   và `vectors_fts` (`db.rs:661`). Đường ghi vector là đường **có thật trong production**,
   gọi được qua nhánh `"memory:upsert_vector"` của `handle_command` (`lib.rs:1168`). Phần
   còn lại của schema bộ nhớ dài hạn vẫn là **schema rỗng**.
   > ~~"chỉ 3/15 bảng thực sự có câu `INSERT` (`facts`, `tasks`, `agent_checkpoints`)"~~ —
   > bản đếm cũ bỏ sót nhánh vector; sửa 22/07/2026.
   > 📌 Nguồn đầy đủ: [Tầng dữ liệu và bảo mật](07-tang-du-lieu-va-bao-mat.md)

---

## Liên quan

**Đọc tiếp theo mạch:** [⬆ Mục lục](../README.md) · [◀ Tổng quan hệ thống](00-tong-quan-he-thong.md) · [Giao thức IPC và WebSocket ▶](02-giao-thuc-ipc-va-websocket.md)

**Tài liệu này dựa vào (nguồn sự thật ở nơi khác):**

- [Tổng quan hệ thống](00-tong-quan-he-thong.md) — bảng chỉ số dự án và bản đồ workspace mà sơ đồ §1 vẽ lại
- [Giao thức IPC và WebSocket](02-giao-thuc-ipc-va-websocket.md) — khung nhị phân 9 byte, bảng opcode, 44 lệnh `handle_command` (§2.3, §3.3)
- [Đường ống thoại](03-duong-ong-thoai.md) — bảng engine STT, bảng backend TTS, ngưỡng VAD/AEC/denoise (§2.4, §4.1)
- [Hệ LLM và prompt](04-he-llm-va-prompt.md) — cấu hình LLM (`n_ctx`, `n_gpu_layers`, model router)
- [Thị giác, quan sát thụ động và governor](06-thi-giac-passive-va-governor.md) — ngưỡng và hành vi governor game-aware
- [Tầng dữ liệu và bảo mật](07-tang-du-lieu-va-bao-mat.md) — ERD SQLite, 15 bảng, sơ đồ mã hoá (§4.1, §5.3)
- [Frontend và vỏ Tauri](08-frontend-va-vo-tauri.md) — bảng 8 lệnh Tauri, cấu hình cửa sổ, bảng màn hình dashboard (§2.1, §2.2)
- [Tích hợp ngoài](09-tich-hop-ngoai.md) — bảng tích hợp Telegram / `liva-voice` / Edge-TTS / Obsidian (§2.6)
- [Phụ thuộc module và tra cứu file](10-phu-thuoc-module-va-tra-cuu.md) — LOC và phụ thuộc từng module (§4.1)
- [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) — mọi biến `LIVA_*` và `data/liva-config.json`
- [Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) — bảng model và tài nguyên RAM/VRAM (§2.5)
- [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md) — bảng tiến trình, cổng, cách chạy đúng (§4.2)
- [Báo cáo khảo sát kiến trúc gốc 2026-07](../03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) — dữ liệu khảo sát gốc (đóng băng)

**Tài liệu khác dựa vào tài liệu này:**

- [Mục lục bộ tài liệu](../README.md) — trích cảnh báo "hai profile chạy" ngay ở đầu trang
- [Giao thức IPC và WebSocket](02-giao-thuc-ipc-va-websocket.md) — lấy ranh giới profile để giải thích vì sao WS 8002 vắng mặt ở Tauri
- [Đường ống thoại](03-duong-ong-thoai.md) — lấy sự thật "VAD/AEC/denoise/turn-shadow = `None` ở profile Tauri"
- [Frontend và vỏ Tauri](08-frontend-va-vo-tauri.md) — lấy sơ đồ tổng thể để định vị vỏ Tauri trong hệ
- [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md) — lấy hai profile để mô tả cách chạy đúng
- [Đối chiếu tuyên bố và thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — lấy trạng thái [OK]/[MỘT PHẦN]/[THIẾU] ở §4.1
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — lấy 5 điểm ở §3 (lịch sử lệch header 9 byte, MCP chưa có client UI, `set_eco_mode` chết)

**Khi sửa code sau đây thì phải cập nhật tài liệu này:**

- `liva-native-core/src/main.rs` — điểm vào A; đổi thứ tự dựng state hoặc bỏ/thêm thành phần ⇒ sai §0.1, §2.3, §4.1
- `liva-desktop/src-tauri/src/lib.rs` — điểm vào B; đổi 4 field `None` hoặc danh sách lệnh Tauri ⇒ sai §0.1, §2.2
- `scripts/start_all.ps1` — kịch bản khởi động; đổi tiến trình được bật ⇒ sai §0.2, §4.2
- `liva-native-core/src/webrtc/frame.rs` — định nghĩa `VoiceFrame`; đổi header ⇒ sai §2.3 và điểm 3 của §3
- `liva-ui/src/composables/useGateway.ts` — chọn kênh Tauri IPC vs WebSocket ⇒ sai điểm 1 của §3
- `liva-ui/src/composables/useVoicePipeline.ts` + `liva-ui/src/utils/speakerFrame.ts` — hợp đồng khung mic lên / loa xuống ⇒ sai điểm 3 của §3
- `liva-native-core/src/mcp/server.rs` — nếu đổi danh sách tool hoặc hai nhánh `mcp:*` trong `handle_command` ⇒ sai điểm 4 của §3 và §4.1
- `data/liva-config.json` + `liva-ui/vite.config.ts` — đường dẫn model và danh sách entry HTML ⇒ sai §2.1, §2.5
