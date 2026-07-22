---
title: "Đối chiếu tuyên bố và thực tế"
updated: 2026-07-22
commit: b298051
status: living
owns:
  - bang-doi-chieu-tuyen-bo
  - kiem-chung-offline
covers:
  - Cargo.toml
  - data/liva-config.json
  - liva-desktop/src-tauri/Cargo.toml
  - liva-desktop/src-tauri/tauri.conf.json
  - liva-desktop/src-tauri/src/lib.rs
  - liva-native-core/Cargo.toml
  - liva-native-core/src/*
  - liva-native-core/src/agent/*
  - liva-native-core/src/bin/router_stress.rs
  - liva-native-core/src/bin/verify_duplex.rs
  - liva-native-core/src/evolution/mod.rs
  - liva-native-core/src/integrations/smart_home.rs
  - liva-native-core/src/llm/embed.rs
  - liva-native-core/src/llm/prompt/mod.rs
  - liva-native-core/src/mcp/client.rs
  - liva-native-core/src/mcp/server.rs
  - liva-native-core/src/stt/*
  - liva-native-core/src/tts/*
  - liva-native-core/src/vision/capture.rs
  - liva-native-core/src/webrtc/*
  - liva-native-core/tests/*
  - liva-ui/src/App.vue
  - liva-ui/src/WidgetApp.vue
  - liva-ui/src/components/dashboard/*
  - liva-ui/src/composables/*
  - liva-ui/src/platform/TauriAdapter.ts
  - liva-ui/src/utils/fetch.ts
  - liva-ui/src/utils/speakerFrame.ts
  - liva-voice/liva_api.py
  - liva-voice/test_integration.py
  - liva-voice/src/*
  - scripts/*
---
# Đối chiếu: LIVA tuyên bố gì — code làm gì

[⬆ Mục lục](../README.md) · [◀ Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) · [Nợ kỹ thuật và rủi ro ▶](02-no-ky-thuat-va-rui-ro.md)

---

Tài liệu này đặt từng tuyên bố trong `README.md` (và trong `data/liva-config.json`, `.env.example`) cạnh bằng chứng đọc được từ mã nguồn. Mục đích **không** phải hạ thấp dự án, mà để mọi phát biểu ra bên ngoài (hồ sơ dự thi, README, demo cho beta tester) đều có chỗ dựa kiểm chứng được, và để đội phát triển biết chính xác chỗ nào cần nối dây tiếp.

Quy ước nhãn dùng xuyên suốt:

| Nhãn | Nghĩa |
|---|---|
| **[OK]** | Đang chạy thật trên đường chạy chính, nối dây đầy đủ |
| **[MỘT PHẦN]** | Có code thật nhưng bị tắt mặc định / opt-in / chỉ sống ở một entry point / chưa nối dây tới UI |
| **[THIẾU]** | Chưa có, là stub, hoặc tuyên bố sai sự thật |

---

## 0. Điều kiện tiên quyết: LIVA có HAI "profile chạy" không tương đương

Mọi phán quyết bên dưới chỉ đọc đúng khi hiểu điều này trước. Cùng một `AppState` và cùng một `handle_command` được dựng ở **hai** entry point khác nhau, nhưng chúng **không** cấu hình giống nhau: bản standalone `liva-native-core.exe` có đủ WS server 8002, VAD/denoise/turn-shadow/AEC, wake gate và Telegram bot; còn Tauri shell `liva-desktop` (chính là cái `npm run dev` chạy) **không** có WS server, hard-code bốn module thoại thành `None`, không wake gate, không Telegram — chỉ governor game-mode là có ở cả hai.

> 📌 Nguồn đầy đủ (bảng so sánh hai profile + sơ đồ kiến trúc tổng thể): [Kiến trúc tổng thể](../01-ban-ve/01-kien-truc-tong-the.md)

Phần đối chiếu **riêng của tài liệu này** là chỗ tuyên bố lệch với script khởi động:

`scripts/start_all.ps1` **kill** tiến trình đang giữ port 8002 (dòng 24: `$ports = @(8101, 8100, 8002, 8082, 5173, 8000)`) rồi chỉ chạy `liva-ui` (dòng 52) + `npx.cmd tauri dev --no-dev-server` (dòng 66). **Không dòng nào khởi động `liva-native-core`.** Comment trong `liva-desktop/src-tauri/src/lib.rs:460` ("Gateway is already running on port 8002 (started by start_all.ps1)") **sai sự thật so với chính file script**.

Hệ quả trực tiếp: mọi tính năng đi qua WebSocket nhị phân — full-duplex, barge-in, `OP_SPEAKER_OUT`, wake word, VAD — **không sống** trong luồng dev/chạy chuẩn, trừ khi người dùng tự tay chạy binary standalone.

> 📌 Sơ đồ hai profile và cách chạy đúng từng profile: [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md)

---

## 1. Bảng đối chiếu đầy đủ

| Tính năng | Nguồn tuyên bố | Trạng thái | Bằng chứng (file:dòng) | Ghi chú |
|---|---|---|---|---|
| **Rust binary đơn + Tokio, không GC** | README:23 | **[OK]** | `main.rs:30-49` (`Builder::new_multi_thread`), `Cargo.toml` workspace | Đúng như mô tả. Đây là claim "hạ tầng" chắc nhất của README |
| **TTFT < 100 ms** | README:23 | **[THIẾU] — không có bằng chứng** | grep `TTFT\|Time-To-First\|100ms` trong `liva-native-core/src` → **0 hit** (chỉ `aec.rs:125` là comment về 100 ms audio) | Không có benchmark, assert, hay log nào đo TTFT. `verify_duplex.rs:66,140` chỉ assert VAD <15 ms và preemption <10 ms — **không phải** TTFT. Con số 100 ms là mục tiêu (*targeting*), không phải kết quả đo |
| **"Text generation và memory embedding chạy trên llama.cpp contexts TÁCH RỜI (decoupled), lưu ký ức và stream token ĐỒNG THỜI"** | README:23, :27 | **[THIẾU] — SAI SỰ THẬT** | `engine.rs:54-64` `LlamaRouterManager { engine: Option<LlamaEngine> }` — **một** engine; `LlamaEngine { context, mtmd, model }` (`engine.rs:34-42`) — **một** context. `lib.rs:1298-1308` `llm:embed` mượn **chính** `engine.context`. `llm/embed.rs:10` `context.clear_kv_cache()` — **xoá sạch KV cache của chat**. `AppState.llm` là **một** `tokio::sync::Mutex` (`lib.rs:33-46`) khoá chung chat/embed/vision/swap | Không những không "decoupled", việc embed còn **phá** KV cache chat và block toàn bộ LLM. Xem phân tích sâu ở §2.1 |
| **In-Process Embeddings — "memory engine decoupled khỏi chat stream, tránh VRAM thrashing, giữ memory write khỏi hot path"** | README:27 | **[MỘT PHẦN]** — nửa đầu đúng, nửa sau sai | Embedding tính in-process qua `llama.cpp` thật (`llm/embed.rs`, `lib.rs:1298-1308`) — **không** có embedding service ngoài, đúng | Nhưng "decoupled khỏi chat stream" và "off the hot path" là sai: cùng context, cùng Mutex, cộng thêm `clear_kv_cache()` |
| **Sequential Hot-Swap 4B Router ↔ 26B Expert, dùng `mmap`** | README:24 | **[MỘT PHẦN]** | Cơ chế swap có thật: `engine.rs:117-207`, `with_use_mmap(true)` (`engine.rs:140`), giải phóng engine + `sleep(500ms)` cho driver (`engine.rs:125-131`); lệnh `llm:swap_model` (`lib.rs:1265-1281`) | **Không có code tự động swap Router→Expert.** `DEFAULT_EXPERT_MODEL` (`lib.rs:61`) và `ai.expertModel` (`data/liva-config.json:21`) chỉ được **ghi ra** trong config mặc định (`lib.rs:379,444`); grep toàn repo: **không file `.rs` nào đọc `ai.expertModel`**. Chỉ có `configured_router_model_path()` (`lib.rs:119-138`). Ngoài ra "26B" ≠ thực tế: config là `gemma-4-12B-it-qat-UD-Q4_K_XL.gguf` |
| **Router model = Gemma 4B** | README:155 | **[THIẾU] — lỗi thời** | `data/liva-config.json:19` = `Qwen3-VL-2B-Instruct-GGUF/Qwen3-VL-2B-Instruct-Q4_K_M.gguf` | README chưa cập nhật sau khi chuyển sang Qwen3-VL. Router hiện tại là mô hình **đa phương thức 2B**, không phải Gemma 4B |
| **LLM GGUF lấy từ `LIVA_LLM_MODEL_DIR` (mặc định `E:\AI_Models`)** | README:155 | **[THIẾU]** | grep `LIVA_LLM_MODEL_DIR` trong code chạy → chỉ `src/bin/router_stress.rs:68` (bin test). Runtime đọc `data/liva-config.json` → `ai.localModelsDir` + `ai.routerModel` (`lib.rs:129-137`) | Env này **không tồn tại** ở runtime chính; `.env.example:28` cũng sai theo |
| **Nemotron ASR (ONNX) đa ngôn ngữ** | README:25 | **[OK]** | `stt/engine.rs:5-7,25-60` — 3 session ONNX encoder/decoder/joint (RNN-T thật, `decoder_hidden_state: vec![0.0; 2*1*640]`); model có trên đĩa (`models/nemotron-asr/encoder.onnx.data` 690 MB) | Claim đúng, giữ nguyên |
| **`voice:set_language` chuyển vi/en runtime** | README:25, :116 | **[MỘT PHẦN]** | `lib.rs:1220-1233` → `stt/mod.rs:140-149` `set_language(&mut self, code: &str)`; `stt/lang.rs:20-23` `VERIFIED_LANG_IDS: [("vi-VN",33),("en-US",0)]` | Bảng lang_id được xác định **bằng thực nghiệm** (comment `stt/lang.rs:1-17`), chỉ **2/39** id được verify, và **không caller nào phía UI**. "Multilingual" trên thực tế = **song ngữ cố định** |
| **Piper VITS (`vi_VN-vais1000` + `en_US-lessac`)** | README:25 | **[OK]** | `tts/mod.rs:194-254` `load_piper_voices` (quét `vi*.onnx`/`en*.onnx`), `tts/mod.rs:264-275` `piper_for_chunk`, `tts/mod.rs:388-402` ưu tiên Piper; file có thật: `models/piper/vi_VN-vais1000-medium.onnx` + `en_US-lessac-medium.onnx` (63 MB mỗi cái) | Auto-detect tiếng Việt theo dấu (`tts/mod.rs:101-105`). Claim đúng |
| **Kokoro = "optional premium English fallback"** | README:25 | **[THIẾU] — đảo ngược thực tế** | `models/kokoro-v1.0.onnx` **KHÔNG tồn tại** trên đĩa; `tts/engine.rs:27-31` báo lỗi khi thiếu — nhưng session là **lazy** (`ensure_session`), nên `TtsManager::from_bin` vẫn OK vì chỉ cần `node_modules/kokoro-js/voices/af_heart.bin` (**có** trên đĩa, 522 KB) | Kokoro thực chất là **nhánh chết**: nó là fallback cuối (`tts/mod.rs:404-418`), chỉ chạy khi không có Piper voice nào — mà Piper thì đã có sẵn. Đồng thời `af_heart.bin` lại là **điều kiện tiên quyết** để khởi tạo cả `TtsManager` |
| **espeak-ng G2P** | README:25 | **[MỘT PHẦN]** | `tts/espeak.rs:12` `LIVA_ESPEAK_PATH`; gọi ở `tts/mod.rs:404` và `webrtc/pipeline.rs:345` — **chỉ trong nhánh fallback Kokoro** | Piper tự phonemize. espeak-ng **không nằm trên đường chạy chính**, dù CLAUDE.md/README liệt kê nó như prerequisite |
| **Silero VAD ~0.7 s end-of-turn** | README:25 | **[MỘT PHẦN]** — con số **chính xác** | `webrtc/vad.rs:33-51` `VadConfig::from_env()` → `speech_end_threshold` mặc định `get_usize("LIVA_VAD_END_FRAMES", 22)`, 32 ms/frame ⇒ **0,704 s**; `Default` là 45 (1,44 s). Model `models/silero_vad_v6.onnx` có thật | Đây là một trong ít con số trong README khớp code từng chữ số. **Nhưng** chỉ được dựng trong binary standalone (`main.rs:152-164`); Tauri = `None` |
| **Full-duplex streaming + barge-in preemption qua `ws://localhost:8002`** | README:25, :85 | **[MỘT PHẦN] — upstream mic HỎNG PROTOCOL** | Server: `main.rs:446-492`, `handle_ws_connection` `main.rs:494-1037`; pipeline actor `webrtc/pipeline.rs:8-17,80-127`; `OP_SPEAKER_OUT` có thật (`webrtc/pipeline.rs:376-388`). **Nhưng** frame chuẩn là **header 9 byte** (`webrtc/frame.rs:29-35`: `op u8` + `seq u32LE` + `size u32LE`), còn UI gửi mic bằng **header 1 byte**: `useVoicePipeline.ts:344-350` `msg[0] = 0x01; msg.set(PCM, 1)` | Rust đọc 4 byte PCM đầu làm `seq_id`, 4 byte kế làm `payload_size` → gần như luôn `>1MB` → `Err("Payload exceeds 1MB limit")` (`frame.rs:36-38`) → `break` vòng lặp (`main.rs:568-577`). Chiều **xuống** (server→client) thì UI parse **đúng** 9 byte (`WidgetApp.vue:677-688`, `utils/speakerFrame.ts:14`). Grep toàn `liva-ui/src`: **không có hàm encode VoiceFrame nào**. Xem §2.3 |
| **UI kết nối được gateway 8002** | README:185 | **[MỘT PHẦN]** | `useGateway.ts:274-289` — khi `isTauri` thì `connect()` **return sớm**, không tạo WebSocket; mọi lệnh JSON đi qua `invoke("native_ipc_call")` (`useGateway.ts:252-261`). Nhưng `WidgetApp.vue:650-664` lại mở WS thô `ws://127.0.0.1:8002/ws` **bất kể** Tauri | Hai đường truyền song song cùng tồn tại; đường WS chỉ sống nếu binary standalone chạy riêng |
| **Wake word "LIVA" (`LIVA_WAKE_MODE=asr_prefix`)** | README:26, :196 | **[MỘT PHẦN]** | `wake.rs:56-120` `WakeGate::from_env()`, 4 mode `Off/AsrPrefix/TrainedModel/Hybrid`; mặc định **Off** (`wake.rs:66`); logic gate thật ở `main.rs:640-720` (`check_streaming`, `try_wake`, `transcribe_for_wake`). Model đã train có thật: `models/wake_liva_vi.onnx`, `models/wake_liva_en.onnx` | Chỉ được khởi tạo trong `handle_ws_connection` (`main.rs:551`) ⇒ **không tồn tại trong Tauri**. README nói "optional" nhưng không nói rõ nó phụ thuộc WS standalone |
| **Game-mode governor (`LIVA_GAME_MODE=auto`) hạ process priority** | README:26 | **[OK]** | `governor.rs:200-237` `game_mode_active()`, `governor.rs:269-317` `foreground_is_fullscreen()` (Win32 `GetForegroundWindow` + `GetWindowRect`, loại trừ Progman/WorkerW và chính mình), `governor.rs:324-338` `set_process_below_normal` → `SetPriorityClass(BELOW_NORMAL)` (`:335`). Nối dây ở **cả hai** entry: `main.rs:141-151`, `liva-desktop/src-tauri/src/lib.rs:468-473` | Claim của README đúng nguyên văn. Bonus **không** có trong README: hạ `n_gpu_layers` khi game (`lib.rs:292-317` `reload_llm_gpu_layers` + `main.rs:279-310`), nhưng early-return nếu `LIVA_LLM_N_GPU_LAYERS=0` (mặc định) ⇒ mặc định không kích hoạt. Xem thêm dòng dưới về nhánh tải CPU (22/07/2026) |
| **"Sống chung được với mọi workload nặng" — LIVA nhường tài nguyên thay vì độc chiếm** | Trụ cột dự án (không phải README) | **[MỘT PHẦN]** — đã kiểm chứng ở phần CPU, **chưa** ở phần GPU | Từ `733ea1b` (22/07/2026) governor đọc **tải CPU thật**: `system_cpu_percent()` (`governor.rs:127-173`) lấy `GetSystemTimes` + `GetProcessTimes`, `external_cpu_percent()` (`governor.rs:103-121`) trừ phần CPU của chính LIVA rồi quy ra %. Chế độ tiết kiệm bật khi **fullscreen HOẶC CPU ngoài ≥ ngưỡng** (`governor.rs:213-222`). Ngưỡng `LIVA_BUSY_CPU_PERCENT` mặc định **80** (`governor.rs:79,82-88`), đặt `0` để tắt hẳn nhánh CPU. **7 unit test** khoá phép tính trong `mod tests` ở `governor.rs:368-541` (đáng chú ý `tru_phan_cpu_cua_chinh_liva` `:411`, `kep_lai_khi_so_lieu_lech` `:402`, `nguong_doc_tu_env_va_kep_gia_tri` `:441`) + **1 smoke test nạp tải thật trên phần cứng** (`do_duoc_tai_that_tren_may`, `governor.rs:475-477`) nhưng test này gắn `#[ignore]` nên **không chạy trong `cargo test` mặc định lẫn trên CI** — phải chạy tay `cargo test --lib governor -- --ignored` | **Đã kiểm chứng:** governor phản ứng với tải CPU của tiến trình khác, và **không** tự sập priority khi chính LIVA chạy LLM (đây là vòng phản hồi ngược mà việc trừ `GetProcessTimes` sinh ra để chặn). **Chưa kiểm chứng / chưa có:** không đọc tải GPU hay VRAM (không NVML, không counter GPU) — tải GPU chỉ được suy ra gián tiếp qua dấu hiệu fullscreen. Ngoài ra nhánh CPU **chỉ chi phối process priority**: `game_mode_active_now()` (`governor.rs:261-267`) — hàm mà đường hạ `n_gpu_layers` (`main.rs:298`, Tauri `lib.rs:444`) và đường vision (`vision/capture.rs:132`) dùng — vẫn **chỉ hỏi fullscreen**. ⇒ "sống chung với render/build nặng" mới đúng ở mức hạ priority |
| **Deep Verification Suite (`verify_round2`, `router_stress`, `voice_stress`, `verify_duplex`)** | README:28 | **[OK]** | 4 file tồn tại trong `liva-native-core/src/bin/` (tổng 17 bin). `verify_duplex.rs:66` assert VAD <15 ms, `verify_duplex.rs:140` preemption <10 ms | Test suite thật nhưng nhỏ, và **con số chỉ đọc đúng khi kèm điều kiện feature** (đo 22/07/2026 bằng `cargo test -- --list`): ở **build mặc định**, `tests/` chỉ sinh **9 hàm** (`integration_tests` 7, `verify_commands` 1, `panic_cleanup` 1) — ba file `sandbox_stress` / `self_correction_stress` / `swarm_stress_tests` sinh **0 test** vì cả file nằm sau `#![cfg(feature = "experimental")]` (`:5` mỗi file). Với `--features experimental` thì thành **19 hàm** (thêm `sandbox_stress` 3, `self_correction_stress` 4, `swarm_stress_tests` 2, và `integration_tests` lên 8 do `tests/integration_tests.rs:331`). Tổng toàn bộ `cargo test`: **206 pass + 1 ignored** mặc định, **226 pass + 1 ignored** với `experimental` |
| **CSP nghiêm ngặt** | README:29 | **[MỘT PHẦN]** | `liva-desktop/src-tauri/tauri.conf.json` `app.security.csp`: `default-src 'self'; connect-src 'self' ipc: http://localhost:5173 ws://localhost:5173 ws://localhost:8002 ws://127.0.0.1:8002; …` | CSP có thật và **cưỡng chế** được offline (xem §3.3), nhưng có `'unsafe-inline'` cho `script-src`/`style-src` — không phải "nghiêm ngặt" tuyệt đối |
| **Argon2id cho Stronghold vault** | README:29, :90 | **[MỘT PHẦN]** | `liva-desktop/src-tauri/src/lib.rs:139-148` `argon2::Variant::Argon2id`, `hash_length = 32`; `read_vault_key`/`write_vault_key` (`lib.rs:151-226`); dep `rust-argon2 = "2.1.0"` | `get_stronghold_credentials()` (`lib.rs:123-129`) fallback password = `"LIVA_DEFAULT_SECURE_PASSWORD"` và salt là chuỗi cố định khi không set env ⇒ két sắt **mặc định vô nghĩa về bảo mật**. KDF đúng nhưng bí mật đầu vào công khai trong source |
| **AES-256-GCM cho stored memories** | README:29 | **[MỘT PHẦN]** | `crypto.rs` `EncryptionEngine`; dùng ở `db::set_fact`/`get_fact` (`lib.rs:991,1013`) và giải mã trong `get_memory_data` (`lib.rs:876`) | **Chỉ bảng `facts`** được mã hoá. `agent_checkpoints.state_json` (chứa **nguyên văn hội thoại**) lưu plaintext (`agent/memory.rs:24`); `vectors_meta.content` cũng plaintext (`db.rs:566`). Key mặc định là `"0" × 32` (`main.rs:63`, `liva-desktop/src-tauri/src/lib.rs:270-271`) — không panic, chạy tiếp |
| **SQLite WAL sống sót SIGKILL, zero corruption** | README:30 | **[MỘT PHẦN]** | `db.rs:44` `PRAGMA journal_mode = WAL`, `wal_autocheckpoint=500`, `busy_timeout=5000`; test `db.rs:912-916` assert `journal_mode == WAL` | WAL là thật và đúng. Nhưng **không tìm thấy test SIGKILL nào** trong `tests/` — "survives SIGKILL with zero data corruption" là suy luận từ tính chất WAL, không phải kết quả kiểm chứng |
| **Hybrid search `sqlite-vec` + FTS5** | README:30, :112 | **[MỘT PHẦN] — API có, không ai gọi** | Hạ tầng thật: `db.rs:318` `vectors_fts USING fts5(...)`, `db.rs:348` `vec_idx USING vec0(embedding int8[384])`, `db.rs:839` `search_hybrid_vectors(...)`; lệnh `memory:search_hybrid` (`lib.rs:1024-1083`), `memory:upsert_vector` (`lib.rs:1084-1169`) | **0 caller từ `liva-ui`** (grep `memory:search_hybrid\|memory:upsert_vector` trong `liva-ui/src` → 0). Chỉ `mobile_client/src/components/MemoryTaskBoard.vue:310,323` gọi. Client phải **tự tính vector** rồi truyền vào. Thêm: `vec_idx` là **384 chiều int8**, còn `llm:embed` trả embedding của Qwen3-VL-2B ⇒ **không khớp chiều** |
| **Bộ nhớ 5 tầng L0→L3, Reflection Daemon chưng cất L1→L2, Nightly Cron dựng Knowledge Graph L3** | README:55-75 | **[THIẾU]** | Schema có đủ: `turn_layer_nodes` (`db.rs:243`), `events` + `consolidation_status` (`db.rs:221-232`), `consolidation_checkpoints` (`db.rs:272`), `dlq_consolidation` (`db.rs:280`), `l3_nodes`/`l3_edges`. **Nhưng grep toàn `liva-native-core/src/` + `liva-desktop/src-tauri/src/`: KHÔNG có `INSERT INTO events`, `INSERT INTO turn_layer_nodes`, `INSERT INTO l3_nodes` nào** (xác minh lại 2026-07-21: 0 hit) | Toàn bộ 5 tầng là **schema rỗng**. `chat:completion` (`lib.rs:1318-1393`) hoàn toàn **stateless**: không đọc RAG, không ghi memory. Trường `l0_5` trong `get_memory_data` là literal chuỗi rỗng (`lib.rs:969`). Grep `Reflection\|distill\|cron\|nightly\|Consolidat` trong Rust → chỉ khớp **tên cột** và câu SELECT hiển thị (`lib.rs:894,925`). Xem §2.2 |
| **Memory Dashboard 2D realtime qua WebSocket, xem L0/L1/L2** | README:34, :197 | **[MỘT PHẦN]** | `get_memory_data` (`lib.rs:844-978`) query thật 4 nguồn (`turn_layer_nodes`, `facts`, `events`, `vectors_meta`) | Truy vấn thật nhưng **các bảng không bao giờ được ghi** ⇒ dashboard hiển thị rỗng (trừ `facts` nếu người dùng tự set). Trong Tauri nó đi qua `invoke`, **không phải WebSocket** (`useGateway.ts:275-288`) |
| **Native Screen Vision (`vision:capture`, `vision:get_changed_regions`)** | README:31, :117 | **[OK]** | `lib.rs:249-273` capture (base64), `lib.rs:274-288` add/remove_region, `lib.rs:289-336` get_changed_regions (lần đầu trả baseline `difference = 1.0`), `lib.rs:337-343` set_config; capturer thật `vision/capture.rs:160-244` dùng crate `xcap` + cache monitor | "Pure-Rust" đúng (xcap là Rust). Có thêm `vision:ask` (`lib.rs:1394-1445`) đa phương thức Qwen3-VL — **README không nhắc** |
| **Self-Correction Loop: sandbox chạy test → đọc log → "hỏi local LLM" xin patch → retry đến khi xanh** | README:32, :108 | **[THIẾU] — không có agent LLM nào** | Loop có thật và đầy đủ: `evolution/mod.rs:92-163` `SelfCorrectionLoop::run(project_path, source_file_path)`, `max_retries = 3` (`mod.rs:96`), `BackupGuard` khôi phục file (`mod.rs:52-90`), `extract_error` (`mod.rs:165-193`); `Sandbox::run_tests` spawn `cargo test` | **`trait CodeAgent` (`mod.rs:6-12`) chỉ có 3 impl, TẤT CẢ là Mock trong test**: `evolution/mod.rs:206` `MockCodeAgent`, `tests/sandbox_stress.rs:172`, `tests/self_correction_stress.rs:57` (hai toạ độ test trôi +6 dòng sau khi thêm `#![cfg(feature = "experimental")]`). Grep `SelfCorrectionLoop` trong `src/` (kiểm lại 22/07/2026) → chỉ 3 hit, tất cả trong chính `evolution/mod.rs` (`:14` định nghĩa, `:92` impl, `:284` test). **0 call site sản phẩm.** Không có adapter nối `LlamaRouterManager` vào `CodeAgent` ⇒ "hỏi local LLM" **chưa tồn tại**. Từ `4c08f18` (22/07/2026) cả `src/evolution/` (428 dòng) **không còn được biên dịch** vào build mặc định — nằm sau `#[cfg(feature = "experimental")]` (`lib.rs:14`) |
| **Ghost Mode UI: Tauri v2 overlay trong suốt, click-through** | README:33, :198 | **[OK]** | `tauri.conf.json` window `widget`: `transparent:true, decorations:false, alwaysOnTop:true, skipTaskbar:true, shadow:false`; `liva-desktop/src-tauri/src/lib.rs:76-78` `toggle_ghost_mode` → `window.set_ignore_cursor_events(enabled)`; poll con trỏ tự động bật/tắt (`lib.rs:534,547`); UI: `platform/TauriAdapter.ts:11-17`, `App.vue:16,20` | Nối dây end-to-end đầy đủ. Một trong những tính năng "demo được ngay" đáng tin nhất |
| **Planner/Executor Loop + persistent task graph** | README:107 | **[MỘT PHẦN] / gần như [THIẾU]** | Có `agent/graph.rs` `StateGraph` + `build_pipeline_graph` (4 node: router / tool_exec / chat_completion / vision) — nhưng **chỉ chạy trên đường voice/WebRTC** (`webrtc/pipeline.rs:246+`), không dùng cho `chat:completion`. `task_plan_chat` (`lib.rs:708-808`) là **một lượt LLM one-shot**, không sinh plan có cấu trúc, không có executor tiêu thụ | Router phân loại bằng `contains()` trên chuỗi thường (`agent/graph.rs:95-123`), **không dùng LLM**. `agent/dispatcher.rs` (swarm, 187 dòng) **không có call site nào trong `src/`**; logic agent là stub hard-code (`dispatcher.rs:116-136`). Từ `4c08f18` (22/07/2026) file này nằm sau `#[cfg(feature = "experimental")]` (`agent/mod.rs:4`) ⇒ **không còn trong build mặc định** |
| **"Persistent agent memory" / checkpoint hội thoại** | README:107 | **[MỘT PHẦN] — hỏng ngữ nghĩa** | `agent/memory.rs:14-33` `save_checkpoint(thread_id, …)` ghi `INSERT OR REPLACE INTO agent_checkpoints`; đọc lại ở `webrtc/pipeline.rs:246-260` với `thread_id = session_id.to_string()` | `session_id` **tăng ở mọi sự kiện VAD** (`webrtc/pipeline.rs:437-439` trong `cancel_active_operations`, gọi từ `handle_vad_start`/`vad_end`/`interrupted`) ⇒ `load_checkpoint` **luôn** trả `None`. Hội thoại **không có trí nhớ đa lượt**; bảng phình 1 hàng/lượt nói |
| **GitNexus Automation + AI pre-commit hook audit diff bằng local LLM** | README:109 | **[MỘT PHẦN]** | Hook thật: `.husky/pre-commit` chạy `lint-staged` rồi `node scripts/ai-pre-commit.cjs` và chặn commit nếu fail | README ghi **sai tên file** (`.js` vs thực tế `.cjs`). Script gọi `AI_BASE_URL` mặc định `http://127.0.0.1:8000/v1` (`ai-pre-commit.cjs:47`) — tức **một llama-server ngoài trên port 8000**, không phải `liva-native-core`. Model mặc định `gemma-4-E4B-it-Q6_K.gguf` (`ai-pre-commit.cjs:49`) — lệch config thật |
| **Obsidian Knowledge Vault qua MCP (`read_markdown`, `search_vault`, `write_markdown`)** | README:113 | **[MỘT PHẦN] — MCP server Rust MỒ CÔI** | `mcp/server.rs:38-60` `NativeMcpServer::list_tools()` khai báo 4 tool (thêm `control_smarthome`); được dựng và nhét vào `AppState` (`main.rs:168,251`; `liva-desktop/src-tauri/src/lib.rs:347,366`) | **`handle_command` không có nhánh nào gọi `state.mcp_server`** — grep `mcp_server` trong `lib.rs` (2026-07-21) → **đúng 1 hit**: `lib.rs:44` khai báo field. Mọi `call_tool` chỉ xuất hiện trong `tests/integration_tests.rs:41,62,78,88`. Bản MCP thực sự chạy được là **TypeScript riêng**: `teamwork_projects/obsidian_llm_wiki/src/server.ts:21,50,78`. `mcp/client.rs` (49 dòng) cũng không ai gọi |
| **Telegram Remote-Control Hub + ID allow-list** | README:120 | **[MỘT PHẦN]** | `telegram.rs:33,41,48` `allowed_ids: HashSet<String>`; `telegram.rs:73-78` `is_authorized()`; gate ở `telegram.rs:89,280`; HTTP thật `reqwest` + `api.telegram.org` (`telegram.rs:324-326`); lệnh `telegram:send_text` (`lib.rs:1459-1473`); khởi động ở `main.rs:320-341` từ `TELEGRAM_BOT_TOKEN` + `TELEGRAM_ALLOWED_IDS` | **Không khởi động trong Tauri.** `/ask` và tin nhắn không tới agent loop. **Đính chính so với bản khảo sát thô:** allow-list rỗng là **fail-closed**, không phải fail-open — `telegram.rs:74-76` `if self.allowed_ids.is_empty() { return false; }`. Đã đọc lại code ngày 2026-07-21 để xác nhận. Rủi ro thật nằm ở chỗ khác: `/cat` **không sandbox path** (xem §3.4) |
| **Smart Home Control (`integration:smart_home_control`)** | README:121 | **[THIẾU] — STUB** | `integrations/smart_home.rs:51-67` `execute()` chỉ validate enum rồi `tracing::info!` + `Ok(format!("Device '{}' successfully turned '{}'.", …))` | **Không có protocol, không có thiết bị, không có I/O.** Trả chuỗi thành công **vô điều kiện** — nguy hiểm hơn cả việc thiếu, vì LLM sẽ báo cáo với người dùng là đã bật đèn |
| **Email (IMAP) & Zalo OA "configured via environment variables"** | README:122 | **[THIẾU]** | grep `imap\|IMAP\|zalo\|Zalo` trong `liva-native-core/src`: chỉ `lib.rs:512` (chuỗi status giả `"zalo": {"status":"offline"}`) và `tts/normalizer.rs:602` (`"zalo" => "za lô"`, luật đọc TTS) | Không có client, không có env được đọc, không có lệnh |
| **"Proactive / digest"** — trụ cột **"chủ động"** (LIVA tự quan sát, tự mở lời) | `data/liva-config.json:38-60`, trụ cột dự án | **[THIẾU]** — và từ 22/07/2026 còn **xa hơn trước** | grep `proactive\|digest` trong Rust → chỉ `lib.rs:475` (`"proactiveEnabled": true` trong config mặc định). Hạ tầng quan sát thụ động nằm ở `src/passive/` (647 dòng: `hook.rs` 328 + `buffer.rs` 314 + `mod.rs` 5) — tham chiếu duy nhất trong toàn repo là `lib.rs:13 pub mod passive;`, mà dòng đó nay đã bị `#[cfg(feature = "experimental")]` (`lib.rs:12`) chặn | 20+ khoá config **không có code đọc**. Người dùng bật/tắt trong `SettingsView.vue` chỉ ghi vào JSON, không có consumer. Sau `4c08f18`, `passive/` **không còn được biên dịch vào build mặc định** ⇒ khoảng cách giữa tuyên bố "chủ động" và code đang chạy còn rộng hơn trước. **Nhưng đây là quyết định đúng về an toàn:** `passive/hook.rs:216-231` cài `SetWindowsHookExW(WH_KEYBOARD_LL, …)` + `WH_MOUSE_LL` — tức một **keylogger toàn hệ thống đầy đủ chức năng**, chưa có cổng xin đồng ý người dùng, chưa có UI bật/tắt, chưa có chỉ báo đang ghi. Không nên nằm trong binary giao cho beta tester trước khi có cổng đó |
| **100% offline, no CDN** | README:29 | **[MỘT PHẦN]** — đúng về asset, sai về tuyệt đối hoá | grep `cdn\|unpkg\|jsdelivr\|googleapis` trong `liva-ui/src` → chỉ 1 hit là **placeholder text** trong ô input (`ApiManagementView.vue:240`). MediaPipe wasm + model **đã vendor local** | Runtime của đường chạy chính thực sự im lặng về mạng, nhưng có 4 ngoại lệ phải nói rõ. Xem §3 |
| **VieNeu-TTS, GTCRN denoise, Smart Turn, AEC, Parakeet-vi, Qwen3-VL** | *(README **KHÔNG** nhắc)* | Có code, đa số opt-in | `tts/mod.rs:156-189` (`LIVA_TTS_VIENEU=1`); `main.rs:181-209` GTCRN **bật mặc định**; `main.rs:214-230` turn-shadow (`=1`, log-only); `main.rs:234-238` AEC (`=1`); `stt/mod.rs:49-51,108-136` Parakeet lazy-load 2,4 GB (`LIVA_STT_VI_ENGINE=parakeet`); `lib.rs:1394-1445` `vision:ask` | README **thiếu toàn bộ nhóm này** — tài liệu lạc hậu theo hướng **dưới-báo cáo**, ngược chiều với các mục ở trên. Đây là phần LIVA đang mạnh mà không ai biết |

Bảng trên là **phán quyết**, không phải đặc tả. Chi tiết kỹ thuật đầy đủ của từng hạng mục nằm ở tài liệu sở hữu tương ứng:

> 📌 Ngưỡng VAD/AEC/denoise, backend TTS, engine STT: [Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md)
> 📌 Cấu hình LLM, persona, chống injection: [Hệ LLM và prompt](../01-ban-ve/04-he-llm-va-prompt.md)
> 📌 Máy trạng thái agent, StateGraph 4 node: [Agent, bộ nhớ và tiến hoá](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md)
> 📌 ERD SQLite, 15 bảng dữ liệu, sơ đồ mã hoá: [Tầng dữ liệu và bảo mật](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md)
> 📌 Bảng tích hợp ngoài (Telegram, MCP, smart home): [Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md)
> 📌 Bảng biến môi trường và lệch `.env.example` vs code: [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md)

---

## 2. Ba claim sai nghiêm trọng nhất

Xếp theo mức lệch giữa lời nói và code, không theo mức nghiêm trọng kỹ thuật.

### 2.1 "Decoupled `llama.cpp` contexts — lưu ký ức và stream token đồng thời"

**Nguồn:** README:23 và README:27 (mục *In-Process Embeddings* lặp lại ý này: *"The memory engine is decoupled from the chat stream, preventing VRAM thrashing and keeping memory writes off the hot path."*).

**Vì sao đây là claim sai nghiêm trọng nhất:** nó không phải "chưa làm" mà là **ngược lại điều được quảng cáo**. Ba tầng bằng chứng chồng lên nhau:

1. **Một engine, một context.** `engine.rs:54-64` định nghĩa `LlamaRouterManager { engine: Option<LlamaEngine> }` — trường số ít, `Option`, không phải collection. `LlamaEngine { context, mtmd, model }` (`engine.rs:34-42`) cũng chỉ giữ **một** `LlamaContext`.
2. **Embed mượn chính context của chat.** `lib.rs:1298-1308` (arm `llm:embed`) lấy `engine.context` ra dùng trực tiếp — không tạo context thứ hai, không clone model handle sang context mới.
3. **Embed phá KV cache của chat.** Dòng đầu tiên của `get_embedding` là:

   ```rust
   // liva-native-core/src/llm/embed.rs:10
   context.clear_kv_cache();
   ```

   Nghĩa là mỗi lần tính embedding, toàn bộ prefix cache của cuộc hội thoại đang chạy bị xoá. Lượt chat kế tiếp phải prefill lại từ đầu.
4. **Một Mutex khoá tất cả.** `AppState.llm` là **một** `tokio::sync::Mutex` (`lib.rs:33-46`), dùng chung cho chat, embed, vision và swap model. Hai thao tác không thể chạy đồng thời về mặt vật lý.

**Hệ quả thực tế:** nếu ai đó nối dây memory pipeline (§2.2) đúng như README mô tả — ghi ký ức trong lúc stream token — thì mỗi lần ghi sẽ **vừa block stream vừa xoá cache**, làm TTFT của lượt sau tệ đi đáng kể. Claim này không chỉ sai, nó còn che mất một hạn chế kiến trúc cần được giải quyết trước khi bật memory.

**Cần gì để claim thành đúng:** một `LlamaContext` thứ hai (hoặc một `LlamaModel` nạp riêng ở chế độ embedding, `n_ctx` nhỏ), tách `AppState.llm` thành `llm_chat` và `llm_embed` với hai Mutex độc lập, và bỏ `clear_kv_cache()` khỏi đường chat.

```mermaid
flowchart LR
    subgraph CLAIM["README nói"]
        C1["Chat context"] --> G1[GPU]
        C2["Embed context"] --> G1
        C1 -.->|"song song"| C2
    end
    subgraph REAL["Code thật"]
        M["AppState.llm<br/>MỘT tokio Mutex<br/>lib.rs:33-46"] --> E["LlamaEngine<br/>MỘT context<br/>engine.rs:34-42"]
        E --> CH["chat:completion"]
        E --> EM["llm:embed"]
        EM -->|"clear_kv_cache()<br/>embed.rs:10"| X(["KV cache chat<br/>BỊ XOÁ"])
    end
```

### 2.2 Bộ nhớ 5 tầng L0→L3 (Reflection Daemon, Nightly Cron, Knowledge Graph)

**Nguồn:** README:55-75 — một mục dài mô tả kiến trúc bộ nhớ phân tầng với daemon chưng cất và cron đêm.

**Thực tế:** schema đầy đủ và được thiết kế nghiêm túc trong `db.rs` — `turn_layer_nodes` (`db.rs:243`), `events` + `consolidation_status` (`db.rs:221-232`), `consolidation_checkpoints` (`db.rs:272`), `dlq_consolidation` (`db.rs:280`), `l3_nodes`/`l3_edges`. Có cả dead-letter queue cho consolidation, tức là người thiết kế đã nghĩ tới lỗi vận hành.

**Nhưng không một dòng Rust nào ghi vào các bảng đó.** Grep `INSERT INTO events`, `INSERT INTO turn_layer_nodes`, `INSERT INTO l3_nodes` trên toàn bộ `liva-native-core/src/` và `liva-desktop/src-tauri/src/` → **0 hit** (kiểm lại 2026-07-21). Grep `Reflection|distill|cron|nightly|Consolidat` trong Rust chỉ khớp **tên cột** trong các câu `SELECT` hiển thị (`lib.rs:894,925`).

Bổ sung ba chi tiết làm rõ mức độ:

- `chat:completion` (`lib.rs:1318-1393`) **hoàn toàn stateless**: không truy vấn RAG trước khi sinh, không ghi gì sau khi sinh.
- Trường `l0_5` trong `get_memory_data` là **literal chuỗi rỗng** (`lib.rs:969`) — dashboard được thiết kế để hiển thị một tầng mà backend còn chưa định nghĩa dữ liệu.
- Đường duy nhất thực sự ghi trạng thái hội thoại là `agent/memory.rs:14-33` `save_checkpoint` — nhưng nó **hỏng ngữ nghĩa** vì `session_id` tăng ở mọi sự kiện VAD (`webrtc/pipeline.rs:437-439`), nên `load_checkpoint` luôn trả `None`. Bảng chỉ phình lên, không bao giờ được đọc lại.

**Hệ quả thực tế:** Memory Dashboard (README:34) chạy đúng về mặt kỹ thuật nhưng **hiển thị rỗng**, vì nó query những bảng không bao giờ có dữ liệu. Với beta tester, đây là thứ dễ bị phát hiện nhất trong 5 phút đầu.

### 2.3 "Full-duplex + barge-in runs over `ws://localhost:8002`"

**Nguồn:** README:25, README:85.

Claim này đứt ở **ba tầng độc lập** — sửa một tầng vẫn không chạy được:

**Tầng 1 — script không khởi động server đó.** `scripts/start_all.ps1:24` kill mọi tiến trình giữ port 8002, rồi chạy `liva-ui` (dòng 52) và `npx.cmd tauri dev --no-dev-server` (dòng 66). Không dòng nào chạy `liva-native-core`. Comment `liva-desktop/src-tauri/src/lib.rs:460` khẳng định ngược lại và **sai**.

**Tầng 2 — Tauri không có VAD.** Ngay cả khi WS server chạy, bản desktop dựng `AppState` với:

```rust
// liva-desktop/src-tauri/src/lib.rs:362-365
vad: tokio::sync::Mutex::new(None),
denoiser: tokio::sync::Mutex::new(None),
turn_shadow: tokio::sync::Mutex::new(None),
aec: tokio::sync::Mutex::new(None),
```

Không VAD ⇒ không có sự kiện `vad_start`/`vad_end` ⇒ không có preemption ⇒ không có barge-in, bất kể protocol đúng hay sai.

**Tầng 3 — hợp đồng khung mic sai.** Đây là lỗi cụ thể nhất và dễ sửa nhất. Core decode header **9 byte**:

```rust
// liva-native-core/src/webrtc/frame.rs:29-38
if src.len() < 9 { return Ok(None); }
let op_code = src[0];
let seq_id = u32::from_le_bytes([src[1], src[2], src[3], src[4]]);
let payload_size = u32::from_le_bytes([src[5], src[6], src[7], src[8]]) as usize;
if payload_size > 1024 * 1024 { return Err("Payload exceeds 1MB limit".to_string()); }
```

> 📌 Đặc tả khung nhị phân 9 byte và bảng opcode đầy đủ: [Giao thức IPC và WebSocket](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md)

UI gửi header **1 byte**:

```ts
// liva-ui/src/composables/useVoicePipeline.ts:344-350
const msg = new Uint8Array(1 + pcmBuffer.byteLength);
msg[0] = 0x01; // Audio header
msg.set(new Uint8Array(pcmBuffer), 1);
wsRef.send(msg);
```

Rust sẽ đọc 4 byte PCM đầu tiên làm `seq_id` và 4 byte kế làm `payload_size`. Với PCM float32, giá trị này gần như luôn `> 1MB` ⇒ `Err` ⇒ `break` vòng lặp (`main.rs:568-577`) ⇒ **kết nối đứt ngay khung mic đầu tiên**.

Điều đáng chú ý: **chiều xuống thì đúng.** UI parse `OP_SPEAKER_OUT` bằng đúng 9 byte (`WidgetApp.vue:677-688`, `liva-ui/src/utils/speakerFrame.ts:14`). Tức là ai đó đã viết decoder chuẩn cho một chiều mà quên encoder cho chiều kia. Grep toàn `liva-ui/src`: **không có hàm encode `VoiceFrame` nào tồn tại**.

```mermaid
sequenceDiagram
    participant UI as liva-ui (useVoicePipeline.ts)
    participant WS as WS :8002 (main.rs:494+)
    participant F as frame.rs decode
    UI->>WS: mic frame — header 1 byte<br/>[0x01][PCM…]
    WS->>F: decode(BytesMut)
    Note over F: src[1..5] = PCM bytes → seq_id<br/>src[5..9] = PCM bytes → payload_size
    F-->>WS: Err("Payload exceeds 1MB limit")
    WS-->>UI: break vòng lặp — đóng kết nối
    Note over UI,WS: Chiều xuống OP_SPEAKER_OUT thì UI<br/>parse ĐÚNG 9 byte (speakerFrame.ts:14)
```

---

## 3. Kiểm chứng tuyên bố "100% offline"

### 3.1 Kết luận ngắn trước

**Đường chạy chính của LIVA (Tauri shell + `liva-ui` + `liva-native-core`) không có một lời gọi mạng ra Internet nào.** Grep `https?://` trên toàn bộ `liva-native-core/src/` trả về **đúng 1 hit**: `telegram.rs:324`. `reqwest::get` duy nhất ở `telegram.rs:326`. Không có client OpenAI/Gemini/Anthropic trong Rust (grep `openai|anthropic|googleapis|api_key|base_url` trên `src/` → chỉ 2 hit, đều là **comment** ở `llm/prompt/mod.rs:45,149`).

Nhưng tuyên bố "100% offline" vẫn có **4 vết nứt** phải nói rõ: Telegram (opt-in), `liva-voice/` (Python, ngoài đường chạy), build-time CDN, và model weights out-of-band.

### 3.2 Bảng đầy đủ mọi điểm ra mạng

Ký hiệu: **(a)** bắt buộc cho tính năng lõi · **(b)** tuỳ chọn / người dùng bật · **(c)** chỉ test/dev/dead-code.

#### 3.2.1 Rust core (`liva-native-core`)

| Nơi gọi (file:dòng) | Đích | Loại | Ảnh hưởng offline |
|---|---|---|---|
| `telegram.rs:324` — `format!("https://api.telegram.org/file/bot{}/{}", token, file.path)` | `api.telegram.org` (Telegram CDN) | **(b)** | Chỉ chạy khi user gửi voice message qua bot; bot chỉ spawn khi có `TELEGRAM_BOT_TOKEN` |
| `telegram.rs:326` — `reqwest::get(&file_url).await?` | như trên | **(b)** | Tải file `.ogg` từ server Telegram |
| `telegram.rs:54` `TelegramBotManager::start()` → `teloxide` `Dispatcher` (long-polling) | `api.telegram.org/bot<token>/getUpdates` liên tục | **(b)** | **Đây mới là kết nối THƯỜNG TRỰC ra Internet**, không phải chỉ 1 request. Gate: `main.rs:320-341` `if let Some(token) = std::env::var("TELEGRAM_BOT_TOKEN").ok()` |
| `lib.rs:1459-1472` — arm IPC `"telegram:send_text"` → `Bot::new(token).send_message(...)` | `api.telegram.org` | **(b)** → thực tế **(c)** | Arm này nằm trong `handle_command`, tức **dùng chung với Tauri shell** → ngay cả bản desktop (vốn không spawn bot) vẫn gửi được tin ra Internet nếu env có token. Hiện **không UI nào gọi** (grep `telegram:send_text` trong `liva-ui/src` = 0 hit) |
| `liva-native-core/Cargo.toml:24-25` — `teloxide 0.13` + `reqwest 0.11` là dependency **vô điều kiện** | — | — | Binary **luôn** chứa HTTP/TLS client. Không có `#[cfg(feature)]` nào bọc `mod telegram` |
| ~~`webrtc/signaling.rs:24` — `TcpListener::bind("0.0.0.0:{port}")`~~ | — | — | **ĐÃ XOÁ 22/07/2026** (`510c9e2`). File `webrtc/signaling.rs` (63 dòng) không còn tồn tại; `webrtc/mod.rs` chỉ còn khai báo `frame/vad/denoise/turn_shadow/aec/pipeline`. Điểm bind `0.0.0.0` duy nhất trong lõi đã biến mất |
| `main.rs:452-454` — WS server bind `LIVA_SERVER_HOST` mặc định `127.0.0.1:8002` | loopback | (a) | **Không ra Internet.** Mặc định an toàn; nhưng có thể đổi thành `0.0.0.0` qua env |
| ~~`liva-native-core/Cargo.toml:26` — crate `webrtc = "0.12.0"`~~ | — | — | **ĐÃ GỠ 22/07/2026** (`510c9e2`): crate không còn trong `Cargo.toml`, kéo theo 45 crate khỏi cây phụ thuộc (`stun`, `turn`, `srtp`, `rtp`, `rtcp`, `webrtc-*`…). Mọi `webrtc::` trong mã đều là module **nội bộ** `crate::webrtc`. Kết luận cũ vẫn đúng và nay còn chắc hơn: **không có PeerConnection, không có STUN/TURN** trong lõi |
| `liva-native-core/static/app.js:30` — `iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]` | **Google STUN** | **(c)** | File tĩnh trong `static/` — grep `static`/`ServeDir`/`include_str` trong `src/` = **0 hit**, không server nào phục vụ nó. **Dead demo page.** Đây là chỗ **duy nhất** trong repo có STUN Google |

#### 3.2.2 Frontend (`liva-ui`) + Tauri shell

| Nơi gọi (file:dòng) | Đích | Loại | Ảnh hưởng offline |
|---|---|---|---|
| `liva-ui/src/composables/useGateway.ts:296-297` — `ws://${wsHost}:8002/ws` | loopback (hoặc `window.location.hostname` nếu phục vụ qua LAN, `:294`) | (a) | Nội bộ |
| `liva-ui/src/App.vue:124`, `WidgetApp.vue:652` — `ws://127.0.0.1:${port}/ws` | loopback | (a) | Nội bộ |
| `liva-ui/src/App.vue:96` và `WidgetApp.vue:539` — `safeFetch("http://127.0.0.1:3000/api/sensory-capture", {method:"POST"})` | **localhost:3000** | **(c)** | **Không có server nào ở port 3000 trong repo** (grep `sensory-capture` = chỉ 2 hit này + coverage report). Tàn dư stack Node đã xoá. Ngoài ra **CSP chặn**: `tauri.conf.json:45` `connect-src` **không có** `http://127.0.0.1:3000` → trong bản Tauri, call này bị CSP chặn cứng |
| `liva-ui/src/composables/useFaceTracking.ts:200-206` — `FilesetResolver.forVisionTasks("/assets/wasm")`, `modelAssetPath: "/assets/models/face_landmarker.task"` | **local** | (a) | MediaPipe **không** tải wasm từ CDN — đã vendor: `liva-ui/public/assets/wasm/{vision_wasm_internal.js,.wasm}` và `public/assets/models/face_landmarker.task` tồn tại thật. **Điểm cộng lớn cho offline** |
| `liva-ui/index.html:20`, `widget.html:13` — `<script src="/assets/live2d.min.js">` | local | (a) | Bundle local. Chuỗi `https://get.webgl.org` bên trong chỉ là **text trong `console.error`**, không phải fetch |
| `liva-ui/src/components/dashboard/AvatarGallery.vue:176,181` — `window.open('https://hub.vroid.com/')`, `window.open('https://www.mixamo.com/')` | VRoid Hub, Mixamo | **(b)** | Người dùng bấm nút "tải model" → mở trình duyệt ngoài. Không phải LIVA tự gọi |
| `liva-ui/src/components/dashboard/{ApiManagementView,AISettings}.vue:240,269,184` — `placeholder="https://generativelanguage.googleapis.com/…"`, `https://api.groq.com/…`, `https://api.openai.com/v1` | — | **(c)** | **Chỉ là `placeholder` HTML**, không phải endpoint được gọi. Xem §3.5 |
| `liva-desktop/src-tauri/Cargo.toml` | — | — | **Không có `tauri-plugin-http`, không có `tauri-plugin-updater`.** Plugin: `opener`, `dialog`, `stronghold`, `process`. `capabilities/default.json` cũng không cấp quyền HTTP → **không có auto-update phone-home** |
| `liva-desktop/src-tauri/src/lib.rs` | — | — | grep `telegram\|reqwest\|http` = **0 hit** → **bản desktop KHÔNG spawn Telegram bot.** Đường chạy chính hoàn toàn im lặng về mạng |

#### 3.2.3 `liva-voice/` (Python, port 8765 — KHÔNG được app khởi động)

| Nơi gọi (file:dòng) | Đích | Loại | Ảnh hưởng offline |
|---|---|---|---|
| `liva-voice/liva_api.py:272,278` (`POST /tts`) và `:318,322` (WS `/ws`) — `edge_tts.Communicate(text, voice_id, rate="+10%")` | **Microsoft Azure Edge TTS** (`speech.platform.bing.com`) | **(a)** với chính endpoint đó | **Gửi NGUYÊN VĂN text của người dùng lên Microsoft.** Đây là vi phạm offline rõ ràng nhất về mặt riêng tư — nếu ai đó chạy service này |
| `liva-voice/src/audio_processor.py:162-191` — subprocess `yt-dlp` | YouTube / URL bất kỳ | **(a)** cho tính năng clone | `CloneRequest.audio_url` (`liva_api.py:21-26`) mô tả "Audio URL (YouTube, direct link)" |
| `liva-voice/src/speaker_verifier.py:80-82,104-106` — `SpeakerEncoder.from_hparams(source="speechbrain/spkrec-ecapa-voxceleb", …)` | **HuggingFace Hub** | **(a)** lần đầu | Tải weights ECAPA-TDNN. Có cache local sau lần đầu |
| `liva-voice/src/voice_pipeline.py:306-317` — `WhisperModel("large-v3-turbo" \| "small", …)` | **HuggingFace Hub** (`Systran/faster-whisper-*`) | **(a)** lần đầu | ctranslate2 tự tải model |
| `liva-voice/requirements.txt:10,17,20` — `edge-tts`, `yt-dlp`, `faster-whisper` | — | — | Khai báo dependency mạng ngay trong requirements |
| `liva-voice/test_integration.py:27` — `http://127.0.0.1:8002/tts` | loopback | **(c)** | Test; endpoint `/tts` không tồn tại trên WS gateway 8002 |
| `liva-voice/src/gpt_sovits_core.py:104,587` — `print("Download from: https://github.com/Soulghost/GPT-SoVITS")` | — | (c) | Chỉ là chuỗi in ra |

**Mức độ nguy hiểm thực tế = thấp**, vì `scripts/start_all.ps1:24` chỉ giải phóng port `8101, 8100, 8002, 8082, 5173, 8000` — **không có 8765** — và không dòng nào chạy `liva_api.py`. Grep `8765` toàn repo: chỉ `CLAUDE.md:50`, `README.md:99`, `liva_api.py:381,396`. Không file `.rs`/`.ts`/`.vue` nào chạm tới. Đây là **code opt-in chạy tay, không nối dây**.

#### 3.2.4 Build-time / dev-time (không phải runtime)

| Nơi | Đích | Loại | Ảnh hưởng |
|---|---|---|---|
| `liva-native-core/Cargo.toml:30` — `ort = { version = "2.0.0-rc.9", features = ["ndarray"] }` (**không** `default-features = false`) → `ort-2.0.0-rc.11/Cargo.toml:79-86` `default = ["std","ndarray","tracing","download-binaries","tls-native","copy-dylibs"]` | `https://cdn.pyke.io/0/pyke:ort-rs/ms@1.23.2/x86_64-pc-windows-msvc.tar.lzma2` (`ort-sys-2.0.0-rc.11/build/download/dist.txt:6`) | **(a)** build-time | **Lần build đầu BẮT BUỘC có mạng.** ONNX Runtime DLL được tải từ CDN của pyke, verify SHA256, cache trong `~/.cargo`. Đây là điểm phụ thuộc mạng **cứng nhất** của toàn dự án — mọi thứ STT/TTS/VAD/wakeword đều đứng trên `ort` |
| `Cargo.lock` / toàn bộ crates | `crates.io` | (a) build-time | Bình thường |
| `scripts/ai-pre-commit.cjs:47-48,55,63,169,189` — `AI_BASE_URL` mặc định `http://127.0.0.1:8000/v1`, `Authorization: Bearer ${apiKey}` | mặc định **localhost:8000**, nhưng user **có thể trỏ ra cloud** qua `.env` | **(b)** dev-time | Hook pre-commit gửi diff code lên endpoint LLM. Mặc định local; `.env.example:161-164` **gợi ý** `AI_BASE_URL=https://generativelanguage.googleapis.com/v1beta/openai`. Bypass: `SKIP_AI_HOOK=1`. **Chỉ chạy lúc commit, không nằm trong sản phẩm** |
| Model weights `*.gguf`, `*.onnx`, `models/nemotron-asr` (nested git repo + LFS) | out-of-band | (a) setup | Không có script auto-download trong repo (`scripts/` chỉ có `ai-pre-commit.cjs`, `generate_hey_liva_model.py`, `start_all.ps1`, `legacy/`) |

> 📌 Danh sách model, dung lượng RAM/VRAM và điều kiện tiên quyết build đầy đủ: [Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md)

#### 3.2.5 Ngoài phạm vi sản phẩm

| Nơi | Ghi chú |
|---|---|
| `teamwork_projects/obsidian_llm_wiki/src/` | grep `http://\|https://\|fetch(\|axios` = **0 hit**. MCP server local stdio, không mạng |
| `mobile_client/` (Capacitor Android) | `src/App.vue:70` `wsUrl = ref('ws://127.0.0.1:8002/ws')`, người dùng tự nhập IP LAN. `WebSocketClient.ts:60` `new WebSocket(this.url)`. **LAN, không Internet** |
| `liva-computer-use/` | **Thư mục rỗng** |

### 3.3 Hàng rào kỹ thuật thực sự có (điểm cộng, kiểm chứng được)

1. **CSP của Tauri** — `liva-desktop/src-tauri/tauri.conf.json:45`:
   ```
   default-src 'self'; connect-src 'self' ipc: http://localhost:5173 ws://localhost:5173 ws://localhost:8002 ws://127.0.0.1:8002;
   script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' asset: data:; font-src 'self';
   ```
   → **Trong bản đóng gói, WebView không thể fetch/WS ra bất kỳ host ngoài nào.** `font-src 'self'` chặn cả Google Fonts. Đây là ràng buộc **cưỡng chế**, không phải quy ước.
2. **Không có plugin HTTP/updater** cho Tauri (`Cargo.toml` + `capabilities/default.json`) → không có kênh phone-home.
3. **MediaPipe wasm + model đã vendor local** (`public/assets/wasm/`, `public/assets/models/face_landmarker.task`) — mặc định thư viện này tải wasm từ jsDelivr; ở đây đã tránh.
4. **WS server bind `127.0.0.1` mặc định** (`main.rs:453`).
5. **ESLint cấm `fetch` trần**, buộc qua `safeFetch` (`liva-ui/src/utils/fetch.ts:16`) — dễ audit, chỉ 3 call site.
6. **Telegram allow-list fail-closed** — `telegram.rs:73-78`: `if self.allowed_ids.is_empty() { return false; }`. Không set `TELEGRAM_ALLOWED_IDS` thì bot từ chối **tất cả**, kể cả chính chủ.

### 3.4 Hàng rào KHÔNG có (rủi ro còn lại)

- **Không có kill-switch mạng ở tầng Rust.** `teloxide`/`reqwest` được compile vào **mọi** build. Chỉ cần biến môi trường `TELEGRAM_BOT_TOKEN` là một kết nối long-polling thường trực ra `api.telegram.org` được mở (`main.rs:320-341`) — **không có UI nào cảnh báo**, không có feature flag lúc build.
- **Telegram `/cat` không sandbox path.** `telegram.rs:223` gọi thẳng `tokio::fs::read_to_string(file_path.trim())` — **không chặn `..`, không giới hạn thư mục gốc**, trái ngược với `mcp/server.rs:67-77` `resolve_path` vốn có chống traversal. Ai lọt vào `TELEGRAM_ALLOWED_IDS` đọc được `.env`, vault, khoá mã hoá — **qua Internet**. `/ls <path>` (`telegram.rs:175-217`) tương tự. Allow-list fail-closed làm giảm rủi ro mặc định, nhưng không giảm rủi ro khi đã cấu hình.
- ~~**`webrtc/signaling.rs:24` bind `0.0.0.0`**~~ — **đã hết rủi ro này**: file bị xoá ở `510c9e2` (22/07/2026) cùng crate `webrtc = "0.12.0"` chưa dùng.
- **Build phụ thuộc CDN bên thứ ba** (`cdn.pyke.io`) **không được ghi** trong `CLAUDE.md`/`README.md` như một prerequisite.
- **Vendor lock ngầm ở `liva-voice`**: nếu ai đó chạy `python liva_api.py` (như README/CLAUDE.md hướng dẫn), WS `/ws` mặc định `current_voice = "vi-VN-HoaiMyNeural"` (`liva_api.py:301`) → **mọi câu TTS đi qua Microsoft**. Service bind `0.0.0.0:8765` (`liva_api.py:381`), **không auth, không CORS, không rate-limit**, `/docs` Swagger phơi ra.

> 📌 Các rủi ro trên đã được xếp hạng mức độ (kèm rủi ro ngoài phạm vi offline) ở: [Nợ kỹ thuật và rủi ro](02-no-ky-thuat-va-rui-ro.md)

### 3.5 Điểm phản biện quan trọng: "chế độ cloud" trong UI là ẢO

`.env.example:159-164` và `AISettings.vue` / `ApiManagementView.vue` cho phép chọn `provider: "cloud"`, nhập `AI_BASE_URL`, `AI_API_KEY`, `AI_MODEL`. Nhưng:

- **Rust core không đọc `AI_PROVIDER`/`AI_BASE_URL`/`AI_API_KEY`** — grep trên `liva-native-core/src/` = 0 hit.
- Không có HTTP client cho LLM ở đâu cả (chỉ `reqwest::get` duy nhất trong `telegram.rs`).
- Hệ quả cụ thể, `lib.rs:119-128`:

  ```rust
  pub fn configured_router_model_path() -> Option<std::path::PathBuf> {
      let provider = ai.get("provider").and_then(|v| v.as_str()).unwrap_or("local");
      if provider != "local" { return None; }   // ← lib.rs:126-128
      ...
  }
  ```

  → Chuyển sang `"cloud"` **không** khiến LIVA gọi cloud; nó khiến **LLM không nạp model nào cả** (engine = `None`), tức chatbot **chết câm**.
- `ApiManagementView.vue` ghi/đọc `.env` qua op `get_env_config`/`save_env_config` — **cả hai không tồn tại trong `handle_command`** (`lib.rs:1483` `_ => Err(format!("Unknown command: {}", command))`) → form còn không lưu được.
- Tương tự, `REMOTE_CONTROL_ENABLED`, `ZALO_*`, `EMAIL_HOST/PORT/USER/PASS` trong `.env.example:166-186`: grep trên `liva-native-core/src` + `liva-desktop/src-tauri/src` = **0 hit**. Toàn bộ là **khai báo chết**, di sản của stack Node đã xoá.
- `geolocationEnabled: true` trong `data/liva-config.json:37`: grep `geolocation` trong Rust → **chỉ 1 hit ở `lib.rs:390`** (literal trong JSON fallback của `get_config`). **Không có code IP-lookup nào.**

### 3.6 Rút mạng thì sao? — trả lời thẳng

*Giả định: đã build xong, đã có model trên đĩa, không đặt `TELEGRAM_BOT_TOKEN`, không chạy `liva-voice`.*

**VẪN CHẠY 100% (kiểm chứng bằng code)**

| Chức năng | Vì sao offline |
|---|---|
| LLM chat + vision | `llama-cpp-2` với GGUF local từ `ai.localModelsDir` (`data/liva-config.json:15-21`, `E:\AI_Models\Qwen3-VL-2B-Instruct-GGUF\…`). llama.cpp compile từ source vendored |
| STT (Nemotron / Parakeet), VAD Silero, wakeword, GTCRN denoise | `ort` + file `.onnx` local trong `models/` |
| TTS (Piper / Kokoro / VieNeu) | `.onnx` local + `espeak-ng` binary trên PATH |
| Vision `vision:ask` | `xcap` chụp màn hình + mmproj GGUF local (`lib.rs:1394-1445`) |
| DB / memory / crypto | `rusqlite` bundled, AES-GCM local |
| Agent graph, smart_home (stub), tasks, MCP server nội bộ | Toàn bộ in-process |
| Avatar 3D/2D, face tracking | three.js/pixi bundle + wasm + model local |
| Dashboard, Widget, WS gateway | loopback |

**HỎNG / KHÔNG HOẠT ĐỘNG**

| Thứ | Mức độ | Ghi chú |
|---|---|---|
| Telegram bot (nếu đã bật) | Hỏng hoàn toàn | `teloxide` polling fail liên tục; `/ask`, `/cat`, voice message đều chết |
| `telegram:send_text` IPC | Trả lỗi | Hiện không ai gọi |
| Nút "Tải model VRoid/Mixamo" (`AvatarGallery.vue:176,181`) | Mở trình duyệt lỗi | Cosmetic |
| Toàn bộ `liva-voice/` (nếu chạy tay) | `/tts`, `/ws`, `/clone` chết | edge-tts + yt-dlp + HF |
| `safeFetch` → `127.0.0.1:3000` | Vốn đã hỏng sẵn | Không phải do mất mạng |
| **Build lại từ đầu** | **KHÔNG BUILD ĐƯỢC** | `ort-sys` cần `cdn.pyke.io`; crates.io |

**KHÔNG HỎNG NHƯNG CŨNG KHÔNG BAO GIỜ HOẠT ĐỘNG (dù có mạng)**

`ApiManagementView` (`get_env_config`/`save_env_config` không có handler), chế độ `provider: "cloud"` (chỉ làm LLM tắt), `geolocationEnabled`, `ZALO_*`, `EMAIL_*`, `REMOTE_CONTROL_ENABLED`, digest delivery Telegram/Zalo/Email trong `SettingsView.vue` (chỉ ghi vào JSON config, không có consumer Rust).

### 3.7 Kết luận thẳng thắn: LIVA offline tới mức nào?

**LIVA offline ở mức rất cao trên đường chạy sản phẩm — nhưng đó là offline "de facto", không phải offline "by design".**

Cụ thể:

- **Về runtime của bản desktop: gần như tuyệt đối.** Không HTTP client nào được gọi, CSP cưỡng chế WebView chỉ nói chuyện loopback, không updater, không telemetry, MediaPipe đã vendor. Đây là điều **kiểm chứng được bằng grep**, không phải lời hứa.
- **Về chủ đích thiết kế: yếu, nhưng đã có tiền lệ kỹ thuật.** LIVA vẫn không có kill-switch mạng, vẫn **không** feature-gate `teloxide`/`reqwest`, và không có kiểm tra runtime nào chặn kết nối ra ngoài. Nó offline **vì đường cloud chưa bao giờ được viết**, chứ không phải vì có ai chặn nó. Một PR duy nhất nối `AI_BASE_URL` vào LLM là tuyên bố đổ. Điểm mới ngày 22/07/2026: `4c08f18` đã dựng sẵn cơ chế `[features]` + `#[cfg(feature = …)]` cho 3 module `experimental` (`Cargo.toml:64-78`), tức **kỹ thuật để feature-gate `mod telegram` nay đã có mẫu trong chính repo** — việc còn lại là quyết định làm hay không.
- **Về setup: không offline.** Build đầu tiên bắt buộc có mạng (`cdn.pyke.io` cho ONNX Runtime, crates.io cho crates), và toàn bộ model weights phải tải out-of-band. "100% offline" đúng với **người dùng cuối đã cài xong**, sai với **người build từ source**.
- **Về các module phụ: có vi phạm rõ.** `liva-voice/` gửi nguyên văn text người dùng lên Microsoft Edge TTS. Module này không được app khởi động và không nằm trên đường thoại realtime — nhưng nó **có trong repo** và **được README/CLAUDE.md hướng dẫn chạy**, nên không thể lờ đi khi phát biểu công khai.

Ba việc biến tuyên bố thành sự thật **cưỡng chế**:

1. Đưa `mod telegram` + `teloxide`/`reqwest` vào `#[cfg(feature = "remote")]`, mặc định tắt → binary offline **không chứa** HTTP client nào. *(Mẫu làm sẵn: `experimental = []` ở `Cargo.toml:75` gate 3 module qua `#[cfg]`, làm từ 22/07/2026.)*
2. `ort` đặt `default-features = false` + link ONNX Runtime đã vendor, để build offline được — hoặc ít nhất ghi rõ prerequisite mạng trong `CLAUDE.md`/`README.md`.
3. Xoá `liva-native-core/static/` (chứa STUN Google), ~~`webrtc/signaling.rs`~~, 2 call `safeFetch("http://127.0.0.1:3000/api/sensory-capture")`, và các key cloud chết trong `.env.example`. **Đã xong một phần ngày 22/07/2026:** `webrtc/signaling.rs` và crate `webrtc` đã bị gỡ (`510c9e2`); `static/` **vẫn còn** (`static/app.js:30` vẫn giữ `stun:stun.l.google.com:19302`).

Bổ sung về bảo mật: sandbox `/cat` và `/ls` của Telegram bằng chính `resolve_path` của `mcp/server.rs:67-77`.

> 📌 Ba việc trên đã được xếp vào lộ trình 5 giai đoạn kèm hướng dẫn sửa F1–F5: [Lộ trình sửa lỗi và nâng cấp](03-lo-trinh-sua-loi-va-nang-cap.md)

---

## 4. Claim đúng và nên giữ nguyên

Danh sách này quan trọng ngang danh sách sai — đây là những gì LIVA có thể tự tin nói mà không sợ bị bắt bẻ:

- Piper song ngữ tự chọn giọng theo dấu tiếng Việt (`tts/mod.rs:101-105,194-254`)
- Nemotron RNN-T ONNX thật, 3 session encoder/decoder/joint (`stt/engine.rs:5-7,25-60`)
- Silero VAD 22 frame ≈ 0,704 s — **con số trong README khớp code từng chữ số** (`webrtc/vad.rs:33-51`)
- Governor game-mode Win32 thật, nối dây **cả hai** entry (`governor.rs:200-237,269-338`; `main.rs:141-151`, Tauri `lib.rs:468-473`). Từ 22/07/2026 có thêm nhánh đọc tải CPU thật có trừ phần của chính LIVA (`governor.rs:103-121,127-173`) — **nhưng chưa đọc tải GPU**, nên đừng phát biểu là "tự nhường GPU theo tải"
- Ghost Mode click-through end-to-end (`tauri.conf.json`, `lib.rs:76-78,534,547`, `TauriAdapter.ts:11-17`)
- CSP cưỡng chế + Argon2id KDF + SQLite WAL
- Screen vision + region diff pure-Rust (`lib.rs:249-343`, `vision/capture.rs:160-244`)
- 4 binary verify tồn tại thật trong `src/bin/`
- Rust binary đơn + Tokio, không GC (`main.rs:30-49`)
- Không CDN trong asset frontend; MediaPipe wasm đã vendor local

---

## 5. Việc README đang **dưới-báo cáo**

Một nghịch lý đáng chú ý: cùng lúc README nói quá về 5-6 tính năng, nó lại **hoàn toàn không nhắc** một nhóm tính năng đã có code thật:

| Tính năng có code nhưng README im lặng | Bằng chứng | Trạng thái |
|---|---|---|
| VieNeu-TTS (giọng Việt neural, port thuần Rust) | `tts/mod.rs:156-189` (`LIVA_TTS_VIENEU=1`) | **[MỘT PHẦN]** opt-in |
| GTCRN denoise | `main.rs:181-209` | **[OK]** — **bật mặc định** trong standalone |
| Smart Turn (turn-shadow) | `main.rs:214-230` (`=1`, log-only) | **[MỘT PHẦN]** opt-in, mới log |
| AEC (khử vọng) | `main.rs:234-238` (`=1`) | **[MỘT PHẦN]** opt-in |
| Parakeet-vi (STT tiếng Việt offline) | `stt/mod.rs:49-51,108-136`, lazy-load 2,4 GB (`LIVA_STT_VI_ENGINE=parakeet`) | **[MỘT PHẦN]** opt-in |
| Qwen3-VL `vision:ask` (hỏi đáp trên ảnh màn hình) | `lib.rs:1394-1445` | **[OK]** |
| Governor nhận diện "máy đang bận" bằng **tải CPU thật**, có trừ CPU của chính LIVA | `governor.rs:103-121,127-173,213-222`; ngưỡng `LIVA_BUSY_CPU_PERCENT` mặc định 80 (`governor.rs:79`) | **[OK]** — bật mặc định ở cả hai entry (chỉ chi phối process priority; **không** đọc GPU) |

Với hồ sơ dự thi, đây là phần **nên được kể**, vì nó là công sức thật và kiểm chứng được — trong khi các claim ở §2 thì không.

---

## 6. Câu chữ thay thế cho README

Các đoạn dưới đây đã được viết sẵn để **dán thẳng** vào `README.md`, thay cho các mục tương ứng. Mọi số liệu và tên file trong đó đều có nguồn trong tài liệu này.

### 6.1 Thay cho mục "Zero-Latency Native Engine" (README:23)

> **⚡ Lõi native Rust, không GC.** Toàn bộ backend là một binary Rust duy nhất (`liva-native-core`) chạy trên Tokio async runtime — không garbage collector, không interpreter, không event-loop stall. Suy luận LLM đi qua `llama.cpp` được nhúng in-process (`llama-cpp-2`), nạp model GGUF bằng `mmap`. Embedding ngữ nghĩa cũng được tính in-process qua chính engine đó, không cần embedding service ngoài hay worker runtime riêng.
>
> *Ghi chú kỹ thuật:* hiện chat và embedding **dùng chung một `LlamaContext`**, nên hai thao tác này tuần tự chứ chưa song song. Tách context là hạng mục đã lên kế hoạch.

*(Bỏ hẳn "TTFT < 100 ms" cho tới khi có benchmark. Nếu muốn giữ một con số, chỉ nên dùng số đã đo thật: `verify_duplex` assert phát hiện VAD < 15 ms và preemption < 10 ms.)*

### 6.2 Thay cho mục "Sequential Hot-Swap Model" (README:24)

> **🔄 Hot-Swap model tuần tự.** LIVA có thể tráo model GGUF ngay trong VRAM qua lệnh `llm:swap_model`: engine cũ được giải phóng, chờ driver thu hồi, rồi nạp model mới bằng `mmap` để rút ngắn thời gian load. Hiện việc chuyển giữa router model và expert model được kích hoạt **thủ công**; cơ chế tự động định tuyến theo độ khó câu hỏi đang trong lộ trình.
>
> Cấu hình model đọc từ `data/liva-config.json` (`ai.localModelsDir` + `ai.routerModel`). Router mặc định hiện tại là **Qwen3-VL-2B-Instruct (Q4_K_M)** — một model đa phương thức, đảm nhiệm cả text lẫn thị giác.

### 6.3 Thay cho mục "100% Local Voice Stack" (README:25)

> **🎙️ Ngăn xếp giọng nói chạy hoàn toàn cục bộ.** ASR **Nemotron** (ONNX, RNN-T) với khả năng đổi ngôn ngữ lúc chạy qua `voice:set_language` (hiện đã kiểm chứng cho `vi-VN` và `en-US`). TTS **Piper VITS** với giọng riêng theo ngôn ngữ (`vi_VN-vais1000` + `en_US-lessac`), tự chọn giọng theo dấu tiếng Việt. **Silero VAD** cấu hình 22 frame × 32 ms ≈ **0,7 s** end-of-turn cho cảm giác đối thoại nhanh. Bổ sung: khử nhiễu **GTCRN**, tuỳ chọn **AEC**, tuỳ chọn STT tiếng Việt offline **Parakeet**, và tuỳ chọn TTS neural tiếng Việt **VieNeu**.
>
> Streaming song công (full-duplex) với ngắt lời (barge-in) được cài đặt trong WebSocket server nhúng (`ws://localhost:8002`) của `liva-native-core`. **Lưu ý ở bản hiện tại:** server này chỉ hoạt động khi chạy binary `liva-native-core` độc lập; bản desktop Tauri đi qua IPC và chưa bật đường VAD/barge-in. Hợp đồng khung âm thanh giữa UI và core đang được đồng bộ lại.

### 6.4 Thay cho mục "Absolute Data Privacy & Security" (README:29)

> **🔒 Riêng tư theo mặc định.** Toàn bộ suy luận AI của LIVA — LLM, thị giác, nhận dạng giọng nói, tổng hợp giọng, VAD, wake word — chạy **hoàn toàn cục bộ** bằng `llama.cpp` + ONNX Runtime trên model lưu trên đĩa. Lõi Rust **không chứa client AI đám mây nào**. WebView bị Content Security Policy khoá, chỉ được kết nối loopback (`font-src 'self'` chặn cả Google Fonts). Không có plugin auto-update, không có telemetry. Thư viện nhận diện khuôn mặt MediaPipe đã được vendor sẵn wasm + model, không tải từ CDN. **Rút mạng, LIVA vẫn hoạt động đầy đủ.**
>
> Bảo mật dữ liệu: **Argon2id** cho két Stronghold trên desktop, **AES-256-GCM** cho bảng ghi nhớ `facts`, SQLite chạy chế độ **WAL**.
>
> **Ngoại lệ cần nói rõ:**
> 1. Tích hợp **Telegram** là tuỳ chọn và theo bản chất cần Internet — **mặc định TẮT**, chỉ bật khi người dùng tự đặt `TELEGRAM_BOT_TOKEN`, và **không được bật trong bản desktop**.
> 2. Thư mục `liva-voice/` là **sandbox thí nghiệm nhân bản giọng**, không thuộc đường thoại realtime, không được ứng dụng khởi động, và **có dùng dịch vụ đám mây** (Edge TTS / YouTube / HuggingFace). Chưa dùng cho sản phẩm.
> 3. **Lần build đầu tiên cần Internet** để tải thư viện ONNX Runtime (`cdn.pyke.io`) và trọng số model — sau đó chạy hẳn offline.

### 6.5 Thay cho mục "Atomic Memory & Hybrid Search" (README:30) và "Memory Dashboard" (README:34)

> **♻️ Nền tảng bộ nhớ.** SQLite chạy chế độ WAL (`journal_mode=WAL`, `wal_autocheckpoint=500`, `busy_timeout=5000`) để chịu được ngắt tiến trình đột ngột. Tầng truy hồi lai đã có: chỉ mục vector `sqlite-vec` (`vec_idx`, 384 chiều int8) đặt cạnh chỉ mục toàn văn FTS5, hợp nhất kết quả qua `memory:search_hybrid`.
>
> **Trạng thái hiện tại:** schema bộ nhớ phân tầng (L0→L3), hàng đợi consolidation và dead-letter queue đã được thiết kế đầy đủ trong `db.rs`, nhưng **đường ghi dữ liệu chưa được nối** — `chat:completion` hiện chưa tự động lưu ký ức. Memory Dashboard đọc trực tiếp các bảng này nên sẽ hiển thị rỗng cho tới khi pipeline ghi được bật. Đây là hạng mục ưu tiên của lộ trình kế tiếp.

*(Bỏ cụm "survives SIGKILL with zero data corruption" cho tới khi có test crash thật; bỏ "Reflection Daemon" và "Nightly Cron" khỏi phần mô tả tính năng, chuyển sang mục Lộ trình.)*

### 6.6 Thay cho mục "Self-Correction Loop" (README:32)

> **🤖 Sandbox tự sửa lỗi (khung sườn).** LIVA có sẵn một vòng lặp tự sửa: chạy `cargo test` trong sandbox, trích lỗi từ log, áp patch, khôi phục file gốc bằng `BackupGuard` nếu thất bại, retry tối đa 3 lần. Vòng lặp này đã hoàn chỉnh và có test.
>
> **Trạng thái:** phần sinh patch được trừu tượng hoá qua `trait CodeAgent`; adapter nối trait này vào LLM cục bộ **chưa được viết**, nên hiện chỉ có implementation giả dùng trong test. Đây là viên gạch đầu tiên, chưa phải tính năng dùng được.

### 6.7 Thay cho các mục tích hợp (README:113, :120, :121, :122)

> **🔌 Tích hợp.**
> - **Telegram Remote-Control** — có thật, chạy khi khởi động `liva-native-core` độc lập với `TELEGRAM_BOT_TOKEN` + `TELEGRAM_ALLOWED_IDS`. Allow-list **fail-closed**: không cấu hình ID thì bot từ chối mọi người. Chưa khả dụng trong bản desktop.
> - **Obsidian Knowledge Vault (MCP)** — bản MCP server đang dùng được là **TypeScript** (`teamwork_projects/obsidian_llm_wiki/`). Bản MCP nhúng trong lõi Rust đã khai báo tool nhưng chưa được nối vào bộ điều phối lệnh.
> - **Smart Home / Email (IMAP) / Zalo OA** — *chưa cài đặt*. `integration:smart_home_control` hiện là stub trả về thành công mà không điều khiển thiết bị nào; các khoá `EMAIL_*`, `ZALO_*` trong `.env.example` là khai báo còn sót lại, không có code đọc.

*(Nguyên tắc chung cho README mới: mọi mục chưa nối dây phải nằm dưới tiêu đề **Lộ trình**, không nằm trong **Tính năng**. Đó là khác biệt giữa "tham vọng có tài liệu" và "quảng cáo sai".)*

---

## Liên quan

**Đọc tiếp theo mạch:** [⬆ Mục lục](../README.md) · [◀ Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) · [Nợ kỹ thuật và rủi ro ▶](02-no-ky-thuat-va-rui-ro.md)

**Tài liệu này dựa vào (nguồn sự thật ở nơi khác):**

- [Kiến trúc tổng thể](../01-ban-ve/01-kien-truc-tong-the.md) — bảng so sánh hai profile chạy và sơ đồ kiến trúc, tiền đề của §0
- [Giao thức IPC và WebSocket](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) — đặc tả khung nhị phân 9 byte và bảng opcode, dùng để chứng minh lỗi hợp đồng khung mic ở §2.3
- [Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) — ngưỡng VAD/AEC/denoise, backend TTS, engine STT đứng sau các dòng phán quyết thoại ở §1
- [Hệ LLM và prompt](../01-ban-ve/04-he-llm-va-prompt.md) — cấu hình LLM/router, làm nền cho §2.1
- [Agent, bộ nhớ và tiến hoá](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) — máy trạng thái agent và StateGraph, nền cho §2.2
- [Tầng dữ liệu và bảo mật](../01-ban-ve/07-tang-du-lieu-va-bao-mat.md) — ERD SQLite và sơ đồ mã hoá, nền cho các dòng WAL/AES/Argon2id
- [Tích hợp ngoài](../01-ban-ve/09-tich-hop-ngoai.md) — bảng tích hợp Telegram/MCP/smart home
- [Cấu hình và biến môi trường](../02-van-hanh/01-cau-hinh-va-bien-moi-truong.md) — bảng biến `LIVA_*`/`AI_*` và lệch `.env.example` vs code
- [Mô hình AI và tài nguyên](../02-van-hanh/02-mo-hinh-ai-va-tai-nguyen.md) — bảng model và điều kiện tiên quyết build (`cdn.pyke.io`)
- [Triển khai và runtime](../02-van-hanh/03-trien-khai-va-runtime.md) — bảng tiến trình và cách chạy đúng từng profile
- [Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) — bảng test và binary verify, nguồn cho dòng "Deep Verification Suite"
- [Báo cáo khảo sát gốc 2026-07](00-bao-cao-khao-sat-goc-2026-07.md) — dữ liệu khảo sát thô ban đầu (đã được đính chính ở tài liệu này)

**Tài liệu khác dựa vào tài liệu này:**

- [Nợ kỹ thuật và rủi ro](02-no-ky-thuat-va-rui-ro.md) — lấy danh sách claim **[THIẾU]**/**[MỘT PHẦN]** và các vết nứt offline làm đầu vào xếp hạng rủi ro
- [Lộ trình sửa lỗi và nâng cấp](03-lo-trinh-sua-loi-va-nang-cap.md) — lấy §2.1/§2.2/§2.3 làm gốc cho các hạng mục sửa F1–F5
- [Prompt sinh README](../04-quy-trinh/prompts/readme-generation-prompt.md) — dùng §6 làm câu chữ thay thế cho `README.md`
- [Knowledge Base](../04-quy-trinh/KNOWLEDGE_BASE.md) — trích nguyên tắc "tính năng chưa nối dây phải nằm dưới Lộ trình"

**Khi sửa code sau đây thì phải cập nhật tài liệu này:**

- `liva-native-core/src/main.rs` — §0 (profile standalone), các dòng VAD/wake/Telegram/governor trong §1
- `liva-native-core/src/governor.rs` — dòng governor và dòng "sống chung với workload nặng" trong §1, mục governor ở §4 và §5
- `liva-native-core/Cargo.toml` (mục `[features]`) — mọi phán quyết có kèm điều kiện `--features experimental`: số test ở dòng "Deep Verification Suite", `evolution/`, `agent/dispatcher.rs`, `passive/` trong §1
- `liva-desktop/src-tauri/src/lib.rs` — §0 (profile Tauri), §1 (handle_command, Argon2id, AES, vision, swap model), §3.5
- `liva-native-core/src/llm/embed.rs` + `src/llm/*` (engine, prompt) — §2.1 toàn bộ
- `liva-native-core/src/webrtc/frame.rs` + `src/webrtc/pipeline.rs` — §2.3 (hợp đồng khung 9 byte, preemption)
- `liva-ui/src/composables/useVoicePipeline.ts` + `useGateway.ts` — §2.3 (header 1 byte), §1 (đường truyền WS vs IPC)
- `liva-native-core/src/db.rs` + `src/agent/memory.rs` — §2.2 (schema L0→L3, checkpoint hỏng ngữ nghĩa)
- `liva-native-core/src/telegram.rs` + `src/integrations/smart_home.rs` + `src/mcp/*` — §1 và §3.2.1, §3.4 (điểm ra mạng, `/cat` không sandbox)
- `data/liva-config.json`, `.env.example`, `liva-desktop/src-tauri/tauri.conf.json` — §1 (router model, env chết), §3.3 (CSP), §3.5
- `scripts/start_all.ps1` — §0 (script không khởi động `liva-native-core`) và §2.3 tầng 1
- `liva-voice/liva_api.py` + `liva-voice/src/*` — §3.2.3 (Edge TTS / yt-dlp / HuggingFace)
