# Models — tải out-of-band (weights bị gitignore)

Tất cả `*.onnx`, `*.gguf` trong repo đều bị gitignore; clone mới phải tự tải về đúng chỗ.

## Cách nhanh: hai lệnh

```bash
npm run doctor                          # thiếu gì → tính năng nào đang TẮT
npm run setup:models                    # tải profile minimal (~5,6 GB)
npm run setup:models -- --profile full  # thêm Parakeet, vision, VieNeu, wake-word (~11,6 GB)
```

`scripts/models.mjs` tải có **resume** (rớt mạng thì chạy lại, không mất từ đầu) và
**retry 3 lần**. Thư mục LLM lấy từ `data/liva-config.json → ai.localModelsDir`; máy
không có ổ đĩa đó thì `--llm-dir <đường dẫn>`.

`npm run doctor` cũng báo `models/parakeet_vi.onnx.data` nếu còn sót từ bản cũ. Manifest
hiện dùng `models/model.onnx_data`; sau khi xác nhận Parakeet nạp được, có thể xoá file
đời cũ thủ công để thu hồi khoảng 2,43 GB.

Nhóm `wake_liva_*.onnx` vẫn không có nguồn công khai vì là model tự train; `doctor`
vẫn báo thiếu và chỉ chỗ chuẩn bị. Parakeet-vi được tải trong profile `full` từ
revision cố định, có kiểm SHA-256. Bảng dưới là nguồn gốc đầy đủ của từng file.

| File | Nguồn tải | Ghi chú |
|------|-----------|---------|
| `silero_vad_v6.onnx` | <https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx> | Silero VAD v6.2 (~2.3MB). Resolver ưu tiên file này; thiếu thì fallback `nemotron-asr/silero_vad.onnx` (v5, có sẵn trong nested repo). Override: `LIVA_VAD_MODEL_PATH`. |
| `piper/vi_VN-vais1000-medium.onnx` | <https://huggingface.co/rhasspy/piper-voices/tree/main/vi/vi_VN/vais1000/medium> | Giọng TTS tiếng Việt (63MB). File `.onnx.json` đi kèm ĐÃ commit. |
| `piper/en_US-lessac-medium.onnx` | <https://huggingface.co/rhasspy/piper-voices/tree/main/en/en_US/lessac/medium> | Giọng TTS tiếng Anh (63MB). `.onnx.json` đã commit. |
| `nemotron-asr/` | nested git repo (LFS) — clone riêng | STT streaming, wake-word và fallback (RNN-T). KHÔNG đụng vào bằng git từ repo ngoài (luôn hiện "modified content" — kệ nó). |
| `parakeet_vi.onnx` + `model.onnx_data` + `parakeet_vi_vocab.json` | bản ONNX FP32 của [OpenVoiceOS](https://huggingface.co/OpenVoiceOS/nvidia-parakeet-ctc-0.6b-vietnamese-onnx), chuyển từ [NVIDIA Parakeet CTC 0.6B Vietnamese](https://huggingface.co/nvidia/parakeet-ctc-0.6b-Vietnamese), khóa tại revision `240d82cc243f7cf47d100b293c7dff96e65a04c2` | **STT tiếng Việt mặc định** cho whole-utterance sau VAD-end; Nemotron vẫn xử lý wake-word/streaming và là fallback. LIVA tự đo 100 câu FLEURS-vi: **WER 12,00%** so với Nemotron **15,01%**, p50 **667 ms** so với **3.772 ms**. Graph 953.822 byte tham chiếu external weights `model.onnx_data` 2.475.958.276 byte; phải giữ cùng thư mục. CTC greedy, vocab BPE 1.024 token. License NVIDIA Open Model. |
| `kokoro-v1.0.onnx` | (tùy chọn, chưa dùng mặc định) | TTS EN premium — vắng mặt thì TTS tự route sang Piper. |
| `vieneu/` (nhiều file) | HF <https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo> (thư mục `onnx_update/` + root) · codec <https://huggingface.co/OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano-ONNX> · phonemizer `sea_g2p.bin` từ <https://github.com/pnnbao97/sea-g2p> (`python/sea_g2p/sea_g2p.bin`) · `voices_v3_turbo.json` từ <https://github.com/pnnbao97/VieNeu-TTS> (`src/vieneu/assets/`) | **TTS tiếng Việt "giọng đẹp" premium — tự hồi quy (LLM Qwen3 + codec MOSS 48kHz), clone-giọng preset**. Toàn bộ **Apache-2.0** — **TRÁNH** repo `VieNeu-TTS-0.3B-q4-gguf` (CC-BY-NC). Files bắt buộc: `vieneu_prefill.onnx`, `vieneu_decode_step.onnx`, `vieneu_acoustic_cached.onnx`, `vieneu_backbone_shared.data` (~396MB), `vieneu_v3_heads.npz` (~50MB), `config.json`, `tokenizer.json`, `voices_v3_turbo.json`, `sea_g2p.bin` (~50MB), `moss_audio_tokenizer_decode_full.onnx` (+ `_decode_shared.data` ~42MB), `codec_browser_onnx_meta.json`. **✅ ĐÃ tích hợp** (`tts/vieneu/`, port thuần Rust `ort` của engine tham chiếu `onnx_runtime_lite.py`) — **opt-in `LIVA_TTS_VIENEU=1`**, đứng trên Piper (Piper vẫn always-on/khi tải nặng). Env: `LIVA_VIENEU_MODEL_DIR` (mặc định `models/vieneu`), `LIVA_VIENEU_VOICE` (mặc định `default_voice` = "Phạm Tuyên"), `LIVA_VIENEU_SEED`, `LIVA_VIENEU_THREADS` (mặc định 4). **Lưu ý:** tự hồi quy nên **RTF ~1.75 trên CPU** (chậm hơn real-time — dùng như tier chất lượng, chưa cắt được barge-in giữa chunk); cần crate `tokenizers` ≥0.21 (tokenizer.json dùng merges dạng mảng + `ignore_merges`). |
| `embedding/model.onnx` + `embedding/tokenizer.json` | <https://huggingface.co/intfloat/multilingual-e5-small> (export ONNX) | **Model embedding cho bộ nhớ dài hạn — 384 chiều**, tách khỏi model chat có chủ đích. Lý do: `vec_idx` khai cứng `int8[384]`, còn embedding của model chat phụ thuộc model đang nạp (Qwen3-VL-2B → 2048 chiều, sqlite-vec báo `Dimension mismatch`); quan trọng hơn, dùng model chat nghĩa là **đổi model chat là mất sạch bộ nhớ cũ**. Họ E5 cần tiền tố `query: ` / `passage: ` — `EmbeddingEngine::embed_query` / `embed_passage` đã tự thêm. Override thư mục: `LIVA_EMBEDDING_MODEL_DIR`. **Thiếu file → không lỗi chí mạng**, chỉ là không có RAG. ⚠️ Chưa kiểm chứng end-to-end với model thật (chưa tải về máy dev). |
| `gtcrn_simple.onnx` | <https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/gtcrn_simple.onnx> | Denoise trước VAD/STT (523KB, MIT). **BẬT mặc định** — tắt bằng `LIVA_DENOISE_ENABLED=0`. Thiếu file → tự chạy không khử ồn (không lỗi). |
| `smart_turn_v3.2_cpu.onnx` | <https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/main/smart-turn-v3.2-cpu.onnx> | Semantic end-of-turn SHADOW MODE (8.68MB, BSD-2). Tắt mặc định — `LIVA_TURN_SHADOW_ENABLED=1`. |
| `wakeword_melspec.onnx`, `wakeword_embedding.onnx` | <https://github.com/livekit/rust-sdks/tree/main/livekit-wakeword/onnx> | Model dùng chung cho pipeline wake-word tự train (Apache-2.0). Bắt buộc nếu dùng `LIVA_WAKE_MODE=trained_model`. |
| `wake_fixtures/{hey_livekit.onnx,positive.wav,negative.wav}` | <https://github.com/livekit/rust-sdks/tree/main/livekit-wakeword/tests/fixtures> | Fixture tham chiếu để test `wake_model.rs` (không phải model của LIVA — dùng để đối chiếu pipeline reimplement đúng). |
| `wake_liva_en_v2.onnx` | tự train (2026-07-31) | **Artifact mặc định “Hey Liva”.** Conv-attention, SHA-256 `7487fcb480ce05a6ba02901ee48071caed73b593dfed5d9b55202ae1001c4780`, 956.143 byte. Eval tổng hợp 25,88h tại ngưỡng tối ưu **0,58**: recall **91,82%**, FPPH **0,0773**. Core kiểm hash manifest trước khi nạp artifact mặc định. Chưa thay thế gate giọng thật trên mic mục tiêu. |
| `wake_liva_en.onnx` | tự train (2026-07-04, xem báo cáo) | Artifact rollback. Classifier "LIVA"/"hey liva" giọng Anh (Piper VITS). Eval trên 17.85h validation: **recall 98.8%, FPPH 1.74 @ threshold 0.5**; ngưỡng tối ưu theo eval **0.77** (recall 98.15%, FPPH 0.168). Chỉ dùng bằng override `LIVA_WAKE_MODEL_PATHS`. |
| `wake_liva_vi.onnx` | tự train 2026-07-05 (VoxCPM) | Classifier giọng Việt — **CHẤT LƯỢNG KÉM**: eval 17.85h chỉ recall 91.5%/**FPPH 19.4 @0.5**; ngưỡng tối ưu 0.91 → recall tụt còn 63.2%. Nguyên nhân: embedding lõi English-centric + giọng VoxCPM kém đa dạng. **Chưa đủ tốt dùng một mình** — với tiếng Việt nên dùng `LIVA_WAKE_MODE=asr_prefix` (STT Nemotron nhận "liva" trong câu, đáng tin hơn). Giữ file để cải thiện sau (train lại bằng giọng thật / TTS tốt hơn như VieNeu-TTS). |

LLM GGUF đặt theo `LIVA_LLM_MODEL_DIR` (xem `.env.example`), không nằm trong thư mục này.

**Lưu ý quan trọng (2026-07-04):** KHÔNG thêm crate Rust `livekit-wakeword` vào Cargo.toml — trên Windows x86_64 nó kéo theo `ort` feature `alternative-backend` (+ backend `ort-tract` thuần Rust), và Cargo hợp nhất feature toàn dependency graph nên sẽ làm HỎNG NGẦM mọi `ort::Session` khác trong cùng tiến trình (VAD/GTCRN/Smart Turn/STT/TTS) với lỗi "attempted to use `ort` APIs before initializing a backend". `wake_model.rs` đã tự triển khai lại pipeline mel→embedding→classify bằng `ort` gốc của dự án để né xung đột này — xem comment đầu file.
