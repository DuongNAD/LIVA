---
title: "Giao thức IPC và WebSocket"
updated: 2026-07-21
commit: 73edb9b
status: living
owns:
  - bang-42-lenh-handle-command
  - khung-nhi-phan-9-byte
  - bang-opcode
covers:
  - Cargo.toml
  - data/liva-config.json
  - data/user_profile.json
  - liva-desktop/src-tauri/src/lib.rs
  - liva-native-core/src/*
  - liva-native-core/src/bin/verify_duplex.rs
  - liva-native-core/src/webrtc/*
  - liva-ui/src/App.vue
  - liva-ui/src/composables/*
  - liva-ui/src/utils/speakerFrame.ts
  - scripts/start_all.ps1
---
# Giao thức IPC và WebSocket

[⬆ Mục lục](../README.md) · [◀ Kiến trúc tổng thể](01-kien-truc-tong-the.md) · [Đường ống thoại ▶](03-duong-ong-thoai.md)

---

> **Đây là HỢP ĐỒNG GIAO THỨC.** Bất kỳ ai viết client cho LIVA (web, Tauri, mobile, CLI, thiết bị nhúng) đều phải theo đúng tài liệu này. Mọi mục đều được trích dẫn tới dòng code thật; chỗ nào code lệch với thiết kế gốc đều được nêu rõ ở §10.

---

## Mục lục

1. [Phạm vi & hai điểm vào](#1-phạm-vi--hai-điểm-vào)
2. [`AppState` — trạng thái dùng chung](#2-appstate--trạng-thái-dùng-chung)
3. [Vòng đời khởi động — 25 bước](#3-vòng-đời-khởi-động--25-bước)
4. [Kênh vận chuyển: WebSocket server & stdio IPC](#4-kênh-vận-chuyển-websocket-server--stdio-ipc)
5. [Lớp nhị phân — khung `VoiceFrame`](#5-lớp-nhị-phân--khung-voiceframe)
6. [Lớp text — hai giao thức trên cùng một socket](#6-lớp-text--hai-giao-thức-trên-cùng-một-socket)
7. [`handle_command` — bảng 42 lệnh đầy đủ](#7-handle_command--bảng-42-lệnh-đầy-đủ)
8. [Khung streaming — hai định dạng khác nhau](#8-khung-streaming--hai-định-dạng-khác-nhau)
9. [Lệnh UI gửi mà core không có handler](#9-lệnh-ui-gửi-mà-core-không-có-handler)
10. [Đối chiếu THIẾT KẾ GỐC vs AS-BUILT](#10-đối-chiếu-thiết-kế-gốc-vs-as-built)
11. [Checklist cho người viết client](#11-checklist-cho-người-viết-client)

---

## 1. Phạm vi & hai điểm vào

Cùng một `AppState` + `handle_command` được dựng **hai lần độc lập** ở hai binary khác nhau. Đây là điều quan trọng nhất phải nắm trước khi đọc phần giao thức, vì **không phải điểm vào nào cũng mở WebSocket**. Rút gọn ở góc nhìn giao thức:

- **`liva-native-core`** (bin standalone, `main.rs:30` `fn main()`) — **CÓ** gateway WS 8002 (`start_websocket_server`, `main.rs:446`), **CÓ** stdio IPC (`main.rs:358-433`), có đủ VAD/denoise/AEC/turn-shadow và Telegram.
- **`liva-desktop`** (vỏ Tauri, `lib.rs:261` `pub fn run()`) — **KHÔNG** mở WS, **KHÔNG** dùng stdio (chỉ Tauri `invoke`), và `vad/denoiser/turn_shadow/aec` hard-code `None` (`lib.rs:362-365`).
- Luồng dev chuẩn (`npm run dev` → `scripts/start_all.ps1`) **KHÔNG khởi động binary `liva-native-core`** ⇒ **gateway WebSocket 8002 không chạy**; vỏ Tauri vẫn `emit("gateway-ready", {"port": 8002, "token": null})` (`lib.rs:461-464`) kèm comment sai sự thật ("Gateway is already running on port 8002 (started by start_all.ps1)").

> 📌 Nguồn đầy đủ (bảng so sánh hai profile chạy): [Kiến trúc tổng thể](01-kien-truc-tong-the.md) — cách chạy từng profile: [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md)

⇒ **Toàn bộ đường voice duplex nhị phân** (OP_MIC_IN → VAD → barge-in → OP_SPEAKER_OUT) chỉ sống khi chạy binary `liva-native-core` **thủ công**. Trạng thái: **[MỘT PHẦN]**.

### 1.1 Ba kênh IPC tồn tại trong repo

| Kênh | Điểm vào | Định dạng | Trạng thái |
|---|---|---|---|
| WebSocket `ws://127.0.0.1:8002/ws` | `main.rs:446-1037` | nhị phân `VoiceFrame` + text (2 lớp) | **[MỘT PHẦN]** — chỉ khi chạy binary standalone |
| stdin/stdout dòng-JSON | `main.rs:358-433` (đọc), `main.rs:344-356` (ghi) | `IpcRequest` → `IpcResponse`, mỗi bản ghi 1 dòng + `\n` + flush | **[OK]** trong binary standalone |
| Tauri `invoke` | `liva-desktop/src-tauri/src/lib.rs:228-258` | `native_ipc_call` / `native_ipc_call_stream` | **[OK]** — đây là kênh UI desktop thật đang dùng |

Cả ba kênh cuối cùng đều đổ vào **cùng một** `handle_command` (§7).

### 1.2 Sơ đồ tổng thể các kênh

```mermaid
flowchart LR
    subgraph CLIENTS["Client"]
        UI["liva-ui (Vue 3)<br/>qua Tauri invoke"]
        MOB["mobile_client (TS)<br/>qua WebSocket"]
        CLI["Tooling / test<br/>qua stdin-stdout"]
    end

    subgraph CORE["liva-native-core"]
        WS["start_websocket_server<br/>main.rs:446"]
        STDIO["Vòng đọc stdin<br/>main.rs:358-433"]
        TAURI["native_ipc_call(_stream)<br/>src-tauri/lib.rs:228-258"]
        HC["handle_command<br/>lib.rs:236 — 42 lệnh"]
        ACT["WebRTCActor<br/>webrtc/pipeline.rs"]
        ST["Arc&lt;AppState&gt;<br/>lib.rs:33-46"]
    end

    UI --> TAURI --> HC
    MOB -->|"text: IpcRequest / event"| WS --> HC
    MOB -->|"binary: VoiceFrame"| WS --> ACT
    CLI --> STDIO --> HC

    HC --> ST
    ACT --> ST

    classDef partial fill:#3a3222,stroke:#a08050,color:#e8dcc8
    class WS,ACT partial
```

---

## 2. `AppState` — trạng thái dùng chung

`liva-native-core/src/lib.rs:33-46` — **đủ 12 field, kèm kiểu**:

```rust
pub struct AppState {
    pub db: DatabasePool,                                                     // r2d2: .writer + .readers pools
    pub crypto: EncryptionEngine,                                             // AES-GCM, KHÔNG khoá
    pub stt: tokio::sync::Mutex<SttManager>,
    pub tts: tokio::sync::Mutex<Option<TtsManager>>,
    pub tts_player: TtsAudioPlayer,                                           // KHÔNG khoá (nội bộ tự khoá)
    pub llm: tokio::sync::Mutex<LlamaRouterManager>,
    pub vad: tokio::sync::Mutex<Option<webrtc::vad::VadEngine>>,
    pub denoiser: tokio::sync::Mutex<Option<webrtc::denoise::GtcrnDenoiser>>,
    pub turn_shadow: tokio::sync::Mutex<Option<webrtc::turn_shadow::SmartTurnClassifier>>,
    pub aec: tokio::sync::Mutex<Option<webrtc::aec::SelfEchoCanceller>>,
    pub mcp_server: Arc<mcp::server::NativeMcpServer>,
    pub vision: tokio::sync::Mutex<VisionManager>,
}
```

| # | Field | Kiểu | Có khoá? | Ghi chú |
|---:|---|---|---|---|
| 1 | `db` | `DatabasePool` | không (pool riêng) | `writer` `max_size(1)` + `readers` `max_size(4)` mở `SQLITE_OPEN_READ_ONLY` — `db.rs:131-157` |
| 2 | `crypto` | `EncryptionEngine` | không | AES-256-GCM, chỉ dùng cho `facts.value` |
| 3 | `stt` | `tokio::sync::Mutex<SttManager>` | có | Nemotron RNN-T / Parakeet CTC |
| 4 | `tts` | `tokio::sync::Mutex<Option<TtsManager>>` | có | `None` nếu model TTS thiếu |
| 5 | `tts_player` | `TtsAudioPlayer` | không | tự khoá bên trong |
| 6 | `llm` | `tokio::sync::Mutex<LlamaRouterManager>` | có | **một** Mutex cho chat + embed + vision + swap |
| 7 | `vad` | `tokio::sync::Mutex<Option<VadEngine>>` | có | `None` trong Tauri |
| 8 | `denoiser` | `tokio::sync::Mutex<Option<GtcrnDenoiser>>` | có | `None` trong Tauri |
| 9 | `turn_shadow` | `tokio::sync::Mutex<Option<SmartTurnClassifier>>` | có | `None` trong Tauri |
| 10 | `aec` | `tokio::sync::Mutex<Option<SelfEchoCanceller>>` | có | `None` trong Tauri |
| 11 | `mcp_server` | `Arc<NativeMcpServer>` | không | **chỉ khởi tạo** — không nhánh `mcp:*` nào trong `handle_command` |
| 12 | `vision` | `tokio::sync::Mutex<VisionManager>` | có | WGC qua `xcap` |

Đặc điểm quan trọng đối với người viết client:

- **Toàn bộ dùng `tokio::sync::Mutex`, không có `RwLock` nào.** Không có `Arc` bên trong trừ `mcp_server`.
- Chia sẻ bằng `Arc<AppState>` clone cho từng task (`main.rs:258, 269, 296, 304, 329, 392`) và **cho mỗi kết nối WS** (`main.rs:460`).
- Trong `spawn_blocking` dùng `blocking_lock()` (`main.rs:611,617,625`; `lib.rs:779,1197,1354,1410`).
- **Điểm nghẽn kiến trúc:** `state.llm` là **một** Mutex duy nhất cho chat + embed + vision + swap_model. Một lượt sinh token (blocking) khoá luôn mọi lệnh LLM khác ⇒ client **không nên** phát song song `chat:completion` và `vision:ask`.
- **Engine audio là toàn cục, không per-session.** `vad`/`denoiser`/`aec`/`turn_shadow` mang state hồi quy dòng chảy và không có code phân vùng theo session ⇒ **hai client WS đồng thời sẽ trộn stream vào cùng state**. Hệ quả cho người viết client: **giao thức hiện tại chỉ an toàn với MỘT client voice tại một thời điểm.**

  > 📌 Nguồn đầy đủ (chi tiết state hồi quy từng engine, `reset()` không được gọi ở đường chạy thật): [Đường ống thoại](03-duong-ong-thoai.md)

---

## 3. Vòng đời khởi động — 25 bước

### 3.1 `fn main()` — dựng runtime thủ công

`liva-native-core/src/main.rs:30-49` — **không** dùng `#[tokio::main]`:

| Bước | Việc | Dòng | Mặc định |
|---|---|---|---|
| 1 | `LIVA_TOKIO_WORKER_THREADS` | 31-34 | `available_parallelism()` → fallback 4 |
| 2 | `LIVA_TOKIO_MAX_BLOCKING_THREADS` | 36-39 | **512** |
| 3 | `Builder::new_multi_thread().enable_all().build()` → `rt.block_on(async_main())` | 41-48 | |

### 3.2 `async_main()` — 25 bước, thứ tự chính xác

`liva-native-core/src/main.rs:51-442`:

| # | Việc | Dòng | Ghi chú lỗi |
|---|---|---|---|
| 1 | `FmtSubscriber` level INFO, **writer = stderr** | 53-57 | stdout dành riêng cho IPC |
| 2 | Đọc `LIVA_DB_PATH`, `LIVA_ENCRYPTION_KEY`; `create_dir_all(parent)` | 61-68 | key thiếu → fallback `"0"×32` |
| 3 | `LIVA_DB_IN_MEMORY` (chỉ cần *tồn tại*) → `DatabasePool::new_in_memory()` else `DatabasePool::new(&db_path)` | 70-75 | **`.expect()` — panic nếu lỗi** |
| 4 | `rodio::OutputStream::try_default()` + `Sink::try_new` | 77-90 | lỗi → `None`, không fatal |
| 5 | Resolve 3 đường model qua `resolve_resource_path` | 94-111 | thử prefix `""`, `".."`, `"../.."` |
| 6 | `stt::SttManager::new(&stt_model_dir)` | 113 | |
| 7 | `TtsAudioPlayer::new(shared_sink.clone())` + `TtsManager::from_bin(...)` | 115-125 | lỗi → `None` + log error |
| 8 | `LIVA_LLM_N_CTX` (4096), `LIVA_LLM_N_GPU_LAYERS` (0) → `LlamaRouterManager::new` | 127-136 | **`.expect()` — panic nếu lỗi** |
| 9 | `governor::Governor::from_env()` + **`std::thread`** poll `game_mode_active()` mỗi 5s | 140-149 | |
| 10 | VAD: `webrtc::vad::resolve_model_path(&stt_model_dir)` → `VadEngine::new(path, VadConfig::from_env())` | 152-164 | không có file → `None` |
| 11 | `LIVA_VAULT_PATH` → `NativeMcpServer::new(&vault_path)` | 166-168 | |
| 12 | `NativeScreenCapturer::new(0)` → `VisionManager::new(..., VisionConfig::default())` | 170-174 | hard-code display 0 |
| 13 | GTCRN denoise — **BẬT mặc định**, tắt bằng `LIVA_DENOISE_ENABLED=0/false/off` | 181-209 | |
| 14 | Smart Turn shadow — **opt-in** `LIVA_TURN_SHADOW_ENABLED=1` | 214-230 | |
| 15 | AEC — **opt-in** `LIVA_AEC_ENABLED=1` | 234-238 | |
| 16 | `Arc::new(AppState { … })` | 240-253 | |
| 17 | `tokio::spawn(load_configured_router_model(state, false))` — autoload router LLM | 258-260 | |
| 18 | `tokio::spawn` vòng lặp GPU downshift game-aware (`LIVA_GAME_N_GPU_LAYERS`, mặc định 0) | 268-293 | **early-return nếu `normal_layers == 0`** |
| 19 | `tokio::spawn(start_websocket_server(state))` | 296-301 | ⇐ **giao thức WS bắt đầu sống từ đây** |
| 20 | `tokio::spawn` interval 60s → `tts_mgr.check_idle_unload()` | 304-314 | Tauri **không** có bước này |
| 21 | `mpsc::channel::<String>(100)` (tx/rx cho stdout) | 317 | |
| 22 | Telegram: `TELEGRAM_BOT_TOKEN` + `TELEGRAM_ALLOWED_IDS` (CSV) → `TelegramBotManager::new(...).start()` | 320-341 | bỏ qua nếu không có token |
| 23 | Task ghi stdout: mỗi msg + `\n` + `flush` | 344-356 | |
| 24 | Vòng lặp đọc stdin line-by-line → parse `IpcRequest` → `tokio::spawn(handle_command(...))` | 358-433 | |
| 25 | `drop(tx)` → `writer_handle.await` → log shutdown | 436-441 | |

Bảng trên chỉ ghi giá trị mặc định **tại đúng chỗ nó được đọc trong `main.rs`**, đủ để hiểu thứ tự khởi động; nó không phải danh mục biến môi trường.

> 📌 Nguồn đầy đủ (bảng biến môi trường, lệch `.env.example` vs code): [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md)

### 3.3 `pub fn run()` — Tauri shell (khác biệt)

`liva-desktop/src-tauri/src/lib.rs:261-577`. Trình tự gần giống nhưng:

- `tracing_subscriber::fmt()...try_init()` (`lib.rs:264-266`) — comment ghi rõ không có subscriber thì log của core bị nuốt.
- `AppState` dựng ở `lib.rs:355-368` với **`vad/denoiser/turn_shadow/aec = Mutex::new(None)`** hard-code.
- `std::mem::forget(_stream)` (`lib.rs:372-374`) giữ `rodio::OutputStream` sống vĩnh viễn.
- 4 luồng nền: (A) autoload router LLM (`:402-405`), (B) GPU downshift 5s (`:413-439`), (C) governor priority thread 5s (`:452-457`), (D) hit-test con trỏ 30ms cho ghost mode (`:468-560`).
- **Không** spawn `start_websocket_server`; **không** có task unload TTS idle 60s.
- Có thêm `tauri_plugin_stronghold` (khoá Argon2id từ `LIVA_STRONGHOLD_PASSWORD`/`LIVA_STRONGHOLD_SALT`, mặc định hard-code — `lib.rs:123-129`).

> 📌 Nguồn đầy đủ (bảng lệnh Tauri `invoke`, cấu hình cửa sổ, ghost mode): [Frontend và vỏ Tauri](08-frontend-va-vo-tauri.md)

---

## 4. Kênh vận chuyển: WebSocket server & stdio IPC

### 4.1 Server và handshake

`async fn start_websocket_server(state: Arc<AppState>) -> Result<(), String>` — `main.rs:446-492`.

| Thuộc tính | Giá trị | Dòng |
|---|---|---|
| Bind | `LIVA_SERVER_HOST:LIVA_SERVER_PORT` = `127.0.0.1:8002` | 451-452 |
| Endpoint | `/ws` | 462-481 |
| Log | `WebSocket server listening on ws://{addr}/ws` | — |
| Kiểm path | `accept_hdr_async` callback **chỉ kiểm** `req.uri().path() == "/ws"`; path khác thì handshake **vẫn hoàn tất rồi mới đóng** | 462-481 |
| Auth | **Không kiểm `Origin`, không token, không TLS** | — |

⚠️ **Cảnh báo bảo mật:** `OP_AUTH_HANDSHAKE` chỉ echo lại payload (§5.3), tức là **không có xác thực ở bất kỳ tầng nào**. Bất kỳ tiến trình local nào cũng có thể mở socket và phát lệnh — bao gồm `llm:swap_model` (không validate đường dẫn) và `telegram:send_text`.

### 4.2 `handle_ws_connection` — vòng đời một kết nối

`async fn handle_ws_connection(ws_stream: WebSocketStream<TcpStream>, state: Arc<AppState>) -> Result<(), String>` — `main.rs:494-1037`:

1. `ws_stream.split()` → `ws_sender` / `ws_receiver`.
2. Hai kênh ra: `mpsc::channel::<VoiceFrame>(128)` (`outgoing_tx`) và `mpsc::channel::<String>(128)` (`text_tx`) — `main.rs:505-506`.
3. `WebRTCActor::new(state.clone(), outgoing_tx.clone())` → `(WebRTCPipelineHandle, WebRTCActor)`; `spawn(actor.run())` — `main.rs:509-510`.
4. `send_task`: `tokio::select!` giữa `outgoing_rx` (→ `Message::Binary(frame.encode()?)`) và `text_rx` (→ `Message::Text`) — `main.rs:513-547`. **Đây là chỗ multiplex nhị phân + JSON trên cùng một socket.**
5. State cục bộ mỗi kết nối: `accumulating: bool`, `audio_buffer: Vec<f32>`, `wake_gate = wake::WakeGate::from_env()` — `main.rs:549-551`.
6. Vòng `while let Some(msg_res) = ws_receiver.next().await` với 3 nhánh: `Binary` (§5), `Text` (§6), `Close` → break.
7. Cleanup: `pipeline_handle.on_interrupted()`, `send_task.abort()`, `actor_handle.abort()` — `main.rs:1033-1035`.

```mermaid
sequenceDiagram
    participant C as Client
    participant R as ws_receiver
    participant A as WebRTCActor
    participant H as handle_command
    participant S as send_task (select!)

    C->>R: Message::Binary (VoiceFrame stream)
    R->>R: while len>=9 { VoiceFrame::decode }
    R->>A: on_vad_start / on_vad_end (qua PipelineHandle)
    A-->>S: outgoing_tx: OP_SPEAKER_OUT / OP_FLUSH
    S-->>C: Message::Binary

    C->>R: Message::Text (event | IpcRequest)
    R->>H: tokio::spawn(handle_command)
    H-->>S: text_tx: JSON
    S-->>C: Message::Text
```

### 4.3 stdio IPC — cùng schema, khác vận chuyển

- **Vào:** mỗi dòng stdin là một JSON `IpcRequest` (`main.rs:358-433`, parse ở `:372`).
- **Ra:** mỗi phản hồi là một JSON `IpcResponse` + `\n` + flush (`main.rs:344-356`).
- **stdout là kênh dữ liệu thuần** — log đi stderr (bước 1 §3.2). Client **không** được kỳ vọng log trên stdout.

Struct dây (`main.rs:13-28`):

```rust
#[derive(Debug, Deserialize)]
struct IpcRequest {
    id: String,
    command: String,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct IpcResponse {
    id: String,
    status: String,                                       // "ok" | "error"
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}
```

> **Lưu ý dây quan trọng:** `data` và `error` dùng `skip_serializing_if = "Option::is_none"` ⇒ khi thành công, trường `error` **hoàn toàn vắng mặt** (không phải `null`). Thiết kế gốc ghi `"error": null` — client phải chấp nhận cả hai (§10).

---

## 5. Lớp nhị phân — khung `VoiceFrame`

### 5.1 Định nghĩa

`liva-native-core/src/webrtc/frame.rs` (54 dòng, toàn bộ):

```rust
pub const OP_AUTH_HANDSHAKE: u8 = 0x00;
pub const OP_MIC_IN: u8 = 0x01;
pub const OP_SPEAKER_OUT: u8 = 0x02;
pub const OP_FLUSH: u8 = 0x03;
pub const OP_ACK_PLAYING: u8 = 0x04;

pub struct VoiceFrame { pub op_code: u8, pub seq_id: u32, pub payload: Bytes }
impl VoiceFrame {
    pub fn encode(&self) -> Result<Bytes, String>;                 // giới hạn payload 1 MiB
    pub fn decode(src: &mut BytesMut) -> Result<Option<Self>, String>;
}
```

### 5.2 Sơ đồ header 9 byte

Header **9 byte, little-endian**, rồi payload thô:

```
 byte 0     byte 1   byte 2   byte 3   byte 4
+---------+--------+--------+--------+--------+
| op_code |          seq_id  (u32 LE)         |
+---------+--------+--------+--------+--------+
 byte 5     byte 6   byte 7   byte 8
+--------+--------+--------+--------+
|        payload_len (u32 LE)       |
+--------+--------+--------+--------+
 byte 9 …
+-----------------------------------------------+
|            payload (payload_len byte)          |
+-----------------------------------------------+
```

Quy tắc mã hoá / giải mã (`frame.rs:17-53`):

| Quy tắc | Chi tiết | Dòng |
|---|---|---|
| Giới hạn payload khi **encode** | `payload.len() > 1024*1024` → `Err("Payload exceeds 1MB limit")` | 18-20 |
| Thứ tự ghi | `put_u8(op_code)` → `put_u32_le(seq_id)` → `put_u32_le(len)` → `put_slice(payload)` | 22-25 |
| Thiếu header khi **decode** | `src.len() < 9` → `Ok(None)` (chưa đủ khung) | 30-32 |
| Giới hạn payload khi **decode** | `payload_size > 1024*1024` → `Err` | 37-39 |
| Thiếu payload | `src.len() < 9 + payload_size` → `Ok(None)` | 41-43 |
| Tiêu thụ | `src.advance(9)` + `src.split_to(payload_size)` | 45-46 |

> **1 MiB = 1.048.576 byte**, áp dụng cho **payload**, không tính 9 byte header. Với PCM f32 mono 16 kHz, 1 MiB ≈ 262.144 mẫu ≈ 16,4 giây audio — thực tế client nên gửi chunk ~20-100 ms.

**Framing kiểu stream:** server đọc trong vòng `while bytes_mut.len() >= 9 { VoiceFrame::decode(...) }` (`main.rs:568-578`) ⇒ **nhiều `VoiceFrame` có thể nằm trong một WebSocket binary message**, và một khung dở dang sẽ bị bỏ (`Ok(None)` → `break`) chứ **không** được nối sang message kế tiếp. ⇒ **Client PHẢI gửi trọn vẹn từng khung trong một WS message** (hoặc gửi nhiều khung nguyên vẹn trong một message), không được cắt khung ngang giữa hai message.

### 5.3 Bảng 5 opcode — đầy đủ

| Op | Hex | Hướng | Payload | Server xử lý | Client xử lý | Trạng thái |
|---|---|---|---|---|---|---|
| `OP_AUTH_HANDSHAKE` | `0x00` | C↔S | tuỳ ý (mobile gửi chuỗi UTF-8 `"auth_token"`) | **Echo nguyên payload + nguyên `seq_id`** (`main.rs:580-588`) — **không xác thực gì** | `mobile_client/src/services/WebSocketClient.ts:185` `sendAuthHandshake` (chờ frame `op=0x00` cùng `seqId`) | **[MỘT PHẦN]** chạy nhưng vô nghĩa về bảo mật |
| `OP_MIC_IN` | `0x01` | C→S | PCM **f32 LE mono 16 kHz** thô, **không** header sample-rate | Cắt cho chia hết 4 (`len_rounded = (len/4)*4`, `main.rs:591`), `bytemuck::cast_slice` nếu con trỏ căn 4-byte, ngược lại decode thủ công `f32::from_le_bytes` (`main.rs:593-600`). Chuỗi trong **một** `spawn_blocking`: AEC → GTCRN → VAD (`main.rs:608-635`) | `liva-ui/src/composables/useVoicePipeline.ts:345-350` — **SAI: header 1 byte**; `mobile_client` đúng 9 byte | **[MỘT PHẦN]** — server đúng, `liva-ui` sai hợp đồng |
| `OP_SPEAKER_OUT` | `0x02` | S→C | `[u32 LE sample_rate][f32 LE PCM…]` | `webrtc/pipeline.rs:376-388`; `sample_rate` = 24000 (Kokoro, `pipeline.rs:357`) / 22050 (Piper) / `v.sample_rate()` (VieNeu, `pipeline.rs:341`). `seq_id` tăng dần, reset 0 mỗi `spawn_llm_and_tts` (`pipeline.rs:303`) | `liva-ui/src/utils/speakerFrame.ts:36-66` `parseSpeakerPayload`, `useSpeakerPlayback.ts:133` | **[OK]** (khi gateway chạy) |
| `OP_FLUSH` | `0x03` | S→C | rỗng (`Bytes::new()`), `seq_id: 0` | Gửi trong `WebRTCActor::cancel_active_operations()` (`pipeline.rs:453-458`), tức mỗi `handle_vad_start` / `handle_vad_end` / `handle_interrupted` (`pipeline.rs:166,172,204`) | `liva-ui/src/App.vue:160-165` → `speaker.flush()` → `stop(false)` (`useSpeakerPlayback.ts:207,180-205`) | **[OK]** |
| `OP_ACK_PLAYING` | `0x04` | C→S (thiết kế) | — | **Không nơi nào trong Rust đọc/ghi**; rơi vào `_ => {}` (`main.rs:734`) | Chỉ có hằng số trong TS (`WebSocketClient.ts:8`) | **[THIẾU]** code chết hai đầu |

### 5.4 Định dạng payload từng loại — chi tiết

#### `OP_MIC_IN` (0x01) — client → server

```
payload = [f32 LE][f32 LE][f32 LE] …          // KHÔNG có sample_rate trong payload
```

- Sample rate **ngầm định 16.000 Hz mono** — server **không kiểm tra và không resample**. Nếu client gửi 48 kHz, VAD/STT vẫn chạy nhưng kết quả sai.
- Biên độ: f32 chuẩn `[-1.0, 1.0]`.
- Byte thừa (`len % 4 != 0`) bị **cắt bỏ im lặng** (`main.rs:591`).
- Xử lý sau khi decode (`main.rs:608-635`, trong một `spawn_blocking`, dùng `blocking_lock()`):

```mermaid
flowchart LR
    P["payload f32[]"] --> AEC["aec.process()<br/>opt-in LIVA_AEC_ENABLED=1"]
    AEC --> DN["GTCRN denoise<br/>BẬT mặc định"]
    DN --> VAD["VadEngine.check_streaming()"]
    VAD --> EV{"VadEvent"}
    EV -->|SpeechStart| WG{"wake_gate.is_awake()?"}
    WG -->|có| FS["pipeline_handle.on_vad_start()<br/>⇒ OP_FLUSH"]
    WG -->|không| NOP["chỉ set accumulating=true"]
    EV -->|SpeechEnd| VE["on_vad_end(speech_audio)"]
    EV -->|None| IDLE["accumulating ? buffer.extend"]
```

Điều **client cần biết** về barge-in (`main.rs:648-728`): server **chỉ phát `OP_FLUSH` khi `wake_gate.is_awake()`** — `VadEvent::SpeechStart` lúc gate đóng chỉ âm thầm bật `accumulating`, client sẽ không thấy tín hiệu nào trên socket. Vì `LIVA_WAKE_MODE` mặc định là **Off** (gate mở toàn phần, UX push-to-talk), hành vi mặc định là: mỗi lần VAD bắt đầu → có `OP_FLUSH`.

> 📌 Nguồn đầy đủ (ngưỡng VAD/AEC/denoise, các mode wake `AsrPrefix`/`Hybrid`/`TrainedModel`, cụm từ đánh thức, cửa sổ tỉnh, prefill chống cắt đầu câu): [Đường ống thoại](03-duong-ong-thoai.md)

#### `OP_SPEAKER_OUT` (0x02) — server → client

```
payload = [u32 LE sample_rate][f32 LE][f32 LE] …
          └─ 4 byte ─────────┘└─ (payload_len - 4) byte, chia hết 4 ─┘
```

Hợp đồng ghi thẳng trong code (`webrtc/pipeline.rs:376-388`):

```rust
// OP_SPEAKER_OUT payload contract: [u32 LE sample_rate][f32 LE PCM…]
let raw_bytes: &[u8] = bytemuck::cast_slice(&audio_samples);
let mut payload = Vec::with_capacity(4 + raw_bytes.len());
payload.extend_from_slice(&sample_rate.to_le_bytes());
payload.extend_from_slice(raw_bytes);
```

Client tham chiếu (`liva-ui/src/utils/speakerFrame.ts:36-66`) validate:
- `byteLength >= 8` (4 byte sample-rate + ít nhất 1 mẫu f32);
- `(byteLength - 4) % 4 == 0`;
- `8000 <= sampleRate <= 96000` (giới hạn Web Audio `AudioBuffer`);
- **luôn tôn trọng `byteOffset`** — payload bắt đầu ở byte 9 của WS frame nên **không căn 4-byte**, phải đọc qua `DataView.getFloat32(..., true)` thay vì `new Float32Array(buffer, 9, n)` (sẽ ném lỗi alignment).

> Đây là cái bẫy #1 khi viết client mới: **offset 9 không chia hết cho 4**.

#### `OP_FLUSH` (0x03) — server → client

```
payload = (rỗng, 0 byte)     seq_id = 0
```

Hành động client bắt buộc: **xoá ngay hàng đợi phát và tắt tiếng**. Kèm theo phía server: `session_id += 1`, abort 3 handle (stt/llm/tts), `tts_player.stop().await` (`pipeline.rs:437-458`).

> **Ghi chú mâu thuẫn nguồn (đã giải quyết):** một sơ đồ trình tự trong báo cáo khảo sát chú thích `OP_FLUSH` là "CHƯA CÓ TRONG CODE HIỆN TẠI". **Ba khu vực khảo sát độc lập** (`core-entry`, `webrtc`, `tts`) đều trích dẫn `pipeline.rs:453-458` gửi `OP_FLUSH`, và `bin/verify_duplex.rs:126-145` assert `on_vad_start()` → nhận `OP_FLUSH` **< 10 ms**. Tài liệu này kết luận theo trích dẫn code: **`OP_FLUSH` được gửi thật**.

#### `OP_AUTH_HANDSHAKE` (0x00) — echo

```rust
OP_AUTH_HANDSHAKE => {
    // Echo handshake back to acknowledge
    let handshake_frame = VoiceFrame {
        op_code: OP_AUTH_HANDSHAKE,
        seq_id: frame.seq_id,
        payload: frame.payload.clone(),
    };
    let _ = outgoing_tx.send(handshake_frame).await;
}
```
(`main.rs:580-588`) — dùng được như **ping/pong đo RTT**, không dùng được như xác thực.

#### `OP_ACK_PLAYING` (0x04)

Không có mã xử lý. Gửi lên sẽ rơi vào `_ => {}` (`main.rs:734`) và bị **nuốt im lặng** — client không nhận lỗi.

### 5.5 Máy trạng thái pipeline (ngữ cảnh cho client)

`webrtc/pipeline.rs:8-39`:

```rust
enum PipelineState { Idle, VadStart, VadEnd, SttProcessing, LlmGenerating, TtsSpeaking, Interrupted }
enum PipelineEvent {
    VadStart, VadEnd(Vec<f32>), Interrupted,
    SttCompleted { session_id, result },
    TtsSpeaking { session_id },
    LlmCompleted { … }, TtsCompleted { … },
}
```

Chống kết quả cũ bằng `active_session_id: Arc<AtomicU64>`, so khớp `session_id` ở **mọi** callback.

⚠️ **`WebRTCPipelineHandle::feed_rtp_pcm(&self, _samples: &[f32])` là TODO rỗng** (`pipeline.rs:72-77`) ⇒ code chết. Crate `webrtc = "0.12.0"` có trong `Cargo.toml` nhưng **luồng thật đi qua WebSocket nhị phân, không phải RTP**.

⚠️ **Các `PipelineState` KHÔNG được phát ra socket** dưới dạng sự kiện `state_change` — xem §10.

---

## 6. Lớp text — hai giao thức trên cùng một socket

Nhánh `Message::Text` (`main.rs:741-1022`) thử **theo thứ tự**:

1. Parse JSON. Nếu có field `event` (chuỗi) → **Lớp A (legacy client event)**, xử lý rồi `continue`.
2. Ngược lại parse thành `IpcRequest` → **Lớp B**. Parse lỗi → trả `IpcResponse{ id: "unknown", status: "error", error: "Invalid JSON query: …" }`.

```mermaid
flowchart TD
    T["Message::Text"] --> J{"parse JSON ok?"}
    J -->|không| E1["IpcResponse id=unknown, status=error"]
    J -->|có| EV{"có field 'event'?"}
    EV -->|có| A["LỚP A — legacy event<br/>main.rs:742-967"]
    EV -->|không| B["LỚP B — IpcRequest<br/>main.rs:971-1022"]
    A --> AR["trả {event, payload}"]
    B --> BR["trả IpcResponse {id,status,data?,error?}"]
```

### 6.1 Lớp A — legacy client event

Vào: `{"event": "<tên>", "payload": <bất kỳ>}`. Ra: `{"event": "<tên khác>", "payload": <kết quả>}`.

**Bảng ánh xạ đầy đủ** (`main.rs:742-967`):

| # | Event vào | Payload vào | → `handle_command` | Event ra | Dòng |
|---:|---|---|---|---|---|
| 1 | `get_config` | `{}` | `get_config` | `config_data` | 755 |
| 2 | `get_ai_config` | `{}` | `get_ai_config` | `ai_config` | 762 |
| 3 | `get_voice_status` | `{}` | `get_voice_status` | `voice_status` | 769 |
| 4 | `get_voice_profiles` | `{}` | `get_voice_profiles` | `voice_profiles` | 776 |
| 5 | `get_system_status` | `{}` | `get_system_status` | `system_status` | 783 |
| 6 | `get_skills_list` | `{}` | `get_skills_list` | `skills_list` | 790 |
| 7 | `get_user_profile` | `{}` | `get_user_profile` | `user_profile` | 797 |
| 8 | `get_tasks` | `{}` | `get_tasks` | `tasks_list` | 804 |
| 9 | `get_avatar_models` | `{}` | `get_avatar_models` | `avatar_models_list` | 811 |
| 10 | `get_memory_data` | `{}` | `get_memory_data` | `memory_data` | 818 |
| 11 | `user_voice_command` | `{text}` | **luồng riêng, KHÔNG qua `handle_command`** | `ai_thinking_start` → `ai_stream_start` → n×`ai_stream_chunk{textChunk,isThought}` → `ai_spoken_response{text}` → `ai_thinking_end` | 824-951 |
| 12 | *mọi event khác* | tuỳ | `handle_command(event_name, payload, None, None)` | `"{event}_response"` | 954-961 |

Hệ quả trực tiếp của dòng 12:
- `vision:ask` → `vision:ask_response` (khớp `liva-ui/src/composables/useGateway.ts:432`).
- `update_config` → `update_config_response`, **chứ không phải** `config_updated` — nên client không cập nhật `configData` từ phản hồi này (`useGateway.ts:379-380` chỉ khớp `config_data`/`config_updated`). **[MỘT PHẦN]**

⚠️ **Lỗi bị nuốt:** tất cả nhánh Lớp A đều bọc bằng `if let Ok(res) = handle_command(...)`. Khi `handle_command` trả `Err` (kể cả `"Unknown command: …"`), **không có gì được gửi về client** — client treo chờ vô hạn nếu không có timeout.

### 6.2 Chi tiết `user_voice_command`

`main.rs:831-953`:

| Điều kiện | Nhánh | Lỗi → |
|---|---|---|
| `text.to_lowercase()` chứa `"màn hình"` hoặc `"screen"` | **vision**: `capture_for_vision()` + `llm.answer_with_image(VisionImage::Rgb{…}, TEMP_DEFAULT, TOP_P_DEFAULT, cb streaming)` | chuỗi cứng `"Xin lỗi, hiện mình chưa xem được màn hình."` |
| ngược lại | dựng `[system = PERSONA_LIVA, user = text]` → `compile_prompt` → `generate_completion` streaming | `"Xin lỗi, đã xảy ra lỗi trong quá trình xử lý."` |

Chuỗi sự kiện ra (cả hai nhánh):

```
ai_thinking_start
ai_stream_start
ai_stream_chunk  { textChunk: "...", isThought: bool }   × n
ai_spoken_response { text: "<toàn văn>" }
ai_thinking_end
```

### 6.3 Lớp B — `IpcRequest`

Vào (`main.rs:971-1022`):

```json
{ "id": "req_001", "command": "chat:completion", "payload": { "...": "..." } }
```

Ra:

```json
{ "id": "req_001", "status": "ok", "data": { "...": "..." } }
```
hoặc
```json
{ "id": "req_001", "status": "error", "error": "Unknown command: foo" }
```

Khác biệt then chốt so với Lớp A: `handle_command` được gọi **kèm `tx` và `req_id`** (`main.rs:993-1000`) ⇒ **chỉ Lớp B mới stream được**. Lớp A luôn truyền `None, None`.

---

## 7. `handle_command` — bảng 42 lệnh đầy đủ

Chữ ký (`liva-native-core/src/lib.rs:236-242`):

```rust
pub async fn handle_command(
    state: Arc<AppState>,
    command: &str,
    payload: serde_json::Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
) -> Result<serde_json::Value, String>
```

Nhánh mặc định: `Err(format!("Unknown command: {}", command))` (`lib.rs:1483`).

Ký hiệu: `*` = bắt buộc. Cột "Dòng" là số dòng trong `liva-native-core/src/lib.rs`.

| # | Lệnh | Payload | Trả về (Ok) | Dòng | UI gọi? | Trạng thái |
|---:|---|---|---|---|---|---|
| 1 | `ping` | — | `{"pong": true}` | 246 | mobile | **[OK]** |
| 2 | `vision:capture` | — | `{width, height, format, data(base64)}`; cập nhật `last_frame` | 249-273 | **không** | **[MỘT PHẦN]** — base64 nguyên frame RGBA ≈ 11 MB @1080p |
| 3 | `vision:add_region` | `ScreenRegion{id,name,x,y,width,height,threshold}` | `{"success":true}` | 274-280 | **không** | **[MỘT PHẦN]** |
| 4 | `vision:remove_region` | `{id*}` | `{"success":true}` | 281-288 | **không** | **[MỘT PHẦN]** |
| 5 | `vision:get_changed_regions` | — | `[RegionDiffResult{region_id,name,difference,is_changed}]`; lần đầu (`last_frame=None`) trả baseline `difference=1.0, is_changed=true` | 289-336 | **không** | **[MỘT PHẦN]** |
| 6 | `vision:set_config` | `VisionConfig{color_tolerance,max_regions}` | `{"success":true}` | 337-343 | **không** | **[MỘT PHẦN]** |
| 7 | `echo` | bất kỳ | chính payload | 345 | không | **[OK]** |
| 8 | `status` | — | `{engine:"LIVA Native Engine", status:"healthy", version:CARGO_PKG_VERSION}` | 346-350 | không | **[OK]** |
| 9 | `get_config` | — | nội dung `data/liva-config.json`; thiếu file → object mặc định lớn (`avatar/ai/ui/system/voice`) | 351-403 | có | **[OK]** |
| 10 | `update_config` | patch JSON | `{"success":true}`; deep-merge `merge_json` rồi ghi file; có key `ai` → spawn `load_configured_router_model(state, force=true)` | 404-427 | có | **[OK]** |
| 11 | `get_ai_config` | — | phần `ai` của config, hoặc defaults | 428-451 | có | **[OK]** |
| 12 | `get_voice_status` | — | `{stt: "ready"\|"offline", tts: …}` (`stt.model_dir.exists()`, `tts.is_some()`; hack test: `model_dir == "non_existent_dir"` ⇒ ready) | 452-472 | có | **[OK]** |
| 13 | `get_voice_profiles` | — | mảng **chuỗi** tên file trong `data/voices` (path tương đối, **không** qua `resolve_resource_path`) | 473-488 | có | **[MỘT PHẦN]** — UI mong mảng object |
| 14 | `get_system_status` | — | object health lớn — **phần lớn là số cứng giả** (`cpuUsage:12`, `uptime:3600`, `totalMemory:16000000000`…); chỉ `modelLoaded`/`model`/`aiEngine.status` là thật | 489-527 | có (poll 3s) | **[MỘT PHẦN]** |
| 15 | `get_skills_list` | — | `[smart_home::get_metadata()]` — **đúng 1 skill** | 528-532 | có | **[MỘT PHẦN]** |
| 16 | `get_user_profile` | — | `data/user_profile.json`, hoặc profile hardcode | 533-554 | có | **[OK]** |
| 17 | `get_tasks` | — | `{tasks:[{id,title,description,status,priority,result,createdAt,updatedAt}]}` từ SQLite `tasks` | 555-589 | có | **[OK]** |
| 18 | `add_task` | `{title*, description, priority="medium", status="pending", id?}` | `{"success":true,"id":…}` (id = `rand::random::<u64>()` nếu thiếu) | 590-625 | có | **[OK]** |
| 19 | `delete_task` | `{id*}` | `{"success":true}` | 626-647 | có | **[OK]** |
| 20 | `update_task` | `{id*, updates:{title?,description?,status?,priority?,result?}}` | `{"success":true}` (transaction read-modify-write) | 648-707 | có | **[OK]** |
| 21 | `task_plan_chat` | `{taskId*, message\|text*, temperature?, top_p?, stream?}` — `stream` mặc định `tx.is_some()` | stream: chunk `{taskId, message, done:false}`; cuối `{taskId, message, done:true}`. Prompt `SYS_TASK_PLANNER`; title/desc bọc `<user_task_title>` + `sanitize_untrusted` | 708-808 | có | **[OK]** — chunk **không** bọc `IpcResponse` |
| 22 | `get_avatar_models` | — | `{models2d, models3d}` mảng **chuỗi**, từ `models/live2d`, `models/vrm` | 809-843 | có | **[MỘT PHẦN]** — lệch schema UI |
| 23 | `get_memory_data` | — | `{l0, l0_5:"", facts, events, vectors}`; `facts.value` được `crypto.decrypt` | 844-979 | có | **[MỘT PHẦN]** — bảng nguồn không có writer |
| 24 | `memory:set_fact` | `db::Fact` (13 field, **không** `serde(default)`) | `{"success":true}` | 980-999 | **không** | **[MỘT PHẦN]** |
| 25 | `memory:get_fact` | `{key*}` | `Fact` hoặc `null` | 1000-1023 | **không** | **[MỘT PHẦN]** |
| 26 | `memory:search_hybrid` | `{query_text*, query_vector*:[f32], top_k=5, filter?:MetadataFilter, dense_weight=1.0, sparse_weight=1.0}` | kết quả `search_hybrid_vectors` (RRF K=60) | 1024-1083 | **không** | **[MỘT PHẦN]** — client phải tự tính vector |
| 27 | `memory:upsert_vector` | `{vecId*, type*, content*, vector*:[f32], domain?, category?, traceKeywords?, fileTarget?, sourceEventIds?}` | `{"success":true}` | 1084-1169 | **không** | **[MỘT PHẦN]** |
| 28 | `voice:stt_start` | — | `{"success":true}` (`reset_stream`) | 1170-1173 | **không** | **[MỘT PHẦN]** |
| 29 | `voice:stt_chunk` | `{chunk*: base64 f32 LE PCM, isLast=false}` | `{text}` | 1174-1204 | **không** | **[MỘT PHẦN]** |
| 30 | `voice:stt_stop` | — | `{text}` (`feed_audio(&[], true)`) | 1205-1215 | mobile | **[MỘT PHẦN]** |
| 31 | `voice:stt_flush` | — | `{"success":true}` (giống `stt_start`) | 1216-1219 | **không** | **[MỘT PHẦN]** |
| 32 | `voice:set_language` | `{language*}` | `{"success":true, language}` — set cả STT lẫn TTS | 1220-1233 | **không** | **[MỘT PHẦN]** — ngôn ngữ thực tế cố định bằng env |
| 33 | `voice:tts_speak` | `{text*, flush=false}` | `{"success":true}`; lỗi `"TTS engine not initialized"` nếu `tts=None` | 1234-1251 | **không** | **[MỘT PHẦN]** |
| 34 | `voice:tts_stop` | — | `{"success":true}` — `tts_player.stop()` **trước**, rồi spawn task lock `tts` | 1252-1264 | **không** | **[MỘT PHẦN]** |
| 35 | `llm:swap_model` | `{model_path*, n_ctx?, n_gpu_layers?, vocab_only?}` | `{"success":true}` | 1265-1281 | **không** | **[MỘT PHẦN]** — **không validate path** (rủi ro bảo mật) |
| 36 | `llm:embed` | `{input*: String \| [String]}` | vector đơn nếu input là chuỗi, mảng vector nếu là mảng; lỗi nếu `vocab_only` hoặc chưa load model | 1282-1317 | **không** | **[MỘT PHẦN]** — không có consumer |
| 37 | `chat:completion` | `{messages*:[{role,content}], temperature=TEMP_DEFAULT, top_p=TOP_P_DEFAULT, stream=false}` | stream: `IpcResponse{data:{token, done:false}}` từng token (cần **cả** `tx` **và** `req_id`); cuối `{text, done:true, usage:{prompt_tokens, completion_tokens, total_tokens}}`. **Tự chèn `PERSONA_LIVA`** nếu client không gửi system | 1318-1393 | **không** | **[MỘT PHẦN]** — API cho tool ngoài |
| 38 | `vision:ask` | `{question?, temperature=0.7, top_p=0.8, image?: base64}` — thiếu `question` → mặc định `"Trên màn hình đang hiển thị gì? Mô tả ngắn gọn bằng tiếng Việt."`; thiếu `image` → `capture_for_vision()` | `{text, usage:{prompt_tokens, completion_tokens}}` — **không stream** (callback `\|_\| true`) | 1394-1445 | **có** | **[MỘT PHẦN]** — cần build RELEASE |
| 39 | `llm:health_check` | — | `{status:"healthy", model_loaded, model_path, n_ctx, n_gpu_layers}` | 1446-1458 | **không** | **[MỘT PHẦN]** |
| 40 | `telegram:send_text` | `{chatId*: chuỗi số, text*}` | `{"success":true}` fire-and-forget (tạo `Bot` mới mỗi lần từ env) | 1459-1473 | **không** | **[MỘT PHẦN]** |
| 41 | `integration:smart_home_control` | `SmartHomeArgs` (tuỳ `smart_home::execute`) | `{result}` | 1474-1477 | **không** | **[THIẾU]** — `execute` là stub |
| 42 | `integrations:list` | — | `[smart_home::get_metadata()]` | 1478-1482 | có | **[MỘT PHẦN]** |

---

## 8. Khung streaming — hai định dạng khác nhau

`tx` / `req_id` **chỉ có ý nghĩa** với `chat:completion` (#37) và `task_plan_chat` (#21). Hai lệnh này **không dùng chung định dạng chunk** — đây là điểm gây lỗi client nhiều nhất.

| Lệnh | Chunk giữa chừng | Chunk cuối | Có bọc `IpcResponse`? |
|---|---|---|---|
| `chat:completion` | `{"id":"<req_id>","status":"ok","data":{"token":"…","done":false}}` | Trả qua **giá trị `Ok`** của `handle_command` → server bọc thành `IpcResponse{data:{text,done:true,usage:{…}}}` | **CÓ** |
| `task_plan_chat` | `{"taskId":…,"message":"…","done":false}` | `{"taskId":…,"message":"…","done":true}` | **KHÔNG** — thiếu `id` và `status` |

⇒ **Client phải parse hai dạng khung stream khác nhau trên cùng một socket.** Cách phân biệt an toàn: nếu JSON có field `status` → là `IpcResponse`; nếu có field `taskId` → là chunk `task_plan_chat`; nếu có field `event` → là sự kiện Lớp A.

Với Tauri, chunk stream không đi qua socket mà qua `window.emit(&format!("ipc-stream:{}", req_id), resp)` (`liva-desktop/src-tauri/src/lib.rs:257`, kênh `mpsc(100)`).

---

## 9. Lệnh UI gửi mà core không có handler

**22 sự kiện** mà `liva-ui` gửi đi không khớp match arm nào trong `lib.rs`/`main.rs` (ví dụ `consolidate_memory`, `select_voice_profile`, `save_env_config`, `reset_memory`…) ⇒ rơi vào `_ => Err("Unknown command: …")` (`lib.rs:1483`).

**Điểm giao thức quan trọng:** vì Lớp A bọc lời gọi bằng `if let Ok(res)` (§6.1), `Err` **không sinh ra khung phản hồi nào** — client không nhận `IpcResponse` lỗi, cũng không nhận event. Đây là lý do UI treo spinner rồi tự tắt thay vì báo lỗi. Người viết client mới **phải dùng Lớp B** (§6.3) nếu muốn thấy `Unknown command`. Trạng thái: **[THIẾU]** ở phía core.

> 📌 Nguồn đầy đủ (danh sách 22 sự kiện mồ côi, 14 lệnh core không client nào gọi, `mobile_client` sai contract): [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md)

---

## 10. Đối chiếu THIẾT KẾ GỐC vs AS-BUILT

Nguồn thiết kế: [`docs/99-luu-tru/thiet-ke-goc/LIVA_CLIENT_SERVER_DESIGN.md`](../99-luu-tru/thiet-ke-goc/LIVA_CLIENT_SERVER_DESIGN.md).

### 10.1 Bảng đối chiếu tổng hợp

| Hạng mục thiết kế | Thiết kế gốc nói | AS-BUILT (code thật) | Kết luận |
|---|---|---|---|
| Giao thức | WebSocket WS/WSS | WebSocket, `tokio-tungstenite` 0.21 | **KHỚP** |
| Port & endpoint | `8002`, `/ws`, đổi được bằng `LIVA_SERVER_PORT` | `main.rs:451-452`, `LIVA_SERVER_HOST` + `LIVA_SERVER_PORT`, path `/ws` | **KHỚP** (có thêm `LIVA_SERVER_HOST`) |
| Bind từ xa `0.0.0.0` | "remote deployments bind to `0.0.0.0:8002/ws`" | Mặc định `127.0.0.1`; đổi được bằng env nhưng **không có auth/TLS** | **LỆCH** — mở ra ngoài là không an toàn |
| WSS / TLS qua `rustls` hoặc reverse proxy | có nêu | **Không có code TLS nào trong core** | **THIẾU** |
| Header nhị phân 9 byte | `[op u8][seq u32 LE][len u32 LE]` | `frame.rs:22-25` y hệt | **KHỚP CHÍNH XÁC** |
| Giới hạn payload 1.048.576 | có | `frame.rs:18`, `:37` | **KHỚP CHÍNH XÁC** |
| 5 opcode `0x00`-`0x04` | có | `frame.rs:3-7` y hệt | **KHỚP** về hằng số |
| `OP_AUTH_HANDSHAKE` "ping-pong authentication" | ngụ ý có xác thực | Echo nguyên payload, **không xác thực** (`main.rs:580-588`) | **LỆCH** |
| `OP_ACK_PLAYING` theo dõi tiến độ phát | có đặc tả | **Không nơi nào trong Rust đọc/ghi**; `_ => {}` (`main.rs:734`) | **THIẾU** |
| Payload `OP_MIC_IN` = f32 PCM 16 kHz mono | có | đúng, thêm quy tắc cắt `len/4*4` (`main.rs:591`) | **KHỚP** |
| Payload `OP_SPEAKER_OUT` = f32 PCM **16 kHz** | thiết kế ghi 16 kHz | **Thực tế có prefix `[u32 LE sample_rate]`** và sample rate là 24000 / 22050 / `v.sample_rate()` (`pipeline.rs:376-388, 357, 341`) | **LỆCH** — as-built giàu hơn thiết kế; **client phải đọc sample_rate từ payload**, không được giả định 16 kHz |
| `OP_FLUSH` server→client khi barge-in | có | `pipeline.rs:453-458`, gửi ở `handle_vad_start`/`handle_vad_end`/`handle_interrupted` | **KHỚP** |
| `IpcRequest` `{id, command, payload}` | có, ví dụ `chat:completion` | `main.rs:13-18` y hệt | **KHỚP** |
| `IpcResponse` có `"error": null` khi ok | ví dụ JSON ghi `"error": null` | `skip_serializing_if = "Option::is_none"` ⇒ **trường vắng mặt** (`main.rs:22-28`) | **LỆCH nhẹ** — client phải chịu được cả hai |
| Chunk stream `{"id","status","data":{"token","done"}}` | có | đúng với `chat:completion`; **`task_plan_chat` KHÔNG bọc `IpcResponse`** | **LỆCH MỘT PHẦN** |
| Sự kiện `state_change` (`VadStart`, `LlmGenerating`, `TtsSpeaking`, `Idle`) | 5 lần xuất hiện trong sơ đồ trình tự | **Không có mã nào phát `state_change` ra socket**; `PipelineState` chỉ sống nội bộ (`pipeline.rs:8-17`) | **THIẾU** |
| Sự kiện `stt_completed` gửi text về UI | có | Không có event `stt_completed` trên socket; text chỉ đi qua chuỗi `ai_stream_*` của `user_voice_command` | **THIẾU / thay bằng cơ chế khác** |
| Sự kiện telemetry `system_status` đẩy định kỳ | có | **Chỉ tồn tại kiểu pull**: client phải gửi `{"event":"get_system_status"}` (`main.rs:783`); không có push định kỳ | **LỆCH** |
| "Two frame types" (JSON text + binary) trên **một** kết nối | có | `send_task` `tokio::select!` multiplex (`main.rs:513-547`) | **KHỚP** |
| Giữ stdin/stdout legacy, **dùng chung `Arc<AppState>`** | có | `main.rs:358-433` dùng chính `state` đã dựng ở bước 16 | **KHỚP** |
| Model phía server (router Gemma, TTS Kokoro) | "Gemma-4-E4B-it router model" + Kokoro | Router đã là **Qwen3-VL**; TTS định tuyến VieNeu → Piper → Kokoro | **LỆCH** — thiết kế lỗi thời (không ảnh hưởng hợp đồng dây, trừ `sample_rate` của `OP_SPEAKER_OUT`) |
| Client "ultra-lightweight" không load model AI | có | `liva-ui` vẫn chạy **WakeWordWorker** phía client (`useVoicePipeline.ts:336`) | **LỆCH nhẹ** |
| MCP server native | có | `NativeMcpServer` được **khởi tạo** nhưng **không có nhánh `mcp:*`** trong `handle_command` | **THIẾU** |

> 📌 Nguồn đầy đủ về model: cấu hình LLM và persona ở [Hệ LLM và prompt](04-he-llm-va-prompt.md), bảng model + RAM/VRAM ở [Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md), bảng backend TTS ở [Đường ống thoại](03-duong-ong-thoai.md). Đối chiếu **tuyên bố sản phẩm** (khác với đối chiếu thiết kế gốc ở bảng trên): [Đối chiếu tuyên bố vs thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md).

### 10.2 ⚠️ Lỗi nghiêm trọng nhất — header 1 byte của `liva-ui`

**Thiết kế gốc và server đều dùng header 9 byte. `liva-ui` gửi header 1 byte.**

`liva-ui/src/composables/useVoicePipeline.ts:345-350`:

```ts
// Prepend 0x01 header to raw PCM audio chunk (Audio Buffer Slicing optimization)
const pcmBuffer = buffer.buffer;
const msg = new Uint8Array(1 + pcmBuffer.byteLength);
msg[0] = 0x01; // Audio header
msg.set(new Uint8Array(pcmBuffer), 1);
wsRef.send(msg);
```

So với `mobile_client/src/services/WebSocketClient.ts:176-182` (**đúng hợp đồng**, đi qua `serializeVoiceFrame(opcode, seqId, payload)` với header 9 byte).

**Hậu quả cơ học** khi server decode (`main.rs:568-578` + `frame.rs:29-53`):

| Byte của message `liva-ui` | Server diễn giải là | Thực chất là |
|---|---|---|
| `[0]` = `0x01` | `op_code` = `OP_MIC_IN` ✔ tình cờ đúng | header 1 byte |
| `[1..5]` | `seq_id` (u32 LE) | **4 byte đầu của mẫu PCM f32 thứ nhất** |
| `[5..9]` | `payload_len` (u32 LE) | **4 byte của mẫu PCM f32 thứ hai** |
| `[9..]` | payload | audio bị lệch 9 byte |

Vì `payload_len` được đọc từ **bit pattern của một mẫu f32 audio**, giá trị nó nhận gần như luôn vượt `1 MiB` ⇒ `decode` trả `Err("Payload exceeds 1MB limit")` ⇒ `error!("Frame decode error")` + `break` (`main.rs:573-577`) ⇒ **khung bị vứt, không mẫu audio nào tới VAD**. Chỉ khi mẫu thứ hai đúng bằng `0.0` (im lặng tuyệt đối) thì `payload_len = 0` và server "decode thành công" một khung rỗng — vẫn không có audio.

⇒ Kết luận: **đường mic của `liva-ui` qua WebSocket không thể hoạt động với core hiện tại.** Trạng thái **[MỘT PHẦN]**: server đúng hợp đồng, `mobile_client` đúng hợp đồng, `liva-ui` **sai hợp đồng**.

Lưu ý phụ: lỗi này hiện **bị che** bởi sự thật ở §1 — luồng dev chuẩn không chạy gateway 8002, nên client `liva-ui` không có server để nói chuyện, và lỗi không bao giờ nổi lên.

**Cách sửa đúng** (cho người bảo trì `liva-ui`): dựng khung 9 byte y như `mobile_client`:

```
DataView: setUint8(0, 0x01); setUint32(1, seqId, true); setUint32(5, pcm.byteLength, true);
rồi copy pcm vào offset 9.
```

### 10.3 Sơ đồ trình tự — thiết kế gốc, kèm đính chính as-built

Sơ đồ dưới đây giữ nguyên tinh thần của `LIVA_CLIENT_SERVER_DESIGN.md` §3, các bước **không tồn tại trong code** được đánh dấu rõ.

```mermaid
sequenceDiagram
    autonumber
    participant C as Client (Mic/Speaker/UI)
    participant S as Server (liva-native-core)

    C->>S: OP_AUTH_HANDSHAKE (binary)
    S-->>C: echo OP_AUTH_HANDSHAKE [OK — nhưng KHÔNG xác thực]

    loop Voice interaction
        C->>S: OP_MIC_IN (binary, PCM f32 16kHz mono)
        Note over S: AEC → GTCRN → Silero VAD
    end

    Note over S: VAD SpeechStart (chỉ khi wake_gate.is_awake())
    S-->>C: event state_change(VadStart) [THIẾU — không có code]
    S-->>C: OP_FLUSH (binary, payload rỗng, seq=0) [OK]

    Note over S: VAD SpeechEnd
    S-->>C: event state_change(VadEnd) [THIẾU]
    Note over S: STT (Nemotron / Parakeet)
    S-->>C: event stt_completed(text) [THIẾU]

    Note over S: LLM (Qwen3-VL router), KHÔNG phải Gemma như thiết kế
    S-->>C: event state_change(LlmGenerating) [THIẾU]

    Note over S: TTS (VieNeu → Piper → Kokoro)
    S-->>C: event state_change(TtsSpeaking) [THIẾU]
    loop Từng chunk TTS
        S-->>C: OP_SPEAKER_OUT [u32 LE sample_rate][f32 LE PCM…] [OK]
    end
    S-->>C: event state_change(Idle) [THIẾU]
```

**Đọc sơ đồ:** phần **nhị phân** (`OP_AUTH_HANDSHAKE`, `OP_MIC_IN`, `OP_FLUSH`, `OP_SPEAKER_OUT`) là **có thật**. Toàn bộ **kênh sự kiện trạng thái** (`state_change`, `stt_completed`) trong thiết kế gốc **chưa được hiện thực** — client hiện chỉ có thể suy ra trạng thái gián tiếp qua `OP_FLUSH` (bắt đầu lượt mới) và luồng chunk `OP_SPEAKER_OUT` (đang nói).

---

## 11. Checklist cho người viết client

1. **Chọn kênh.** Muốn voice duplex → bắt buộc chạy binary `liva-native-core` thủ công, kết nối `ws://127.0.0.1:8002/ws`. Chỉ cần lệnh điều khiển → có thể dùng stdio hoặc Tauri `invoke`.
2. **Khung nhị phân LUÔN 9 byte header.** Không có ngoại lệ. Test bằng cách gửi `OP_AUTH_HANDSHAKE` và chờ echo cùng `seq_id`.
3. **Gửi trọn khung trong một WS message.** Server không nối khung dở dang qua ranh giới message.
4. **`OP_MIC_IN`: f32 LE, 16 kHz, mono, không header sample-rate.** Chunk nên ~20-100 ms; giới hạn cứng 1 MiB payload.
5. **`OP_SPEAKER_OUT`: đọc `sample_rate` từ 4 byte đầu payload**, không giả định 16 kHz. Payload bắt đầu ở byte 9 của WS frame ⇒ **không căn 4-byte** ⇒ dùng `DataView.getFloat32(offset, true)`.
6. **`OP_FLUSH` = xoá hàng đợi phát ngay lập tức.** Không đợi hết chunk đang phát.
7. **Đừng gửi `OP_ACK_PLAYING`** — server nuốt im lặng.
8. **Lệnh điều khiển: ưu tiên Lớp B (`IpcRequest`).** Lớp A (`event`) **nuốt mọi lỗi** — dùng nó bạn sẽ không biết lệnh sai.
9. **Streaming: chỉ Lớp B mới stream.** Phân biệt khung bằng `status` (IpcResponse) / `taskId` (task_plan_chat) / `event` (Lớp A).
10. **`error` vắng mặt khi thành công** (không phải `null`) — code client phải chịu được cả hai.
11. **Một client voice tại một thời điểm.** VAD/denoise/AEC là state toàn cục, không phân vùng session.
12. **Không phát song song lệnh LLM.** `state.llm` là một Mutex duy nhất.
13. **Không có auth.** Đừng bind ra `0.0.0.0` khi chưa có TLS + token.

---

## Liên quan

**Đọc tiếp theo mạch:** [⬆ Mục lục](../README.md) · [◀ Kiến trúc tổng thể](01-kien-truc-tong-the.md) · [Đường ống thoại ▶](03-duong-ong-thoai.md)

**Tài liệu này dựa vào (nguồn sự thật ở nơi khác):**

- [Kiến trúc tổng thể](01-kien-truc-tong-the.md) — bảng so sánh **hai profile chạy**, dùng ở §1 để nói profile nào mới mở gateway 8002.
- [Đường ống thoại](03-duong-ong-thoai.md) — ngưỡng VAD/AEC/denoise, các mode wake gate, bảng backend TTS; §5.4 chỉ giữ hệ quả giao thức (khi nào có `OP_FLUSH`, `sample_rate` nào xuất hiện trong `OP_SPEAKER_OUT`).
- [Hệ LLM và prompt](04-he-llm-va-prompt.md) — cấu hình LLM, `PERSONA_LIVA`, `sanitize_untrusted`; §6.2 và §7 (#21, #37, #38) chỉ mô tả phần dây, không lặp lại nội dung prompt.
- [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) — bảng biến môi trường đầy đủ; §3.2 chỉ ghi mặc định tại đúng chỗ đọc trong `main.rs`.
- [Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) — bảng model và RAM/VRAM, dùng ở §10.1 khi nói router đã đổi sang Qwen3-VL.
- [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md) — cách chạy đúng từng profile, tức cách để gateway 8002 thực sự sống.
- [Frontend và vỏ Tauri](08-frontend-va-vo-tauri.md) — bảng lệnh Tauri `invoke` và cấu hình cửa sổ, bổ sung cho §3.3 và §8.
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — danh sách đầy đủ lệnh mồ côi hai chiều core ↔ client, nền cho §9.
- [Đối chiếu tuyên bố vs thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — đối chiếu tuyên bố sản phẩm, khác với đối chiếu thiết kế gốc ở §10.
- [Thiết kế gốc: LIVA Client-Server Design](../99-luu-tru/thiet-ke-goc/LIVA_CLIENT_SERVER_DESIGN.md) — văn bản thiết kế được §10 đem ra đối chiếu.
- [Báo cáo khảo sát gốc 2026-07](../03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) — nguồn khảo sát thô, đã giải quyết mâu thuẫn về `OP_FLUSH` ở §5.4.

**Tài liệu khác dựa vào tài liệu này:**

- [Đường ống thoại](03-duong-ong-thoai.md) — lấy khung nhị phân 9 byte và ý nghĩa 5 opcode để mô tả chặng vận chuyển audio.
- [Frontend và vỏ Tauri](08-frontend-va-vo-tauri.md) — lấy tên lệnh trong bảng 42 lệnh để nói màn hình nào gọi lệnh nào.
- [Agent, bộ nhớ và tiến hoá](05-agent-bo-nho-va-tien-hoa.md) — lấy chữ ký `handle_command` và sự thật "không có nhánh `mcp:*`/`swarm:*`" để chứng minh module mồ côi.
- [Tích hợp ngoài](09-tich-hop-ngoai.md) — lấy hợp đồng `telegram:send_text` và cơ chế `ipc_tx` ghi ra stdout.
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — lấy sự thật "WS 8002 không xác thực" và "`llm:swap_model` không validate path" làm C1/C2.
- [Lộ trình sửa lỗi và nâng cấp](../03-danh-gia/03-lo-trinh-sua-loi-va-nang-cap.md) — lấy §10.2 (header 1 byte của `liva-ui`) làm một hạng mục sửa.
- [Phụ thuộc module và tra cứu](10-phu-thuoc-module-va-tra-cuu.md) — lấy vị trí `lib.rs`/`main.rs`/`webrtc/` để dựng bản đồ tra cứu.

**Khi sửa code sau đây thì phải cập nhật tài liệu này:**

- `liva-native-core/src/lib.rs` — chữ ký và **bảng 42 lệnh** ở §7; thêm/xoá một match arm là phải sửa bảng.
- `liva-native-core/src/main.rs` — §3 (25 bước khởi động), §4 (server WS + stdio IPC), §5.4 (xử lý `OP_MIC_IN`), §6 (hai lớp text).
- `liva-native-core/src/webrtc/frame.rs` — §5.1, §5.2 (khung 9 byte), §5.3 (bảng opcode). Đây là phần lõi tài liệu sở hữu.
- `liva-native-core/src/webrtc/pipeline.rs` — §5.4 (`OP_SPEAKER_OUT`, `OP_FLUSH`), §5.5 (máy trạng thái pipeline).
- `liva-desktop/src-tauri/src/lib.rs` — §1 (điểm vào không mở WS), §3.3 (khác biệt vỏ Tauri), §8 (stream qua `window.emit`).
- `liva-ui/src/composables/` (`useVoicePipeline.ts`, `useGateway.ts`, `useSpeakerPlayback.ts`) — §6.1 (ánh xạ event), §9, §10.2 (lỗi header 1 byte).
- `liva-ui/src/utils/speakerFrame.ts` — §5.4 (bẫy alignment offset 9) và §11 mục 5.
- `scripts/start_all.ps1` — §1: nếu script bắt đầu khởi động binary `liva-native-core` thì kết luận "gateway 8002 không chạy" hết đúng.
