# Wake-word research tools

Thư mục này chứa toolchain production đã pin và hai probe tái lập để đánh giá ứng viên OSS.

- `hey_liva_prod.yaml`: cấu hình LiveKit WakeWord đang dùng để train.
- `sherpa-kws-probe/`: sidecar thử sherpa-onnx 1.13.4. Không liên kết vào native core vì
  khác ONNX Runtime ABI (API 27 so với API 17).
- `rustpotter-probe/`: probe personalized rustpotter 3.0.2; pin `half` 2.3.1 để khớp
  candle 0.2.2 của upstream.
- `work/`: model tải về, corpus, build target và report cục bộ; bị Git bỏ qua.

Hai probe là công cụ benchmark, không phải dependency runtime và không chứng minh đạt production.
Kết quả chuẩn và acceptance gate nằm tại `docs/05-chat-luong/wake-benchmark.md`.
