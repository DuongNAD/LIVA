---
title: "Voice SLO và acceptance gates"
updated: 2026-07-30
commit: 3688b5f
stale-ok: a0153135
status: living
owns:
  - bang-nguong-vad-aec-denoise
  - voice-slo
covers:
  - liva-native-core/src/webrtc/vad.rs
  - liva-native-core/src/webrtc/session.rs
  - liva-native-core/src/webrtc/aec.rs
  - liva-native-core/src/webrtc/denoise.rs
  - liva-native-core/src/webrtc/pipeline.rs
  - liva-native-core/src/webrtc/frame.rs
  - liva-native-core/src/bin/voice_stress.rs
  - liva-native-core/src/bin/ttft_bench.rs
---
# Voice SLO và acceptance gates

[⬆ Mục lục](../README.md) · [Voice runtime](../03-he-thong-con/voice.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

## 1. Trạng thái

Voice đang ở trạng thái `partial` vì chưa có baseline p50/p95 trên Tauri release. Các giá trị dưới
đây là **cấu hình runtime**, không phải kết quả SLO.

## 2. Ngưỡng runtime hiện hành

| Thành phần | Giá trị mặc định sản phẩm | Nguồn |
|---|---:|---|
| Sample rate VAD | 16.000 Hz | `liva-native-core/src/webrtc/vad.rs#VadConfig::default` |
| Kích thước VAD frame | 512 mẫu = 32 ms | `liva-native-core/src/webrtc/vad.rs#VadConfig::default` |
| VAD probability threshold | 0,5 | `liva-native-core/src/webrtc/vad.rs#VadConfig::from_env` |
| Speech start debounce | 3 frame ≈ 96 ms | `liva-native-core/src/webrtc/vad.rs#VadConfig::from_env` |
| Speech end debounce | 22 frame ≈ 704 ms | `liva-native-core/src/webrtc/vad.rs#VadConfig::from_env` |
| Denoise | bật nếu model load được | `liva-native-core/src/webrtc/session.rs#VoiceRuntimeConfig::from_env` |
| SmartTurn | tắt | `liva-native-core/src/webrtc/session.rs#VoiceRuntimeConfig::from_env` |
| Self AEC | tắt | `liva-native-core/src/webrtc/session.rs#VoiceRuntimeConfig::from_env` |
| AEC frame | 160 mẫu = 10 ms ở 16 kHz | `liva-native-core/src/webrtc/aec.rs#SelfEchoCanceller::new` |
| Speaker chunk | tối đa 100 ms PCM/frame | `liva-native-core/src/webrtc/frame.rs#speaker_frames` |

Env override phải đi qua parser hiện hành; benchmark phải ghi lại toàn bộ override để có thể tái
lập.

## 3. SLO cần khóa cho beta

| SLO | Điểm bắt đầu → kết thúc | Thống kê | Baseline |
|---|---|---|---|
| Turn latency | VAD `SpeechEnd` → speaker frame đầu tiên | p50, p95 | chưa đo trên Tauri release |
| Barge-in stop | VAD `SpeechStart` → audio cũ im hẳn | p50, p95, max | chưa đo trên Tauri release |
| STT realtime factor | thời lượng inference / thời lượng utterance | p50, p95 theo độ dài | probe có nhưng chưa khóa corpus |
| TTS TTFS | clause ready → PCM đầu tiên | p50, p95 theo backend | chưa khóa corpus |
| Drop/backpressure | frame mất hoặc queue full / tổng frame | tỷ lệ + số turn | chưa có dashboard |
| Idle resource | không hội thoại, wake theo UX beta | CPU/RAM/GPU | chưa đo release |

Không điền target số trước khi có baseline từ cùng máy, cùng model và cùng build profile.

## 4. Ma trận benchmark bắt buộc

- Build: Tauri release, không dùng dev server làm số beta.
- Thiết bị: ít nhất một máy CPU-only và một máy có CUDA mục tiêu.
- Ngôn ngữ: Việt, Anh, xen kẽ Việt–Anh.
- Điều kiện: phòng yên, quạt/nhiễu nền, loa LIVA đang phát để kiểm barge-in.
- Độ dài utterance: ngắn, trung bình, dài.
- Backend TTS: Piper; VieNeu khi bật; Kokoro fallback cưỡng bức.
- Ghi kèm commit, model manifest hash, env override và audio device.

## 5. Acceptance commands

```powershell
cargo test --manifest-path liva-native-core/Cargo.toml --test voice_runtime_components
cargo test --manifest-path liva-native-core/Cargo.toml --test websocket_transport
npm run e2e:gateway:release
```

Benchmark phần cứng phải chạy thêm `voice_stress`/`ttft_bench` trên binary release và lưu raw
measurement; unit test không thay thế được phép đo này.

## 6. Gate nâng capability lên working

- Có p50/p95 cho turn latency, barge-in và TTFS.
- Không có speaker frame epoch cũ sau flush.
- Queue đầy fail-fast và có quan sát được lỗi.
- Fallback TTS không chạy backend tiếp theo sau cancellation.
- Số đo tái lập được từ metadata build/model/env.
- Không log audio hoặc transcript nhạy cảm ngoài chế độ chẩn đoán có consent.
