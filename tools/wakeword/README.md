# Wake-word research tools

Thư mục này chứa toolchain production đã pin và hai probe tái lập để đánh giá ứng viên OSS.

- `hey_liva_prod.yaml`: cấu hình LiveKit WakeWord đang dùng để train.
- `sherpa-kws-probe/`: sidecar thử sherpa-onnx 1.13.4. Không liên kết vào native core vì
  khác ONNX Runtime ABI (API 27 so với API 17).
- `rustpotter-probe/`: probe personalized rustpotter 3.0.2; pin `half` 2.3.1 để khớp
  candle 0.2.2 của upstream.
- `work/`: model tải về, corpus, build target và report cục bộ; bị Git bỏ qua.

Personalization production cần tối thiểu 20 file trong cả hai thư mục:

- `data/wake-enrollment/positive/`: nói đúng “Hey Liva” ở nhiều tốc độ/khoảng cách;
- `data/wake-enrollment/negative/`: giọng chủ máy nói câu thường và near-miss nhưng không nói
  đúng câu gọi.

Trang `liva-ui/public/wake-word-test.html` thu cả hai lớp theo yêu cầu bấm nút. Chạy
`scripts/train-wakeword.ps1 -Action Personalize`; script fail-closed nếu thiếu một lớp.

## Ma trận corpus công khai

`public-datasets.json` ghim revision và giấy phép CC BY 4.0 của FLEURS tiếng Việt,
Speech Commands v2 và MUSAN noise. Corpus chỉ bổ sung negative speech/keyword/noise;
không thay positive hoặc hard-negative của chính chủ máy. Downloader chuẩn hóa PCM16 mono
16 kHz, loại đúng cụm `hey liva`, chống trùng SHA-256, giữ group split và ghi provenance
vào `work/public-corpus/metadata.jsonl`.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword-matrix.ps1 -Action Doctor
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword-matrix.ps1 -Action Fetch
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword-matrix.ps1 -Action Prepare
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword-matrix.ps1 -Action Train
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword-matrix.ps1 -Action Benchmark
powershell -ExecutionPolicy Bypass -File scripts/train-wakeword-matrix.ps1 -Action Select
```

Ma trận gồm control medium, FLEURS medium, Commands medium, hybrid medium và hybrid large.
Selector chỉ ghi ứng viên thử nghiệm; nó không chép model vào `models/` và chặn promotion
khi thiếu owner hard-negative, negative môi trường dưới một giờ, recall dưới 90% hoặc
FPPH trên 1.

Hai probe là công cụ benchmark, không phải dependency runtime và không chứng minh đạt production.
Kết quả chuẩn và acceptance gate nằm tại `docs/05-chat-luong/wake-benchmark.md`.
