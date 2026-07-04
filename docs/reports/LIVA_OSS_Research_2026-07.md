# Khảo sát OSS & nghiên cứu mới nhất cho LIVA (2025 → giữa 2026)

> Ngày: 2026-07-04 · Phương pháp: deep-research workflow 109 agent — 6 hướng tìm kiếm song song,
> 26 nguồn chính (GitHub / HuggingFace / arXiv / leaderboard / NVIDIA docs), 130 claim trích xuất,
> 25 claim then chốt được **kiểm chứng đối kháng 3 phiếu độc lập**: 22 xác nhận, 0 bị bác bỏ,
> 3 chưa kiểm chứng xong (đứt phiên giữa chừng — KHÔNG phải bị bác bỏ).
> Ràng buộc: 100% offline trên Windows, ưu tiên license permissive (MIT/Apache), ưu tiên ONNX/GGUF
> gọi thẳng từ Rust (`ort` / `llama-cpp-2`), không Python daemon trên đường always-on.

Ký hiệu: ✅ = claim đã kiểm chứng 3 phiếu · ⚠️ = rào cản license/nền tảng · 🔬 = nguồn chính nhưng chưa qua vòng verify

---

## Nhật ký triển khai (2026-07-04, đợt 2 — sau khi user duyệt "xử lý lần lượt")

| # | Hạng mục | Trạng thái | Chi tiết |
|---|----------|-----------|----------|
| 1 | **GTCRN denoise** | ✅ Đã ship, có test | `webrtc/denoise.rs` mới — STFT 512/256 sqrt-Hann, contract xác minh bằng `onnx_probe` + đối chiếu source gốc `gtcrn_stream.py`; model `models/gtcrn_simple.onnx` (523KB). Đo thật trên file mẫu nhiễu của chính GTCRN (`gtcrn_probe`): RMS 0.0403→0.0256 (giảm nhiễu rõ), RTF 0.0544 CPU (~18x realtime). Tắt mặc định, bật bằng `LIVA_DENOISE_ENABLED=1`. |
| 2 | **Wake word "LIVA" (livekit-wakeword)** | ✅ Giọng Anh XONG hoàn chỉnh; giọng Việt đang train (GPU) | **Phần Rust: ✅ đã ship, đã kiểm chứng.** Phát hiện quan trọng: crate Rust `livekit-wakeword` KHÔNG dùng được trực tiếp — trên Windows x86_64 nó bật feature `alternative-backend` của `ort` (dùng backend `ort-tract` thuần Rust), Cargo hợp nhất feature toàn dependency graph nên làm HỎNG NGẦM mọi `ort::Session` khác (VAD/GTCRN/Smart Turn/STT/TTS) với lỗi "attempted to use `ort` APIs before initializing a backend" — bắt được qua `cargo test`, không phải giả thuyết suông. Đã gỡ crate đó, **tự viết lại pipeline mel→embedding→classify bằng `ort` gốc của dự án** (`wake_model.rs`, 2 model phụ trợ Apache-2.0 tải từ chính repo). **Kiểm chứng bằng chính fixture tham chiếu của livekit-wakeword** (`hey_livekit.onnx` + positive/negative.wav): điểm số 0.9997 (positive, cần ≥0.5) và 0.0009 (negative, cần <0.5) — khớp hoàn toàn kỳ vọng. `WakeMode::TrainedModel` mới đã nối vào `main.rs` (quét liên tục, độc lập VAD), tắt mặc định. **Model giọng Anh "liva_en" ĐÃ TRAIN XONG** (`models/wake_liva_en.onnx`, pipeline Piper VITS trên CPU, ~2.5 tiếng generate→augment→train→export): eval trên 17.85 giờ audio validation (2000 positive + 32124 negative) → **recall 98.8%, FPPH 1.74 @ threshold 0.5**; ngưỡng tối ưu theo chính bước eval là **0.77** → recall 98.15%, FPPH chỉ còn 0.168 (~1 lần báo sai/6 giờ). Đối chiếu thêm bằng 16 clip augmented thật (8 positive + 8 negative, ngoài tập mà eval đã dùng) qua chính pipeline Rust: positive 0.846–0.961, negative 0.004–0.015 — tách bạch rõ ràng, không chồng lấn. **Model giọng Việt "liva_vi"**: pipeline VoxCPM ban đầu chạy CPU cực chậm (~29 clip/5 phút) → phát hiện `pip install torch` mặc định trên Windows chỉ ra bản CPU-only; cài lại `torch+cu128` trong venv riêng, xác nhận RTX 5060 Ti (compute capability 12.0/Blackwell) nhận đúng qua phép nhân ma trận thật trên GPU — tốc độ sinh dữ liệu tăng ~4-5 lần. Đang train, chưa xong tại thời điểm ghi chú này. |
| 3 | **VieNeu-TTS PoC** | 🔄 Đang chạy synth test | venv riêng + `pip install vieneu` (torch-free, chỉ onnxruntime) — xác nhận đúng như quảng cáo. **Lưu ý license quan trọng phát hiện thêm**: chỉ dùng `Vieneu()` mặc định (tải v3-Turbo, Apache-2.0 xác nhận) — **TRÁNH** repo `VieNeu-TTS-0.3B-q4-gguf` vì đó là **CC-BY-NC-4.0** (khác với code Apache!). |
| 4 | **Smart Turn v3.2 shadow-mode** | ✅ Đã ship, có test | `webrtc/turn_shadow.rs` mới — feature extraction Whisper log-mel (400/160/80 mel, Slaney) tự viết bằng Rust, port chính xác từ source `pipecat-ai/pipecat`; model `models/smart_turn_v3.2_cpu.onnx` (8.68MB). Chỉ LOG, không gate quyết định thật. Tắt mặc định, bật bằng `LIVA_TURN_SHADOW_ENABLED=1`. |
| 5 | **Parakeet-CTC-0.6B ONNX export** | ⛔ Hoãn (watch) | Nghiên cứu xác nhận: NeMo không cài native trên Windows (cần WSL2/Docker theo chính maintainer NeMo); tải 2.6GB; export có thể ra bản KHÔNG streaming — không khớp kiến trúc hiện tại. Để dành khi có WSL. |
| 6 | **Sonora AEC3 self-echo** | ✅ Đã ship, có test | `webrtc/aec.rs` mới — API thật xác minh qua `karaoke.rs` example: `AudioProcessing::process_render_f32`/`process_capture_f32`, frame 10ms/160 mẫu @16kHz. Triệt tiếng LIVA tự nói vọng lại mic khi barge-in (chưa xử lý được tiếng game qua loa — cần WASAPI loopback, để sau). Tắt mặc định, bật bằng `LIVA_AEC_ENABLED=1`. |

**Toàn bộ 3 module mới (denoise/turn_shadow/aec) đều mặc định TẮT** — không đổi hành vi hiện có; build đầy đủ (root + liva-desktop), 118/118 lib test + verify_duplex + verify_integrations đều xanh sau khi wire vào `main.rs`/`pipeline.rs`/`AppState`. GitNexus `detect_changes` báo **risk HIGH** (18 file, 22 symbol, 12 luồng ảnh hưởng) — do chạm `AppState`/`async_main`/`pipeline.rs` là các điểm trung tâm nhiều luồng hội tụ, không phải do lỗi; đã xác nhận qua toàn bộ bộ test trên.

---

## 1. TTS tiếng Việt cao cấp (tier trên Piper)

### ✅ VieNeu-TTS — ứng viên số 1 "premium local voice"
Nguồn: <https://github.com/pnnbao97/VieNeu-TTS>

- **Apache 2.0** ✅ (phiếu 2-1) — thương mại được, không dính CC-BY-NC.
- On-device, **realtime trên CPU, 24 kHz** (v3 Turbo early-access: 48 kHz qua codec MOSS-Audio-Tokenizer-Nano) ✅.
- **Clone giọng tức thì từ 3–5 giây audio mẫu** ✅.
- **Song ngữ vi-en code-switching** qua phonemizer `sea-g2p`, train trên **10.000+ giờ** dữ liệu song ngữ ✅ — khớp đúng yêu cầu vi+en của LIVA.
- Có đường **GGUF + ONNX cho CPU** (LMDeploy cho GPU) + bản nhỏ **0.3B**; wheels llama-cpp-python phát hành 09/01/2026 ✅ → khả thi tích hợp từ Rust (`ort` hoặc họ llama.cpp) không cần Python daemon.
- **Điểm trừ:** chất lượng tự công bố — README không có MOS/benchmark độc lập ✅.

**Đề xuất:** PoC đo thật (chất lượng tai nghe + RTF + VRAM) làm tier "giọng đẹp" phía trên Piper. Piper vẫn giữ vai trò always-on/game-mode (RTF 0.05 CPU đã đo).

### ⚠️ F5-TTS-Vietnamese-ViVoice (hynt) — chất lượng hứa hẹn nhưng cấm thương mại
Nguồn: <https://huggingface.co/hynt/F5-TTS-Vietnamese-ViVoice>

- Fine-tune F5-TTS trên ~**1000 giờ** tiếng Việt (Vi-Voice + VLSP 2021/2022/2023, lọc Demucs, transcript được STT kiểm) ✅. Train trên 1 GPU RTX 3090 trong ~1,5 tháng 🔬.
- **CC-BY-NC-SA-4.0 — chỉ nghiên cứu phi thương mại** ✅ → không dùng cho sản phẩm. Không có MOS/RTF công bố 🔬.
- **Kết luận:** watch — chỉ tham khảo chất lượng, không tích hợp.

### ⚠️ VietTTS (dangvansam) — loại
Nguồn: <https://github.com/dangvansam/viet-tts>

- Code Apache 2.0 nhưng **weights CC BY-NC** ✅ + **Linux-only, Windows "coming soon"** ✅ → blocker kép.
- Python/PyTorch, không ONNX 🔬; release cuối 12/2024, không bằng chứng chất lượng 🔬.

### Kokoro
Không tìm thấy bằng chứng bản Kokoro nào hỗ trợ tiếng Việt trong khảo sát → giữ nguyên vai trò EN-premium tùy chọn (model vẫn chưa có trên đĩa).

---

## 2. ASR song ngữ vi+en

### ✅ NVIDIA Parakeet-CTC-0.6B Unified Vietnamese–English CS — nhảy vọt độ chính xác
Nguồn: <https://huggingface.co/nvidia/parakeet-ctc-0.6b-Vietnamese> (tạo 15/01/2026, sửa 07/02/2026) ✅

- FastConformer-**CTC** 600M, unified **vi+en code-switching** ✅, train >2.000 giờ tiếng Việt công khai 🔬.
- **WER FLEURS vi 5.15** (in-domain) và **9.30 trung bình blind test** (Gigaspeech2 11.23, VLSP'21-T2 8.99, ViMD 11.02, VIVOS 5.96) ✅ — **~3× tốt hơn** con số 14.45 của Nemotron streaming hiện tại.
- **Rào cản:** license **NVIDIA Open Model** (không phải Apache, nhưng không phải NC) 🔬; model card chỉ tài liệu hóa NeMo 2.6+/PyTorch/Linux, **không nói ONNX export hay streaming** 🔬. Bản NIM Docker `parakeet-ctc-0.6b-vi` có streaming chunk-by-chunk ✅ nhưng là GPU microservice + vi-only 🔬 → không hợp đường always-on.
- **Đề xuất:** thử nghiệm export ONNX bằng NeMo (FastConformer-CTC export được về nguyên tắc), chạy **chunked-CTC làm pass "độ chính xác cao"** (re-score cuối câu) song song Nemotron streaming. Lưu ý WER 14.45 của Nemotron đo TRƯỚC khi ta sửa bug tokenizer decode — chênh lệch thực tế cần đo lại.

### ✅ sherpa-onnx (k2-fsa) — hộp công cụ Rust-ready
Nguồn: <https://github.com/k2-fsa/sherpa-onnx>

- Full stack local: streaming + non-streaming ASR, TTS, VAD, KWS, speech enhancement, diarization — chạy onnxruntime hoàn toàn offline ✅. **Rust bindings chính thức** ✅, Windows x86/x64/arm64, Apache-2.0 🔬.
- Model zoo: Zipformer streaming, Parakeet, SenseVoice, Moonshine, Whisper… 🔬 (README không nêu đích danh tiếng Việt ✅ — coverage tùy model).
- **Vai trò cho LIVA:** nguồn model ONNX đóng gói sẵn (đặc biệt **GTCRN** — xem mục 5) + tham chiếu cách chạy streaming ASR/KWS từ Rust.

---

## 3. Wake word "LIVA" thật (thay asr_prefix gate v1)

### ✅ livekit-wakeword — lựa chọn chốt
Nguồn: <https://github.com/livekit/livekit-wakeword> · blog: <https://livekit.com/blog/livekit-wakeword>

- Gốc openWakeWord, **train 100% từ dữ liệu TTS tổng hợp** (Piper backend, ~10k mẫu/lớp, không cần thu âm người thật), toàn pipeline generate→augment→train→export chạy **1 lệnh YAML** ✅.
- Đầu phân loại conv-attention: **~100× ít false-positive/giờ so với openWakeWord (0.08 vs 8.50), recall 86.1% vs 68.6%** (benchmark "hey livekit", 15k clip dương + ~25h âm) ✅ — số liệu vendor tự công bố.
- **Rust crate native** (`livekit-wakeword`): mel-spectrogram + speech-embedding compile sẵn trong binary, chỉ classifier head load ONNX runtime ✅ → khớp hoàn hảo stack `ort`, không Python daemon.
- Apache-2.0, pre-release mới nhất 28/02/2026 🔬. **Tiếng Việt nằm trong 30 ngôn ngữ** sinh dữ liệu qua backend VoxCPM, nhưng dự án cảnh báo model đa ngữ hiện kém chính xác hơn EN ✅.
- 3 claim từ blog chưa verify xong do đứt phiên (ONNX drop-in tương thích openWakeWord; bộ số blog; train thuần TTS) — nội dung trùng phần lớn với claim GitHub đã xác nhận 3-0.

**Kế hoạch train "LIVA":** sinh mẫu bằng cả Piper `vi_VN-vais1000` (phát âm "li-va" kiểu Việt) + VoxCPM vi + giọng EN; augment nhiễu/reverb; train 1 GPU (RTX 5060 Ti thừa sức — piper-sample-generator đo ~100 mẫu/s trên 2080Ti 🔬); tích hợp qua crate Rust, giữ `LIVA_WAKE_MODE=asr_prefix` làm fallback.

### Các lựa chọn còn lại
- **openWakeWord** 🔬: pre-trained models **CC-BY-NC-SA** ⚠️ (backbone embedding Google Apache-2.0 — model TỰ train thì né được NC); stock chỉ EN; release cuối 02/2024 → đã bị livekit-wakeword vượt.
- **microWakeWord** 🔬: TFLite-Micro (không ONNX), tự nhận "training rất khó, cho người dùng nâng cao" → watch.
- **piper-sample-generator** 🔬 (MIT, v3.1.0 09/2025): bộ sinh dữ liệu chuẩn; generator 904 giọng chỉ EN → mẫu vi phải sinh từ Piper vi_VN.
- Paper Google (arXiv 2505.22995) 🔬: augment bằng "từ dễ nhầm" do LLM sinh + TTS đọc — cải thiện c-AUC 11.3%; stack đóng, chỉ lấy ý tưởng (thêm negative kiểu "li ba", "đi va", "lê na"… khi train).

---

## 4. Turn-taking ngữ nghĩa / barge-in

### 🔬 Smart Turn v3 / v3.2 (pipecat) — semantic end-of-turn chạy CPU
Nguồn: <https://github.com/pipecat-ai/smart-turn> · <https://www.daily.co/blog/announcing-smart-turn-v3-with-cpu-inference-in-just-12ms/>

- 8M params (Whisper-Tiny encoder + linear head), **ONNX int8 8MB**, suy luận **~12ms trên CPU hiện đại**; BSD-2-clause, mở toàn bộ (weights + data + training scripts).
- **23 ngôn ngữ gồm cả VI + EN**; nhưng **tiếng Việt là ngôn ngữ yếu nhất: 81.27%** accuracy (en 94.31%, top 97.1%) — có data mở nên fine-tune vi được.
- Thiết kế **dùng kèm VAD** (không thay) — khớp kiến trúc LIVA: VAD im lặng ~200-300ms → hỏi Smart Turn "câu đã hết chưa?" → hết thì chốt sớm, chưa thì đợi thêm.
- **Đề xuất:** tích hợp **shadow-mode** (chạy song song, chỉ log quyết định) để đo chất lượng vi thực tế trước khi bật thật.

### 🔬 TEN VAD — nhanh và chính xác hơn Silero, nhưng license cần soi
Nguồn: <https://github.com/TEN-framework/ten-vad>

- Tự công bố chính xác hơn WebRTC VAD + Silero; phát hiện chuyển speech→non-speech **nhanh hơn Silero vài trăm ms**; ONNX mở từ 06-07/2025; lib C prebuilt Windows x64 ~500KB, RTF 0.0086–0.057.
- ⚠️ "Apache 2.0 **kèm điều kiện bổ sung**" + code LPCNet BSD — phải đọc kỹ license trước khi ship.

### ✅ (đã làm) Silero VAD v6.2 — nâng cấp drop-in
Nguồn: <https://github.com/snakers4/silero-vad/releases>

- v6.0 (26/08/2025): **−16% lỗi trên dữ liệu noisy đời thực, −11% multi-domain** — đúng kịch bản mic khi chơi game; v6.2 (06/11/2025) ổn định hơn trên giọng lạ/edge-case; giữ nguyên interface ONNX v5 (input/state/sr → output/stateN) 🔬.
- **Trạng thái: ĐÃ NÂNG CẤP trong LIVA ngày 2026-07-04** — model mới `models/silero_vad_v6.onnx`, resolver ưu tiên v6 + env `LIVA_VAD_MODEL_PATH`, fallback model cũ trong thư mục nemotron. Đã xác nhận failure mode còn lại: nhạc có nhạc cụ giống giọng người vẫn có thể false-positive 🔬 → về lâu dài kết hợp GTCRN (mục 5).
- Full-duplex thật (Moshi, Freeze-Omni, parrot-style): research-only với stack Rust/ONNX hiện tại → watch.

---

## 5. Chống ồn / khử echo mic khi chơi game

### 🔬 GTCRN — denoise gần như miễn phí CPU, ship-now
Nguồn: <https://github.com/Xiaobin-Rong/gtcrn> (ICASSP 2024, MIT, còn maintain 01/2026)

- **48.2K params / 33 MMACs-giây** — nhỏ hơn DeepFilterNet vài bậc; bản **streaming causal RTF 0.07 trên i5-12400** (~7% một core).
- Chất lượng: PESQ 2.87 VCTK-DEMAND (DeepFilterNet 2.81, RNNoise 2.29), DNSMOS OVRL 2.70 vs RNNoise 2.53 — **thắng hoặc hòa các model to hơn nhiều**.
- **Đã tích hợp trong sherpa-onnx từ 10/03/2025** → có sẵn ONNX streaming + Rust bindings tham chiếu.
- KHÔNG làm AEC (chỉ noise suppression) → vẫn cần AEC riêng cho echo loa→mic.
- **Đề xuất:** chèn stage GTCRN trước VAD/STT (bật qua env, mặc định off tới khi đo xong) — việc ~0,5–1 ngày gồm STFT streaming.

### 🔬 Sonora — AEC3 thuần Rust, mới ra lò
Nguồn: <https://github.com/dignifiedquire/sonora>

- **Pure-Rust port WebRTC M145 audio processing**: AEC3 (khử echo + delay estimation) + Wiener NS + AGC2; BSD-3-Clause; **Windows x86_64 có CI + SIMD SSE2/AVX2**; 10ms frame xử lý trong ~4–13µs (M4 Max) — chi phí ~0,1% realtime.
- Rất mới (v0.1.0 — 11/02/2026) nhưng validated bằng 2400+ test của chính WebRTC C++.
- **Use case LIVA:** khử **tiếng LIVA tự nói** khỏi mic (đã có sẵn reference = chính audio TTS đang phát) → barge-in sạch; xa hơn: capture WASAPI loopback làm reference khử cả game audio.
- So sánh: crate `webrtc-audio-processing` (tonarino) v2.1 cũng có AEC3 nhưng **không có Windows CI, build cần meson/ninja** 🔬 → Sonora hợp LIVA hơn. DeepFilterNet chất lượng tốt, MIT/Apache, nhưng **dormant** (release cuối 08/2023, crate trên crates.io từ 2022) 🔬 → watch.

---

## 6. LLM local tiếng Việt + tối ưu runtime 16GB (chia sẻ với game)

### Bằng chứng benchmark tiếng Việt
- **VMLU** (<https://vmlu.ai/leaderboard>, ZaloAI + JAIST) 🔬: mảng from-scratch **đã cũ** — mới nhất QwQ-32B (03/2025), CHƯA có Qwen3/Gemma 3/Phi-4/Llama 4; tốt nhất ≤12B trên bảng: **gemma-2-9b-it 59.04**, Qwen2.5-7B-Instruct 57.51. Mảng fine-tuned cập nhật tới 06/2026 nhưng nhiều model đóng (VAI-LLM, KiLM…).
- **SeaExam/SeaBench** (arXiv 2502.06298, đề thi thật khu vực, không phải dịch máy) 🔬: **Gemma-2-9b-it đứng đầu lớp 7–9B tiếng Việt: 68.4%** — hơn cả model chuyên SEA (SeaLLMs-v3-7B-Chat 64.4%) và Llama-3.1-8B (57.1%). Kết luận: **họ Gemma mạnh tiếng Việt một cách nhất quán** → LIVA đang dùng Gemma-class GGUF là hướng đúng, đời mới hơn của cùng họ là nâng cấp tự nhiên.
- **Qwen3** (<https://github.com/QwenLM/Qwen3>) 🔬: Apache 2.0, dense 0.6B→32B, llama.cpp hỗ trợ chính thức (GGUF), định vị agentic/tool-calling; bản **0.6B là draft model lý tưởng cho speculative decoding**. Đáng test A/B tiếng Việt với Gemma.
- **SEA-LION v4/v4.5** (<https://github.com/aisingapore/sealion>) 🔬: CPT trên Gemma-3-27B, có GGUF (v3 có bản 9B vừa budget hơn); ⚠️ license "MIT hết mức có thể" nhưng **kế thừa điều khoản Gemma/Llama theo từng bản** — check per-model.

### Tối ưu llama.cpp cho cảnh "vừa chơi game vừa chạy LIVA" (nguồn forum/blog 🔬)
- **KV cache Q8_0: LIVA ĐÃ BẬT SẴN** (`llm/engine.rs` — type_k = type_v = Q8_0) ✅ trong code — không cần làm gì thêm.
- MoE active-param nhỏ **nhanh ~7–10×** dense cùng chất lượng trên card 16GB (Qwen3.6-35B-A3B IQ3_XXS ~146-149 t/s vs dense 27B ~20 t/s khi tràn VRAM).
- Chỉnh tay `-ngl` (số layer offload) có thể **~2× throughput** so với auto-offload (auto chừa ~1GB headroom).
- **MTP speculative decoding** trong llama.cpp mới: +67% throughput dense 27B — theo dõi khi `llama-cpp-2` expose API.
- **TurboQuant** (arXiv 2504.19874, KV <3-bit): CHƯA merge upstream llama.cpp (03/2026 mới có fork) → watch.

---

## Tổng kết ưu tiên

### Đã làm ngay (2026-07-04)
| # | Việc | Trạng thái |
|---|------|-----------|
| 1 | **Silero VAD v6.2** — model mới + resolver + `LIVA_VAD_MODEL_PATH` | ✅ code xong, verify_duplex kiểm chứng |
| 2 | KV cache Q8_0 | ✅ phát hiện đã bật sẵn từ trước — không cần làm |

### Lộ trình đề xuất (xếp theo giá trị ÷ công sức)
| # | Việc | License | Công sức | Ghi chú |
|---|------|---------|----------|---------|
| 1 | **GTCRN denoise** trước VAD/STT | MIT | ~0,5–1 ngày | ONNX sẵn trong sherpa-onnx; CPU ~7% 1 core |
| 2 | **Wake word "LIVA"** bằng livekit-wakeword | Apache | ~1–2 ngày | Train offline 1 GPU từ TTS synthetic; Rust crate sẵn; thay asr_prefix v1 |
| 3 | **VieNeu-TTS PoC** — tier giọng đẹp trên Piper | Apache | ~1 ngày PoC | Đo tai nghe + RTF + VRAM trước khi tích hợp thật |
| 4 | **Smart Turn v3 shadow-mode** — end-of-turn ngữ nghĩa | BSD-2 | ~1 ngày | vi mới 81% → chỉ log, chưa quyết định; fine-tune vi được |
| 5 | **Parakeet-CTC-0.6B vi-en** export ONNX → re-score pass | NVIDIA OML ⚠️ | ~2-3 ngày | WER tốt hơn ~3×; cần tự export, đo latency chunked-CTC |
| 6 | **Sonora AEC3** — khử tiếng LIVA tự nói khi barge-in | BSD-3 | ~1-2 ngày | Chờ thêm 1-2 tháng cho v0.x ổn định cũng hợp lý |

### Watch (chưa hành động)
TEN VAD (license "điều kiện bổ sung"), TurboQuant KV <3-bit (chưa merge), DeepFilterNet (dormant),
F5-TTS-vi ⚠️ NC, VietTTS ⚠️ NC + Linux-only, microWakeWord (khó train), full-duplex Moshi-style (research),
MTP speculative decoding (chờ llama-cpp-2), SEA-LION (check license per-model).

---

## Phụ lục: 26 nguồn đã fetch & trích claim
GitHub: pnnbao97/VieNeu-TTS · dangvansam/viet-tts · k2-fsa/sherpa-onnx · livekit/livekit-wakeword · dscripka/openWakeWord · OHF-Voice/micro-wake-word · rhasspy/piper-sample-generator · pipecat-ai/smart-turn · TEN-framework/ten-vad · snakers4/silero-vad · Rikorose/DeepFilterNet · dignifiedquire/sonora · tonarino/webrtc-audio-processing · Xiaobin-Rong/gtcrn · QwenLM/Qwen3 · aisingapore/sealion · ggml-org/llama.cpp#20969
HuggingFace: hynt/F5-TTS-Vietnamese-ViVoice · nvidia/parakeet-ctc-0.6b-Vietnamese
arXiv: 2505.22995 (LLM-confusable KWS) · 2502.06298 (SeaExam/SeaBench)
Khác: docs.nvidia.com NIM parakeet-ctc-vi · vmlu.ai/leaderboard · livekit.com/blog · daily.co/blog (smart-turn v3) · glukhov.org (16GB VRAM benchmarks)
