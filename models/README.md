# Models — tải out-of-band (weights bị gitignore)

Tất cả `*.onnx`, `*.gguf` trong repo đều bị gitignore; clone mới phải tự tải về đúng chỗ.

| File | Nguồn tải | Ghi chú |
|------|-----------|---------|
| `silero_vad_v6.onnx` | <https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx> | Silero VAD v6.2 (~2.3MB). Resolver ưu tiên file này; thiếu thì fallback `nemotron-asr/silero_vad.onnx` (v5, có sẵn trong nested repo). Override: `LIVA_VAD_MODEL_PATH`. |
| `piper/vi_VN-vais1000-medium.onnx` | <https://huggingface.co/rhasspy/piper-voices/tree/main/vi/vi_VN/vais1000/medium> | Giọng TTS tiếng Việt (63MB). File `.onnx.json` đi kèm ĐÃ commit. |
| `piper/en_US-lessac-medium.onnx` | <https://huggingface.co/rhasspy/piper-voices/tree/main/en/en_US/lessac/medium> | Giọng TTS tiếng Anh (63MB). `.onnx.json` đã commit. |
| `nemotron-asr/` | nested git repo (LFS) — clone riêng | STT chính. KHÔNG đụng vào bằng git từ repo ngoài (luôn hiện "modified content" — kệ nó). |
| `kokoro-v1.0.onnx` | (tùy chọn, chưa dùng mặc định) | TTS EN premium — vắng mặt thì TTS tự route sang Piper. |
| `gtcrn_simple.onnx` | <https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/gtcrn_simple.onnx> | Denoise trước VAD/STT (523KB, MIT). Tắt mặc định — `LIVA_DENOISE_ENABLED=1`. |
| `smart_turn_v3.2_cpu.onnx` | <https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/main/smart-turn-v3.2-cpu.onnx> | Semantic end-of-turn SHADOW MODE (8.68MB, BSD-2). Tắt mặc định — `LIVA_TURN_SHADOW_ENABLED=1`. |
| `wakeword_melspec.onnx`, `wakeword_embedding.onnx` | <https://github.com/livekit/rust-sdks/tree/main/livekit-wakeword/onnx> | Model dùng chung cho pipeline wake-word tự train (Apache-2.0). Bắt buộc nếu dùng `LIVA_WAKE_MODE=trained_model`. |
| `wake_fixtures/{hey_livekit.onnx,positive.wav,negative.wav}` | <https://github.com/livekit/rust-sdks/tree/main/livekit-wakeword/tests/fixtures> | Fixture tham chiếu để test `wake_model.rs` (không phải model của LIVA — dùng để đối chiếu pipeline reimplement đúng). |
| `wake_liva_en.onnx` | tự train (2026-07-04, xem báo cáo) | Classifier "LIVA"/"hey liva" giọng Anh (Piper VITS). Eval trên 17.85h validation: **recall 98.8%, FPPH 1.74 @ threshold 0.5**; ngưỡng tối ưu theo eval **0.77** (recall 98.15%, FPPH 0.168) — set `LIVA_WAKE_THRESHOLD=0.77` khi dùng model này. |
| `wake_liva_vi.onnx` | tự train (đang chạy, xem báo cáo) | Classifier giọng Việt (VoxCPM) — chưa xong, sẽ cập nhật số đo khi có. |

LLM GGUF đặt theo `LIVA_LLM_MODEL_DIR` (xem `.env.example`), không nằm trong thư mục này.

**Lưu ý quan trọng (2026-07-04):** KHÔNG thêm crate Rust `livekit-wakeword` vào Cargo.toml — trên Windows x86_64 nó kéo theo `ort` feature `alternative-backend` (+ backend `ort-tract` thuần Rust), và Cargo hợp nhất feature toàn dependency graph nên sẽ làm HỎNG NGẦM mọi `ort::Session` khác trong cùng tiến trình (VAD/GTCRN/Smart Turn/STT/TTS) với lỗi "attempted to use `ort` APIs before initializing a backend". `wake_model.rs` đã tự triển khai lại pipeline mel→embedding→classify bằng `ort` gốc của dự án để né xung đột này — xem comment đầu file.
