---
title: "Wake-word benchmark và acceptance gate"
updated: 2026-07-31
commit: 3688b5f
status: living
owns:
  - wake-benchmark
covers:
  - liva-native-core/src/wake.rs
  - liva-native-core/src/wake_model.rs
  - liva-native-core/src/stt/mod.rs
  - liva-ui/src/workers/LivaWakeWorker.ts
  - liva-native-core/src/bin/wakeword_benchmark.rs
  - scripts/e2e-wake-probe.mjs
  - scripts/prepare-wake-enrollment.mjs
  - scripts/train-wakeword.ps1
  - tools/wakeword/hey_liva_prod.yaml
---

# Wake-word benchmark và acceptance gate

[⬆ Mục lục](../README.md) · [Wake architecture](../03-he-thong-con/wake-word.md) · [Master roadmap](../06-ke-hoach/roadmap.md)

## 1. Baseline lịch sử 27/07/2026

Baseline này được giữ để ra quyết định sản phẩm, không được diễn giải thành số đo của model mới.

| Phép đo                           |                                  Kết quả |
| --------------------------------- | ---------------------------------------: |
| “Hey Liva” giọng thật, 8 mẫu      | classifier 0,004–0,025; trung bình 0,014 |
| Threshold hiện hành               |                                     0,68 |
| Fixture tham chiếu của classifier |         positive 0,9997; negative 0,0009 |
| 28 câu tiếng Việt nền TV/video    |                             0 false wake |
| Câu dài chứa “Liva”               |          wake thành công qua socket thật |

Kết luận: classifier pipeline hoạt động, nhưng artifact hiện tại không nhận đủ giọng mục tiêu.
Hạ threshold xuống gần 0,014 sẽ đặt nó ngang vùng nhiễu và không được chấp nhận.

## 2. Acceptance cho classifier cá nhân

| Chỉ số                     |                                           Gate |
| -------------------------- | ---------------------------------------------: |
| Recall positive            |                                          ≥ 90% |
| False positives per hour   |                                       ≤ 1 FPPH |
| Thời lượng negative corpus |                                        ≥ 1 giờ |
| Thiết bị                   |                            mic và máy mục tiêu |
| Môi trường                 | yên tĩnh + TV/video + tiếng nói không gọi LIVA |
| Báo cáo                    |  confusion matrix, threshold sweep, model hash |

Recall đơn lẻ không đủ; threshold chỉ được chọn trên đường cong recall/FPPH.

## 3. Kết quả artifact v2 tổng hợp ngày 31/07/2026

| Thuộc tính          |                                                            Kết quả |
| ------------------- | -----------------------------------------------------------------: |
| Artifact            |                                             `wake_liva_en_v2.onnx` |
| Kiến trúc           |                                                   `conv_attention` |
| Positive validation |                                                        15.000 clip |
| Negative validation |                                            46.584 clip / 25,88 giờ |
| Threshold phát hành |                                                               0,58 |
| Recall              |                                                             91,82% |
| FPPH                |                                                             0,0773 |
| SHA-256             | `7487fcb480ce05a6ba02901ee48071caed73b593dfed5d9b55202ae1001c4780` |

Kết quả này vượt gate số học trên corpus tổng hợp và dùng để chọn artifact/ngưỡng mặc định.
Nó không phải bằng chứng recall giọng người dùng; gate mic mục tiêu trong mục 2 vẫn bắt buộc.

Tám mẫu giọng thật sau đó chỉ đạt score 0,004–0,025. Điều tra pipeline xác nhận trang
enrollment đã tạo WAV đúng, nhưng wrapper train cũ không đọc `data/wake-enrollment/positive`;
vì vậy artifact trong bảng không hề học các mẫu này.

## 4. Benchmark ứng viên OSS ngày 31/07/2026

Các probe được chạy ngoài runtime LIVA, trên cùng bài toán “Hey Liva”; chúng không được đưa
vào app vì đều trượt gate:

| Ứng viên                  |          Positive / negative | Recall | False positive |    FPPH |
| ------------------------- | ---------------------------: | -----: | -------------: | ------: |
| sherpa-onnx GigaSpeech    |                    200 / 200 |  30,5% |              9 |   92,87 |
| sherpa-onnx zh-en phoneme |                    200 / 200 |  27,5% |             12 |  123,83 |
| rustpotter, mặc định      | 5 enrollment giống mẫu / 100 |    60% |              0 |       0 |
| rustpotter, threshold 0,3 | 5 enrollment giống mẫu / 100 |   100% |             81 | 1.866,5 |

Số rustpotter mặc định không phải kết quả đạt: positive ở đây là bản trùng enrollment, vẫn hụt
40%. Việc hạ threshold lấy lại recall nhưng làm false wake tăng không chấp nhận được.

Kết luận: không thay runtime bằng một model OSS có sẵn. Dùng pipeline LiveKit/openWakeWord đã
pin, nhưng chèn enrollment thật trước augmentation và vẫn bắt buộc benchmark độc lập.
Fallback ASR đồng thời được siết thành prefix thật: nhắc “hey liva” ở giữa câu không còn đủ để
mở gate.

## 5. Corpus

- Positive: nhiều tốc độ, khoảng cách, âm lượng và ngữ điệu của chính người dùng.
- Near-positive: “Liva” trong câu không gọi trợ lý, tên tương tự, TV nói gần giống.
- Negative speech: hội thoại Việt/Anh trong phòng.
- Negative non-speech: TV, nhạc, quạt, bàn phím, tiếng ho.
- Mỗi clip phải ghi sample rate, thiết bị, nhãn và quyền lưu.

Không dùng chỉ giọng Piper để kết luận về recall người thật.

## 6. Cách chạy

```powershell
# Train bằng upstream LiveKit WakeWord đã pin commit
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword.ps1 -Action Install
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword.ps1 -Action Setup
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword.ps1 -Action Personalize

# Gate corpus trên artifact đã export
npm run wake:benchmark -- --model models/wake_liva_en_v2.onnx `
  --positive data/wake-enrollment/positive `
  --negative data/wake-enrollment/negative `
  --threshold 0.58 `
  --report tools/wakeword/work/wake_liva_en_v2-report.json

# Gate đường dây gateway
node scripts/e2e-wake-probe.mjs <file.wav> 8002
```

`wakeword_benchmark` trả `0` khi đồng thời đạt recall, FPPH và thời lượng negative
corpus; trả `2` khi model không đạt. Report JSON chứa confusion counts, threshold,
model hash và score từng clip. Runner E2E phải dùng gateway/model thật.

Toolchain huấn luyện dùng repository chính thức
`https://github.com/livekit/livekit-wakeword`, pin SHA trong
`tools/wakeword/toolchain.json`; cấu hình production dùng `conv_attention`, 25.000
mẫu mỗi lớp và 100.000 bước.

Đường probe và quyết định core được neo tại
`liva-native-core/src/wake.rs#WakeGate::score_clip` và
`liva-native-core/src/wake.rs#WakeGate::matches_phrase`.

## 7. Gate phát hành

- UX tiếp tục dùng câu dài nếu chưa đạt toàn bộ gate.
- Không thay threshold mặc định chỉ từ vài clip positive.
- Model mới phải có version/hash và rollback được.
- Raw voice sample là dữ liệu nhạy cảm: opt-in, retention rõ, xóa được.
- Benchmark mới phải giữ baseline cũ trong mục lịch sử, không ghi đè.
- Không dùng lại `generate_hey_liva_model.py`, weights MLP năng lượng hoặc ONNX browser
  cũ; các artifact đó đã được gỡ ngày 31/07/2026.
