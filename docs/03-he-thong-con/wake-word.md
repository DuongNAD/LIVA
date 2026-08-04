---
title: "Wake word — kiến trúc và giới hạn sản phẩm"
updated: 2026-08-02
commit: 3688b5f
status: living
owns:
  - wake-word-viec-con-lai
covers:
  - liva-native-core/src/wake.rs
  - liva-native-core/src/wake_model.rs
  - liva-native-core/src/websocket.rs
  - liva-native-core/src/webrtc/frame.rs
  - liva-native-core/src/stt/mod.rs
  - liva-ui/src/composables/useVoicePipeline.ts
  - liva-ui/src/workers/LivaWakeWorker.ts
  - liva-ui/public/wake-word-test.html
  - liva-native-core/src/bin/wakeword_benchmark.rs
  - scripts/e2e-wake-probe.mjs
  - scripts/prepare-wake-enrollment.mjs
  - scripts/train-wakeword.ps1
  - scripts/fetch-wake-public-corpus.py
  - scripts/prepare-wake-variants.mjs
  - scripts/select-wake-candidate.mjs
  - scripts/train-wakeword-matrix.ps1
  - tools/wakeword/hey_liva_prod.yaml
  - tools/wakeword/public-datasets.json
  - tools/wakeword/variants.json
  - tools/wakeword/sherpa-kws-probe
  - tools/wakeword/rustpotter-probe
---

# Wake word — kiến trúc và giới hạn sản phẩm

[⬆ Mục lục](../README.md) · [Voice runtime](voice.md) · [Wake benchmark](../05-chat-luong/wake-benchmark.md)

## 1. Trạng thái sản phẩm

- Câu đầy đủ bắt đầu bằng “Hey Liva” hoạt động trên đường beta hiện tại.
- Artifact `wake_liva_en_v2.onnx` đã train xong và vượt gate tổng hợp; “Hey Liva” đứng riêng
  vẫn cần nghiệm thu bằng giọng thật trên mic mục tiêu trước khi quảng bá là production-ready.
- Không được hạ threshold để che lỗi model: việc đó mở lại false-positive.
- UX beta phải hướng dẫn “Hey Liva” kèm lệnh cho tới khi classifier cá nhân vượt acceptance gate.
- Toolchain train, trang thu mẫu chủ động và benchmark corpus đã có. Artifact v2 cũ đạt recall
  91,82% và FPPH 0,0773 tại threshold 0,58 trên 25,88 giờ validation tổng hợp; gate giọng thật
  và máy mục tiêu vẫn mở. Artifact này chưa được cá nhân hóa và không được coi là bản phát hành
  cho câu gọi đứng riêng.
- Đo lại bằng đúng runtime Rust ngày 02/08/2026 trên 24 WAV thật của chủ máy: artifact đang
  phát hành chỉ nhận 1/24 ở threshold 0,58 (recall 4,17%). Đây là bằng chứng domain shift;
  tuyệt đối không hạ threshold vì 23 mẫu trượt chủ yếu chỉ đạt 0,002–0,032.

Trạng thái capability do `experience.wake-word` trong `docs/_data/capabilities.json` sở hữu.

## 2. Đường thực thi

```mermaid
flowchart LR
    MIC["Mic local"] --> WORKER["LivaWakeWorker cắt utterance"]
    WORKER --> PROBE["VoiceFrame OP_WAKE_PROBE"]
    PROBE --> WS["WebSocketServer"]
    WS --> GATE["WakeGate"]
    GATE --> CLASSIFIER["trained classifier"]
    GATE --> ASR["STT prefix"]
    CLASSIFIER --> DECISION["wake/reject"]
    ASR --> DECISION
```

- UI gửi probe qua `liva-ui/src/composables/useVoicePipeline.ts#sendWakeProbe`.
- Wire opcode thuộc `liva-native-core/src/webrtc/frame.rs#OP_WAKE_PROBE`.
- Quyết định ở core được cấu hình tại `liva-native-core/src/wake.rs#WakeGate::from_env`.
- Chuẩn hóa transcript trước so cụm từ tại
  `liva-native-core/src/wake.rs#normalize_for_match`.
- Classifier ONNX được dựng tại `liva-native-core/src/wake_model.rs#TrainedWakeDetector::new`.

Browser worker chỉ cắt câu và quản lý lifecycle; nó không được tự quyết định wake bằng RMS.
Sàn utterance hiện là 500 ms: đoạn giọng 320 ms bị loại trước STT, trong khi câu gọi
khoảng 640 ms vẫn đi qua.

## 3. Modes và cấu hình

`liva-native-core/src/wake.rs#WakeGate::from_env` hỗ trợ:

| Mode            | Hành vi                                       |
| --------------- | --------------------------------------------- |
| `off`           | không gate bằng wake; đây là mặc định runtime |
| `asr_prefix`    | STT rồi so với danh sách phrase               |
| `trained_model` | chỉ dùng classifier ONNX                      |
| `hybrid`        | classifier hoặc STT phrase                    |

Các cấu hình chính:

- `LIVA_WAKE_PHRASES`: mặc định chỉ có đúng `hey liva`; CSV chỉ dành cho ghi đè thử nghiệm;
- `LIVA_WAKE_WINDOW_SECS`: cửa sổ tỉnh, mặc định 45 giây;
- `LIVA_WAKE_MODEL_PATHS`: classifier ONNX; để trống thì tự dùng
  `models/wake_liva_en_v2.onnx` và kiểm SHA-256 theo manifest;
- `LIVA_WAKE_THRESHOLD`: ngưỡng classifier, mặc định 0,58.

## 4. Ranh giới an toàn

- Không gửi mic liên tục chỉ vì classifier thiếu hoặc lỗi.
- Fallback ASR chỉ nhận wake phrase ở đầu utterance (cho phép `hey` đứng trước phrase tùy chỉnh);
  không tìm chuỗi ở giữa tám từ đầu như bản cũ.
- Thiếu classifier trong `trained_model` phải fail rõ; `hybrid` được phép rơi về tầng STT với cảnh báo.
- Path model đi qua resource resolver của runtime Tauri.
- Stop pipeline phải vô hiệu startup đang chờ quyền mic và terminate worker.
- Core phải trả phán quyết cho worker. `wake_probe_rejected` xóa cooldown ngay để một probe
  TV/nhiễu không nuốt câu “Hey Liva” thật kế tiếp; `wake_word_triggered` vẫn giữ cooldown để
  chống kích hoạt kép.
- Không quảng cáo “Hey Liva” trần trước khi đạt recall/FPPH trên giọng người dùng.
- Từ 31/07/2026, UX beta dùng câu đầy đủ bắt đầu bằng `Hey Liva` và regression gate
  `scripts/wake-ux-copy.test.mjs` khóa copy trên README, widget và trang thử microphone.
- Corpus giọng nằm trong `data/wake-enrollment/` và output train nằm trong
  `tools/wakeword/work/`; cả hai bị Git bỏ qua vì audio là dữ liệu sinh trắc học và
  output có thể rất lớn.

## 5. Kiểm chứng

```powershell
# Kiểm tra toolchain/config/hardware
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword.ps1 -Action Doctor

# Thu mẫu bằng trang liva-ui/public/wake-word-test.html, rồi benchmark model
npm run wake:benchmark -- --model models/wake_liva_en_v2.onnx `
  --positive data/wake-enrollment/positive `
  --negative data/wake-enrollment/negative `
  --report tools/wakeword/work/wake_liva_en_v2-report.json

# E2E qua gateway thật
node scripts/e2e-wake-probe.mjs <file.wav> 8002
```

Benchmark và probe dùng cùng quy ước: exit `0` là đạt/wake, exit `2` là không
đạt/reject. Runner benchmark ghi recall, FPPH, thời lượng negative corpus và SHA-256
model. Kết quả chất lượng thuộc [Wake benchmark](../05-chat-luong/wake-benchmark.md).

Toolchain pin cả commit LiveKit và PyTorch CUDA trong `tools/wakeword/toolchain.json`.
`Doctor`, `Train` và `Eval` fail-closed nếu `torch.cuda.is_available()` là false để tránh
vô tình chạy production corpus hàng chục nghìn clip trên CPU.

### 5.1. Train có personalization

`Train`, `Personalize` và `All` không còn chạy pipeline tổng hợp thuần. Trình tự bắt buộc là:

1. tạo đủ corpus tổng hợp theo cấu hình;
2. kiểm tra tối thiểu 20 positive và 20 hard-negative của chính chủ máy, đều là WAV thật
   PCM mono 16 kHz/16-bit;
3. tách recording gốc của từng lớp 80/20 trước khi nhân bản, sau đó đưa 10.000 bản vào train
   và 1.000 bản vào test để upstream tạo augmentation khác nhau;
4. augment → extract feature → train → export → eval.

Positive dùng dải `clip_800000…899999`; hard-negative dùng `clip_900000…999999`.
Script chỉ xóa lại đúng dải do enrollment tạo khi chạy lại; corpus tổng hợp không bị đụng tới.
Manifest schema 2 ghi riêng số recording/copy của hai lớp.

```powershell
# Sau khi đã thu ít nhất 20 positive và 20 hard-negative bằng trang microphone
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword.ps1 -Action Personalize

# Hoặc đọc WAV từ một thư mục khác
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword.ps1 -Action Personalize `
  -EnrollmentDir C:\duong-dan\positive `
  -NegativeEnrollmentDir C:\duong-dan\negative
```

### 5.2. Quyết định về mã nguồn mở

Ba hướng đã được đối chiếu bằng PoC cục bộ ngày 31/07/2026:

| Hướng                                  |                                           Kết quả trên corpus thử | Quyết định                        |
| -------------------------------------- | ----------------------------------------------------------------: | --------------------------------- |
| sherpa-onnx KWS GigaSpeech             |                                          recall 30,5%; 92,87 FPPH | Không tích hợp                    |
| sherpa-onnx KWS zh-en                  |                                         recall 27,5%; 123,83 FPPH | Không tích hợp                    |
| rustpotter personalized                | recall 60% ở ngưỡng mặc định; khi ép recall 100% thì 1.866,5 FPPH | Không tích hợp                    |
| LiveKit/openWakeWord artifact hiện tại | 1/24 positive thật ở threshold 0,58 (recall 4,17%) | Giữ kiến trúc, bắt buộc train cá nhân lại |

Hai probe Rust nằm tách khỏi Cargo workspace chính vì sherpa-onnx dùng ORT API 27 trong khi
native core đang nhúng API 17; liên kết trực tiếp sẽ gây xung đột ABI trên Windows. Probe là
bằng chứng tái lập, không phải dependency runtime.

### 5.3. Huấn luyện nhiều biến thể với public negatives

Ma trận mới giữ nguyên runtime classifier và chỉ thay đổi corpus/model size để đo tác động:

- control medium: synthetic + owner positive;
- FLEURS medium: thêm tiếng Việt không gọi LIVA;
- Commands medium: thêm từ khóa/ngữ âm dễ nhầm;
- hybrid medium/large: thêm FLEURS, Speech Commands và MUSAN noise.

Ba dataset được ghim revision và CC BY 4.0 trong `tools/wakeword/public-datasets.json`.
Downloader đọc Parquet theo batch, cân Speech Commands trên sáu shard, chuẩn hóa mono PCM16
16 kHz, chống trùng và giữ group split. Public test không đi vào train. Mỗi ứng viên dùng model
name/output riêng và selector không tự động ghi đè model phát hành.

Public corpus chỉ cải thiện negative diversity. Thiếu `data/wake-enrollment/negative/` hoặc
negative môi trường thật dưới một giờ thì mọi ứng viên vẫn là experimental, bất kể metric tổng hợp.

## 6. Việc tiếp theo

1. Giữ “Hey Liva” kèm lệnh làm UX beta.
2. Giữ 20 positive sạch hiện có; thu thêm tối thiểu 20 hard-negative bằng trang enrollment,
   rồi chạy `-Action Personalize`.
3. Thu ít nhất một giờ negative môi trường thật trên máy mục tiêu để tính FPPH độc lập.
4. Benchmark artifact personalized trên corpus giọng thật độc lập và lưu report có model hash.
5. Chỉ đổi UX mặc định sang “Hey Liva” trần sau khi corpus mic mục tiêu cũng đạt recall ≥90%
   và FPPH ≤1; kết quả tổng hợp không thay thế phép đo này.

Tham chiếu kỹ thuật: [openWakeWord](https://github.com/dscripka/openWakeWord) khuyến nghị
VAD, hiệu chỉnh threshold theo môi trường và verifier theo giọng khi false activation cao;
[sherpa-onnx KWS](https://k2-fsa.github.io/sherpa/onnx/kws/index.html) cũng tách rõ trigger
threshold/boosting score và yêu cầu cân bằng trigger rate với false alarm.
