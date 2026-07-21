---
title: "Mô hình AI và tài nguyên"
updated: 2026-07-21
commit: bf17341
status: living
owns:
  - bang-model
  - bang-tai-nguyen-ram-vram
  - dieu-kien-tien-quyet-build
covers:
  - Cargo.toml
  - package.json
  - .github/workflows/test.yml
  - data/liva-config.json
  - liva-desktop/src-tauri/Cargo.toml
  - liva-desktop/src-tauri/src/lib.rs
  - liva-native-core/Cargo.toml
  - liva-native-core/src/telegram.rs
  - liva-native-core/src/wake_model.rs
  - liva-native-core/src/bin/router_stress.rs
  - liva-native-core/src/llm/engine.rs
  - liva-native-core/src/stt/parakeet.rs
  - liva-native-core/src/tts/*
  - liva-native-core/src/tts/vieneu/mod.rs
  - liva-native-core/tests/integration_tests.rs
  - scripts/ai-pre-commit.cjs
---
# Mô hình AI và tài nguyên

[⬆ Mục lục](../README.md) · [◀ Cấu hình và biến môi trường](01-cau-hinh-va-bien-moi-truong.md) · [Triển khai và runtime ▶](03-trien-khai-va-runtime.md)

---

Tài liệu này trả lời bốn câu hỏi vận hành:

1. LIVA cần những **file model** nào, file nào **thật sự có trên đĩa**, file nào thiếu?
2. Chạy LIVA thì tốn bao nhiêu **RAM/VRAM** (ước tính), và model nào chạy trên **CPU** hay **GPU**?
3. Muốn **build** được thì máy phải có sẵn gì?
4. Ba **feature flag** `cuda` / `vulkan` / `openblas` thật sự làm gì?

> Nhắc lại quy ước nhãn: **[OK]** đang chạy thật · **[MỘT PHẦN]** có code nhưng tắt/opt-in/chưa nối dây · **[THIẾU]** chưa có/stub.

---

## 1. Nguồn sự thật của đường dẫn model

Có **hai** cơ chế xác định model, và chúng không phải một:

| Loại model | Nguồn đường dẫn thật | Ghi chú |
|---|---|---|
| **LLM GGUF** (router + mmproj + expert) | `data/liva-config.json` → `ai.localModelsDir` + `ai.routerModel` / `ai.mmprojModel` / `ai.expertModel` (`liva-desktop/src-tauri/src/lib.rs:119-138`, `:143`) | Fallback hằng số `DEFAULT_MODELS_DIR = "E:\\AI_Models"`, `DEFAULT_ROUTER_MODEL = "gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf"` (`lib.rs:59-61`) |
| **ONNX / Piper / VieNeu / VAD / wake** | biến `LIVA_*` → nếu không có thì default hardcode trong code, phần lớn đi qua `resolve_resource_path` | Tên biến và giá trị mặc định: xem tài liệu cấu hình |

> 📌 Nguồn đầy đủ (bảng ~60 biến `LIVA_*`, giá trị mặc định, các chỗ lệch `.env.example` vs code): [Cấu hình và biến môi trường](01-cau-hinh-va-bien-moi-truong.md)

**Cảnh báo quan trọng:** biến `LIVA_LLM_MODEL_DIR` mà `.env.example` quảng cáo **không hề được đọc** ở đường chạy thật — nó chỉ xuất hiện trong `src/bin/router_stress.rs:68` và `tests/integration_tests.rs:213`. Muốn đổi model LLM thì phải sửa `data/liva-config.json`, không phải `.env`.

Và vì **không có crate `dotenv`/`dotenvy` nào trong `Cargo.lock`** (grep → 0 kết quả) còn `E:\Project\LIVA\.env` **không tồn tại**, mọi `std::env::var("LIVA_*")` đều fail ⇒ **toàn bộ đường dẫn ONNX đang chạy bằng default hardcode**.

---

## 2. Bảng model trong repo — `E:\Project\LIVA\models\`

Tất cả weights (`*.onnx`, `*.onnx.data`, `*.gguf`, `*.bin`, `*.wav`) đều bị gitignore (`.gitignore:31-37`, `:142-150`), phải fetch out-of-band. Các JSON nhỏ (config/tokenizer/voices, `piper/*.onnx.json`) **cố ý giữ tracked** để thư mục model tự tài liệu hoá (`.gitignore:147-148`).

| File | Có? | Kích thước | Dùng cho gì | Nguồn tải |
|---|---|---|---|---|
| `nemotron-asr/encoder.onnx` + `.data` | ✅ | 2,68 MB + **690 MB** | STT chính — RNN-T encoder **[OK]** | nested git repo LFS (`models/README.md:10`) |
| `nemotron-asr/decoder.onnx` + `.data` | ✅ | 4,7 KB + 59,8 MB | STT decoder **[OK]** | ↑ |
| `nemotron-asr/joint.onnx` + `.data` | ✅ | 2,1 KB + 37,8 MB | STT joint **[OK]** | ↑ |
| `nemotron-asr/tokenizer.json`, `vocab.txt` | ✅ | 695 KB / 77 KB | BPE cho STT **[OK]** | ↑ |
| `nemotron-asr/silero_vad.onnx` | ✅ | 2,24 MB | VAD fallback (v5) **[MỘT PHẦN]** | ↑ |
| `parakeet_vi.onnx` | ✅ | **41,9 MB** (`models/README.md:11` ghi "1.1MB graph" — **sai số liệu**) | STT tiếng Việt chất lượng cao, FastConformer-CTC 0.6B **[MỘT PHẦN]** (opt-in `LIVA_STT_VI_ENGINE=parakeet`) | export NeMo từ `nvidia/parakeet-ctc-0.6b-Vietnamese` |
| `parakeet_vi.onnx.data` | ✅ | **2.435.002.372 B ≈ 2,27 GiB** | weights external của Parakeet | ↑ |
| `parakeet_vi_vocab.json` | ✅ | 10 KB | 1024 BPE token | ↑ |
| `silero_vad_v6.onnx` | ✅ | 2,33 MB | VAD chính (được ưu tiên) **[MỘT PHẦN]** — chỉ nạp ở bin standalone | snakers4/silero-vad |
| `gtcrn_simple.onnx` | ✅ | 536 KB | Khử ồn trước VAD/STT (MIT) **[MỘT PHẦN]** | sherpa-onnx release |
| `smart_turn_v3.2_cpu.onnx` | ✅ | 8,68 MB | End-of-turn shadow (BSD-2) **[MỘT PHẦN]** — cần `LIVA_TURN_SHADOW_ENABLED=1` | pipecat-ai/smart-turn-v3 |
| `wakeword_melspec.onnx` | ✅ | 1,09 MB | mel-spectrogram cho pipeline wake **[MỘT PHẦN]** | livekit/rust-sdks (Apache-2.0) |
| `wakeword_embedding.onnx` | ✅ | 1,33 MB | embedding wake **[MỘT PHẦN]** | ↑ |
| `wake_liva_en.onnx` | ✅ | 184 KB | classifier wake EN — recall 98,8% / FPPH 1,74 @0.5 **[MỘT PHẦN]** | tự train 2026-07-04 |
| `wake_liva_vi.onnx` | ✅ | 185 KB | classifier wake VI — **chất lượng kém** (FPPH 19,4 @0.5) **[MỘT PHẦN]** | tự train 2026-07-05 (VoxCPM) |
| `wake_fixtures/hey_livekit.onnx`, `positive.wav`, `negative.wav` | ✅ | 952 KB / 64 KB / 64 KB | fixture test `wake_model.rs` | livekit fixtures |
| `piper/vi_VN-vais1000-medium.onnx` (+ `.onnx.json`) | ✅ | 63.201.294 B (md5 `5e42428c…`) | TTS tiếng Việt local-first **[OK]** | rhasspy/piper-voices |
| `piper/en_US-lessac-medium.onnx` (+ `.onnx.json`) | ✅ | 63.201.294 B (md5 `2fc642b5…` — **khác file**, trùng size vì cùng kiến trúc *medium*) | TTS tiếng Anh **[OK]** | ↑ |
| **`kokoro-v1.0.onnx`** | ❌ **KHÔNG có** | — | TTS EN premium — đây là **default của `LIVA_TTS_MODEL_PATH`** **[THIẾU]** | phải tự tải |
| `node_modules/kokoro-js/voices/af_heart.bin` | ✅ | 522.240 B | voice embedding Kokoro — **đọc EAGER** lúc init (`tts/mod.rs:290`) | npm `kokoro-js` |
| `vieneu/vieneu_prefill.onnx` | ✅ | 324 KB | VieNeu-TTS prefill **[MỘT PHẦN]** (opt-in `LIVA_TTS_VIENEU=1`) | HF `pnnbao-ump/VieNeu-TTS-v3-Turbo` |
| `vieneu/vieneu_decode_step.onnx` | ✅ | 306 KB | decode autoregressive | ↑ |
| `vieneu/vieneu_acoustic_cached.onnx` | ✅ | 7,21 MB | acoustic model | ↑ |
| `vieneu/vieneu_backbone_shared.data` | ✅ | **415 MB** | weights backbone Qwen3 | ↑ |
| `vieneu/vieneu_v3_heads.npz` | ✅ | 52,2 MB | tied embedding/head (đọc bằng `ndarray-npy`, `tts/vieneu/mod.rs:132`) | ↑ |
| `vieneu/moss_audio_tokenizer_decode_full.onnx` + `_shared.data` | ✅ | 682 KB + 44,2 MB | codec MOSS 48 kHz | OpenMOSS `MOSS-Audio-Tokenizer-Nano-ONNX` |
| `vieneu/sea_g2p.bin` | ✅ | 50,1 MB | phonemizer tiếng Việt | pnnbao97/sea-g2p |
| `vieneu/tokenizer.json`, `config.json`, `voices_v3_turbo.json`, `codec_browser_onnx_meta.json` | ✅ | 22 KB / 2,1 KB / 117 KB / 17 KB | tokenizer + preset giọng | ↑ |
| `ggml-vocab-llama-bpe.gguf` / `-spm.gguf` | ✅ | 7,82 MB / 724 KB | fixture test vocab-only (`llm/engine.rs:500`) | llama.cpp |
| `asr_example.wav`, `gtcrn_test_noisy.wav` | ✅ | 64 KB / 77 KB | fixture test | — |

### 2.1 Hai cái bẫy file phải nhớ

1. **`kokoro-v1.0.onnx` thiếu nhưng KHÔNG gây chết** — Kokoro nạp **lazy** (`tts/engine.rs:24-32`), thiếu file thì bỏ qua, TTS vẫn chạy bằng Piper.
2. **`node_modules/kokoro-js/voices/af_heart.bin` thiếu thì CHẾT CẢ TTS** — file này đọc **EAGER** ngay lúc khởi tạo (`tts/mod.rs:290`); thiếu ⇒ `TtsManager = None` ⇒ mất luôn **cả Piper lẫn VieNeu**, dù hai engine đó không liên quan gì tới Kokoro. Đây là phụ thuộc ngược đời nhất trong chuỗi model.

---

## 3. LLM GGUF — `E:\AI_Models\` (ngoài repo)

Khớp `data/liva-config.json` → `ai.localModelsDir = "E:\\AI_Models"`.

| File | Có? | Kích thước | Vai trò | Trạng thái |
|---|---|---|---|---|
| `Qwen3-VL-2B-Instruct-GGUF/Qwen3-VL-2B-Instruct-Q4_K_M.gguf` | ✅ | 1,11 GB | **Router LLM đang chạy thật** (`liva-config.json` → `ai.routerModel`) | **[OK]** |
| `Qwen3-VL-2B-Instruct-GGUF/mmproj-F16.gguf` | ✅ | 819 MB | Vision projector (`ai.mmprojModel` → `configured_mmproj_path()` `lib.rs:143`) | **[MỘT PHẦN]** — chỉ chạy ở build RELEASE |
| `Qwen3-VL-2B-Instruct-GGUF/mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf` | ✅ | 445 MB | mmproj thay thế, nhẹ hơn | **[THIẾU]** — không config nào trỏ tới |
| `gemma-4-12B-it-qat-UD-Q4_K_XL.gguf` | ✅ | 6,72 GB | `ai.expertModel` | **[THIẾU]** — **chưa có code swap expert tự động** |
| `gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf` | ✅ | 4,22 GB | `DEFAULT_ROUTER_MODEL` (`lib.rs:60`) | **[MỘT PHẦN]** — chỉ dùng khi `liva-config.json` vắng |
| Còn lại: `DeepSeek-R1-Distill-Qwen-14B/32B`, `Gemma-2-9B`, `Llama-3-8B`, `Qwen2.5-*`, `Qwythos-9B`, `gemma-4-26B-A4B*`, `gemma-4-E2B/E4B` | ✅ | 3 – 19,8 GB mỗi file | kho model rời | **[THIẾU]** — không config nào trỏ tới |

### 3.1 Cấu hình chết / trỏ sai (đã kiểm chứng)

| Chỗ | Vấn đề |
|---|---|
| `data/models.config.json` | Nội dung `{"llm":{"provider":"gemma","model":"gemma-4-26B-A4B-it-UD-Q6_K.gguf"},"stt":{...},"tts":{"provider":"edge-tts"}}` — file GGUF này **không tồn tại** trên đĩa (`E:\AI_Models` chỉ có bản `-UD-Q4_K_M`), và **không có dòng Rust nào đọc file này** ⇒ cấu hình chết hoàn toàn |
| `liva-config.json` → `avatar.live2dModel` = `models/live2d/pio/index.json`, `avatar.vrmModel` = `models/vrm/…` | Thư mục `models/live2d` và `models/vrm` **không tồn tại** ở root; asset thật nằm ở `liva-ui/public/models/{live2d,vrm}` |
| `.env.example` `LIVA_LLM_MODEL_DIR=E:\AI_Models` | Không được đọc ở runtime (xem mục 1). `models/README.md:21` và `README.md:155` mô tả sai chỗ này |

> 📌 Nguồn đầy đủ của danh sách chỗ lệch giữa `.env.example` và code (dòng số, biến, hướng lệch): [Cấu hình và biến môi trường](01-cau-hinh-va-bien-moi-truong.md)

---

## 4. Ánh xạ model → module → thiết bị

```mermaid
flowchart LR
  subgraph SRC["Nguồn cấu hình"]
    CFG["data/liva-config.json<br/>ai.localModelsDir + ai.routerModel + ai.mmprojModel"]
    ENVD["default hardcode LIVA_*<br/>(vì .env KHÔNG tồn tại)"]
  end

  subgraph FILES["File trên đĩa"]
    GGUF["E:/AI_Models/Qwen3-VL-2B-Instruct-Q4_K_M.gguf · 1,11 GB"]
    MMP["E:/AI_Models/mmproj-F16.gguf · 819 MB"]
    NEMO["models/nemotron-asr/ · ~788 MB"]
    PIPER["models/piper/*.onnx · 2 x 63,2 MB"]
    KOKV["node_modules/kokoro-js/voices/af_heart.bin · 522 KB"]
    KOK["models/kokoro-v1.0.onnx · KHONG TON TAI"]
    VIE["models/vieneu/ · ~569 MB · opt-in"]
    PARA["models/parakeet_vi.onnx(+.data) · ~2,3 GiB · opt-in"]
    SVAD["models/silero_vad_v6.onnx · 2,33 MB"]
    GTC["models/gtcrn_simple.onnx · 536 KB"]
    TURN["models/smart_turn_v3.2_cpu.onnx · 8,68 MB"]
    WAKE["models/wakeword_* + wake_liva_* · ~2,8 MB"]
  end

  subgraph RT["Runtime engine"]
    LLAMA["llama.cpp qua llama-cpp-2 (feature mtmd)<br/>n_ctx 4096 · n_threads 4 · n_gpu_layers 0"]
    ORT["ONNX Runtime qua ort 2.0.0-rc.11<br/>CPU-only, CO Y"]
    ESPK["espeak-ng.exe · shell-out G2P"]
  end

  CFG --> GGUF
  CFG --> MMP
  ENVD --> NEMO
  ENVD --> PIPER
  ENVD --> KOKV
  ENVD --> VIE
  ENVD --> PARA
  ENVD --> SVAD
  ENVD --> GTC
  ENVD --> TURN
  ENVD --> WAKE

  GGUF --> LLAMA
  MMP -.->|"chi build RELEASE"| LLAMA
  NEMO --> ORT
  PIPER --> ORT
  KOKV --> ORT
  KOK -.->|"thieu file · bo qua (lazy)"| ORT
  VIE -.->|"opt-in LIVA_TTS_VIENEU=1"| ORT
  PARA -.->|"opt-in LIVA_STT_VI_ENGINE=parakeet"| ORT
  SVAD -.->|"chi bin standalone"| ORT
  GTC -.->|"chi bin standalone"| ORT
  TURN -.->|"LIVA_TURN_SHADOW_ENABLED=1"| ORT
  WAKE -.->|"LIVA_WAKE_MODE khac Off"| ORT

  PIPER --> ESPK
  LLAMA --> CPUD["CPU · VRAM = 0"]
  ORT --> CPUD
```

---

## 5. Bảng tài nguyên RAM/VRAM

> ⚠️ **ĐÂY LÀ ƯỚC TÍNH.** Các con số RAM dưới đây được suy ra từ **kích thước file thật trên đĩa** cộng overhead runtime thông thường, **chưa đo bằng profiler**. Chỉ dùng để lên kế hoạch dung lượng máy, **không dùng làm số liệu công bố**.
>
> Cột "File thật trên đĩa" thì **chính xác** (đo bằng `ls`/`Get-Item`).

| Model | File thật trên đĩa | RAM ước tính | VRAM | Thiết bị |
|---|---|---|---|---|
| Nemotron ASR (3 ONNX + tokenizer) | ≈ **788 MB** | ~0,9–1,1 GB | 0 | **CPU** (ORT) |
| Router LLM GGUF | Qwen3-VL-2B Q4_K_M **1,11 GB** (hoặc gemma 4–7 GB) | ~3–8 GB + KV-cache `n_ctx=4096` | **0** vì `LIVA_LLM_N_GPU_LAYERS`=0 | **CPU** (llama.cpp, 4 threads) |
| VieNeu-TTS v3 Turbo (opt-in) | ≈ **569 MB** | ~0,7 GB | 0 | **CPU** |
| Parakeet-vi CTC 0.6B (opt-in) | 41,9 MB + **2,27 GiB** | ~2,4–2,8 GB | 0 | **CPU** |
| Piper TTS vi + en | 2 × 63,2 MB | ~0,08–0,15 GB | 0 | **CPU** |
| Kokoro TTS EN | **KHÔNG tồn tại** | 0 | 0 | — |
| Silero VAD v6 | 2,33 MB | ~10 MB | 0 | CPU — **không nạp trong Tauri** |
| GTCRN denoise | 536 KB | ~5 MB | 0 | CPU — chỉ bin standalone |
| Smart Turn v3.2 | 8,68 MB | ~20 MB | 0 | CPU — opt-in |
| Wake-word (3 model) | ≈ **2,8 MB** | ~10 MB | 0 | CPU — mặc định `Off` |
| **Tổng đường mặc định** (Tauri + Nemotron + router + Piper) | — | **≈ 4–9 GB RAM** | **≈ 0 VRAM** | **CPU thuần**, kể cả khi build có `cuda` |

### 5.1 Vì sao VRAM = 0 dù có card rời

Ba lý do độc lập, phải gỡ **cả ba** mới thấy GPU được dùng:

1. **`LIVA_LLM_N_GPU_LAYERS` default = `0`** trong code (`main.rs:131`, `lib.rs:338`) — `.env.example:37` ghi `99`, nhưng vì **không có cơ chế nạp `.env`**, giá trị 99 đó không bao giờ tới được tiến trình.
2. **Build mặc định không bật `cuda`** — `default = []` (`liva-native-core/Cargo.toml:65-69`). Không có backend GPU thì `n_gpu_layers` truyền vào llama.cpp cũng vô nghĩa.
3. **ORT cố ý KHÔNG bật CUDA** — mọi ONNX (STT, TTS, VAD, wake, denoise, turn) chạy CPU-only. Đây là quyết định có chủ đích, xem comment `stt/parakeet.rs:180-185` giải thích nguy cơ dính bẫy khởi tạo backend nêu ở `models/README.md:23`.

Hệ quả: **task GPU downshift game-aware tự `return` ngay** vì `normal_layers == 0` (`main.rs:271`, `lib.rs:419`) — cơ chế nhường GPU cho game hiện **[MỘT PHẦN]**, có code nhưng không bao giờ chạy tới nhánh làm việc.

> 📌 Nguồn đầy đủ về ngưỡng governor và logic phát hiện tải nặng: [Thị giác passive và governor](../01-ban-ve/06-thi-giac-passive-va-governor.md)

Muốn thật sự dùng GPU cho LLM:

```powershell
# 1) build release có cuda
cargo build --release --features cuda
# 2) đặt biến TRƯỚC khi chạy (không có .env loader!)
$env:LIVA_LLM_N_GPU_LAYERS = "99"
$env:LIVA_GAME_N_GPU_LAYERS = "20"   # số layer khi phát hiện game
```

---

## 6. Điều kiện tiên quyết build

### 6.1 Rust / native

| Yêu cầu | Chi tiết | Vì sao |
|---|---|---|
| **Rust ≥ 1.85** | `liva-native-core` dùng `edition = "2024"`; `liva-desktop/src-tauri` dùng `edition = "2021"` | edition 2024 cần toolchain 1.85+ |
| **CMake + trình biên dịch C++** | MSVC Build Tools trên Windows | `llama-cpp-sys-2` build **llama.cpp từ mã nguồn C++** |
| **LLVM + biến `LIBCLANG_PATH`** | CI đặt tường minh: `.github/workflows/test.yml` → `env: LIBCLANG_PATH: 'C:\Program Files\LLVM\bin'` + bước `run: choco install llvm -y` | `bindgen` cần libclang |
| **Mạng ở lần build đầu** | `ort = "2.0.0-rc.9"` (`liva-native-core/Cargo.toml:30`, resolve thật là **rc.11**) **không** đặt `default-features = false` ⇒ bật `download-binaries` ⇒ tải ONNX Runtime từ `https://cdn.pyke.io/0/pyke:ort-rs/ms@1.23.2/x86_64-pc-windows-msvc.tar.lzma2` (`ort-sys/build/download/dist.txt:6`), verify SHA256, cache trong `~/.cargo` | **Điểm phụ thuộc mạng cứng nhất của toàn dự án** — mọi thứ STT/TTS/VAD/wakeword đều đứng trên `ort` |
| **CUDA toolkit** | chỉ khi build `--features cuda`. RTX 5060 Ti / Blackwell cần **CUDA 12.8+** và `CUDAARCHS=120a-real` (comment `liva-desktop/src-tauri/Cargo.toml:22-24`) | backend GPU của llama.cpp |
| **WebView2 Runtime** | Windows 11 có sẵn | vỏ Tauri v2 render UI |

**Build đầu rất lâu** — root `Cargo.toml:8-12` ép `opt-level = 3` cho `llama-cpp-2` và `llama-cpp-sys-2` **ngay cả ở profile dev**:

```toml
[profile.dev.package.llama-cpp-2]
opt-level = 3

[profile.dev.package.llama-cpp-sys-2]
opt-level = 3
```

**Output tập trung ở `E:\Project\LIVA\target\`** (workspace root). Thư mục `liva-native-core\target\` là **tàn dư tiền-workspace**, đừng tìm binary ở đó.

### 6.2 Nhị phân ngoài phải có trên PATH

| Binary | Bắt buộc? | Resolver | Hậu quả nếu thiếu |
|---|---|---|---|
| **`espeak-ng`** | Có, nếu dùng Piper/Kokoro | `LIVA_ESPEAK_PATH` → PATH → `C:\Program Files\eSpeak NG\espeak-ng.exe` → `C:\Program Files (x86)\...` (`tts/espeak.rs:11-36`), cache `OnceLock` | Nếu tất cả fail thì spawn tên trần và lỗi lúc gọi — G2P chết ⇒ TTS không phát âm |
| **`ffmpeg`** | Chỉ cho voice Telegram | `tokio::process::Command::new("ffmpeg")` (`telegram.rs:333`) — **không có fallback dò đường dẫn** | `"ffmpeg decoding failed"` (`telegram.rs:350`). Temp file `%TEMP%/tg_voice_{id}.ogg\|.raw` (`telegram.rs:329-330`) |

### 6.3 Node / JS

- **Node ≥ 20** (`package.json:5-7`); CI dùng **Node 22** + `npm ci`.
- Pre-commit (`.husky/pre-commit`): `npx lint-staged` rồi `node scripts/ai-pre-commit.cjs`.
  - `.lintstagedrc.json` **chỉ có** `"*.ts": ["eslint --max-warnings 0 --no-warn-ignored"]` — **KHÔNG có `tsc`**, và **`*.vue` không được xử lý**, trái với mô tả trong `CLAUDE.md`.
  - `ai-pre-commit.cjs` **cần file `.env`** với `AI_BASE_URL` / `AI_API_KEY` / `AI_MODEL` (fallback `http://127.0.0.1:8000/v1`, `local-ghost-router`, `gemma-4-E4B-it-Q6_K.gguf` — **model này KHÔNG có** trên `E:\AI_Models`, ở đó chỉ có `-Q4_K_M` và `gemma-4-E4B_q4_0-it.gguf`). Bypass: `SKIP_AI_HOOK=1` (`ai-pre-commit.cjs:8`).
- CI (`.github/workflows/test.yml`, windows-latest) chạy vitest cho `liva-ui` + `cargo test` cho core; clippy có chạy nhưng không chặn, không có gate `fmt`. Điều đáng nhớ ở đây là **CI cài `choco install llvm` và đặt `LIBCLANG_PATH`** — tức máy local cũng phải có đủ hai thứ đó thì mới build được.

  > 📌 Nguồn đầy đủ (bảng test, bảng binary verify, từng bước CI pipeline): [Kiểm thử và CI](04-kiem-thu-va-ci.md)

---

## 7. Feature flags `cuda` / `vulkan` / `openblas`

Khai báo thật, `liva-native-core/Cargo.toml:65-69`:

```toml
[features]
default = []
cuda = ["llama-cpp-2/cuda"]
vulkan = ["llama-cpp-2/vulkan"]
openblas = []
```

Forward ở vỏ Tauri, `liva-desktop/src-tauri/Cargo.toml:20-26`:

```toml
[features]
cuda = ["liva-native-core/cuda"]
vulkan = ["liva-native-core/vulkan"]
```

| Flag | Thực tế làm gì | Trạng thái |
|---|---|---|
| `cuda` | Pass-through tới `llama-cpp-2/cuda`. **Chỉ** khi build với nó thì `LIVA_LLM_N_GPU_LAYERS` / `LIVA_GAME_N_GPU_LAYERS` mới có tác dụng thật | **[MỘT PHẦN]** — hoạt động nhưng cần thêm biến env mới thấy hiệu quả |
| `vulkan` | Pass-through tới `llama-cpp-2/vulkan`, được forward ở Tauri | **[MỘT PHẦN]** — chưa có bằng chứng đã kiểm chứng chạy |
| `openblas` | **Feature RỖNG hoàn toàn** — `openblas = []`, không map sang `llama-cpp-2/openblas`, không có `#[cfg]` nào. Bật hay không **không đổi gì** | **[THIẾU]** — dead flag |

### 7.1 Ba sự thật hay bị hiểu nhầm

1. **Không có một `#[cfg(feature = "cuda" | "vulkan" | "openblas")]` nào trong toàn bộ source** (grep → 0 kết quả). LIVA **không rẽ nhánh code** theo feature flag; ba flag chỉ là đường ống tới `llama-cpp-2`.
2. **`openblas` là dead flag.** `CLAUDE.md` liệt kê `--features openblas` như một lựa chọn build hợp lệ — điều này **sai**. Vỏ Tauri thậm chí **không forward** nó.
3. **Vision chỉ chạy ở build RELEASE — đây là ràng buộc RUNTIME, không phải feature flag.** Guard nằm ở `llm/engine.rs:371-377`:

```rust
// Vision needs a RELEASE build on Windows: the debug CRT asserts in the
// clip/mmproj file loader (llama.cpp links the debug CRT, Rust the release
// one → fd-table mismatch) and aborts the process.
if cfg!(all(windows, debug_assertions)) {
    return Err(
        "Vision requires a release build (debug CRT assertion in the mmproj loader) — \
         run the core with `cargo build --release`."
            .to_string(),
    );
}
```

Nghĩa là: `cargo build` (debug) → gọi `vision:ask` sẽ trả lỗi sạch chứ không crash; muốn dùng vision **bắt buộc** `--release`.

### 7.2 Lệnh build tham chiếu

```powershell
# CPU thuần, mặc định — build được nhưng KHÔNG dùng vision
cd liva-native-core
cargo build

# Release + CUDA — cấu hình duy nhất chạy được vision + GPU offload
cargo build --release --features cuda

# Bản desktop
npx tauri build -- --features cuda

# Blackwell (RTX 5060 Ti): cần CUDA 12.8+
$env:CUDAARCHS = "120a-real"
```

---

## 8. Checklist trước khi chạy trên máy mới

| # | Kiểm tra | Cách xác minh | Hậu quả nếu thiếu |
|---|---|---|---|
| 1 | `models/nemotron-asr/` đầy đủ (~788 MB) | `ls models/nemotron-asr` | STT chết — `SttManager` fail |
| 2 | `node_modules/kokoro-js/voices/af_heart.bin` tồn tại | `ls node_modules/kokoro-js/voices/` | **Mất toàn bộ TTS** (kể cả Piper/VieNeu) — đọc EAGER |
| 3 | `models/piper/*.onnx` + `.onnx.json` | `ls models/piper` | Mất giọng vi/en local-first |
| 4 | `E:\AI_Models\Qwen3-VL-2B-Instruct-GGUF/…Q4_K_M.gguf` | đối chiếu `data/liva-config.json` | **Panic khi khởi động** — `LlamaRouterManager::new` lỗi |
| 5 | `espeak-ng` trên PATH | `espeak-ng --version` | TTS không phát âm |
| 6 | `LIBCLANG_PATH` trỏ tới LLVM | `$env:LIBCLANG_PATH` | Build fail ở bindgen |
| 7 | CMake + MSVC Build Tools | `cmake --version` | Build fail ở `llama-cpp-sys-2` |
| 8 | Mạng ở lần build đầu | — | `ort` không tải được ONNX Runtime ⇒ fail |
| 9 | `ffmpeg` trên PATH | `ffmpeg -version` | Chỉ mất voice Telegram (không bắt buộc) |
| 10 | RAM trống ≥ ~9 GB | — | Swap nặng khi nạp router LLM (ước tính, xem mục 5) |

---

## Liên quan

**Đọc tiếp theo mạch:** [◀ Cấu hình và biến môi trường](01-cau-hinh-va-bien-moi-truong.md) · [Triển khai và runtime ▶](03-trien-khai-va-runtime.md)

**Tài liệu này dựa vào (nguồn sự thật ở nơi khác):**

- [Cấu hình và biến môi trường](01-cau-hinh-va-bien-moi-truong.md) — bảng đầy đủ biến `LIVA_*` (đường dẫn model, `LIVA_LLM_N_GPU_LAYERS`, các cờ opt-in) và danh sách chỗ lệch `.env.example` vs code
- [Kiểm thử và CI](04-kiem-thu-va-ci.md) — bảng test, binary verify và các bước CI; ở đây chỉ trích phần CI cài LLVM/`LIBCLANG_PATH`
- [Thị giác passive và governor](../01-ban-ve/06-thi-giac-passive-va-governor.md) — ngưỡng governor và logic game-aware mà mục "VRAM = 0" tham chiếu
- [Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) — chuỗi xử lý thoại, bảng backend TTS và engine STT tiêu thụ các model liệt kê ở đây
- [Hệ LLM và prompt](../01-ban-ve/04-he-llm-va-prompt.md) — cấu hình LLM, persona, cách router dùng file GGUF
- [Báo cáo khảo sát gốc 2026-07](../03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md) — nguồn dữ liệu đo đạc gốc

**Tài liệu khác dựa vào tài liệu này:**

- [Triển khai và runtime](03-trien-khai-va-runtime.md) — lấy điều kiện tiên quyết build và checklist model trước khi chạy
- [Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md) — lấy kích thước/tình trạng file model STT-TTS thật trên đĩa
- [Hệ LLM và prompt](../01-ban-ve/04-he-llm-va-prompt.md) — lấy bảng GGUF ở `E:\AI_Models` và trạng thái router/expert/mmproj
- [Đối chiếu tuyên bố vs thực tế](../03-danh-gia/01-doi-chieu-tuyen-bo-vs-thuc-te.md) — lấy bằng chứng "chạy CPU thuần, VRAM = 0" và các model thiếu
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — lấy các cấu hình chết (`models.config.json`, `openblas`) và phụ thuộc EAGER `af_heart.bin`

**Khi sửa code sau đây thì phải cập nhật tài liệu này:**

- `data/liva-config.json` — nguồn thật của đường dẫn LLM GGUF (mục 1, mục 3)
- `liva-desktop/src-tauri/src/lib.rs` — hằng số `DEFAULT_MODELS_DIR` / `DEFAULT_ROUTER_MODEL`, `configured_mmproj_path()`, default `n_gpu_layers` (mục 1, 3, 5.1)
- `liva-native-core/Cargo.toml` + `liva-desktop/src-tauri/Cargo.toml` — khai báo và forward feature `cuda`/`vulkan`/`openblas`, phiên bản `ort` (mục 6.1, mục 7)
- `Cargo.toml` (root) — override `opt-level = 3` cho `llama-cpp-*` ở profile dev (mục 6.1)
- `liva-native-core/src/llm/engine.rs` — guard "vision cần build RELEASE" (mục 7.1)
- `liva-native-core/src/tts/*` (đặc biệt `tts/mod.rs`, `tts/engine.rs`, `tts/vieneu/mod.rs`) — Kokoro lazy vs `af_heart.bin` EAGER, danh sách file VieNeu (mục 2, 2.1, 5)
- `liva-native-core/src/stt/parakeet.rs` — model Parakeet-vi, lý do ORT CPU-only (mục 2, 5.1)
- `liva-native-core/src/telegram.rs` — phụ thuộc `ffmpeg` (mục 6.2)
- `scripts/ai-pre-commit.cjs` + `.github/workflows/test.yml` — yêu cầu Node/`.env` và điều kiện build phía CI (mục 6.3)
