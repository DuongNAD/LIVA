# Kế hoạch tích hợp Parakeet-CTC vi vào LIVA (STT tiếng Việt chất lượng cao)

> Trạng thái: **Model đã export + verify khả thi (2026-07-05). CHƯA tích hợp.**
> Đây là bản kế hoạch để làm tiếp — đọc file này + memory `liva-overhaul-2026-07` là đủ ngữ cảnh.

---

## 0. Bối cảnh nhanh

- LIVA hiện dùng **NVIDIA Nemotron streaming FastConformer-RNNT** cho STT (encoder/decoder/joint ONNX trong `models/nemotron-asr/`), WER tiếng Việt ~14.45 (FLEURS).
- Đã export **NVIDIA Parakeet-CTC-0.6B Vietnamese** (WER FLEURS vi **5.15 — tốt gấp ~3 lần**) sang ONNX qua NeMo trong WSL. Mục tiêu: dùng model này cho STT tiếng Việt để ra text chuẩn hơn nhiều.
- Đã đo/kiểm chứng: model **load được bằng đúng runtime của LIVA** (`ort` rc.9) qua `onnx_probe`.

## 1. Tài sản đã có (trên đĩa, đã gitignore)

| File | Ý nghĩa |
|------|---------|
| `models/parakeet_vi.onnx` | Graph ONNX (42MB, constant inline) |
| `models/parakeet_vi.onnx.data` | Weights external, **2.44GB — phải nằm CÙNG thư mục với .onnx** |
| `models/parakeet_vi_vocab.json` | List 1024 BPE token (index = id) |
| WSL `~/parakeet_export/` | venv NeMo + .nemo cache + script `export_vi.py` (để re-export nếu cần) |

**Re-export lại nếu mất file** (trong WSL Ubuntu-24.04):
```bash
cd ~/parakeet_export && export HF_HOME=~/parakeet_export/hf_cache
./venv/bin/python export_vi.py            # tạo scattered parakeet_vi.onnx + tensor files
# rồi gộp (QUAN TRỌNG: size_threshold=1024, KHÔNG phải 0):
./venv/bin/python -c '
import onnx; m=onnx.load("parakeet_vi.onnx")
onnx.save(m,"pk_graph.onnx",save_as_external_data=True,all_tensors_to_one_file=True,
          location="parakeet_vi.onnx.data",size_threshold=1024,convert_attribute=False)'
cp pk_graph.onnx /mnt/e/Project/LIVA/models/parakeet_vi.onnx
cp parakeet_vi.onnx.data /mnt/e/Project/LIVA/models/parakeet_vi.onnx.data
```

## 2. Contract của model (ĐÃ VERIFY bằng onnx_probe)

**Inputs:**
- `audio_signal`: `Float32 [B, 80, T]` — 80 log-mel features × T frames
- `length`: `Int64 [B]` — số frame thật của mỗi mẫu (= T nếu không pad)

**Output:**
- `logprobs`: `Float32 [B, T, 1025]` — log-softmax. **1025 = 1024 BPE token + 1 CTC blank** (blank = **index 1024**, tức index cuối).

## 3. Tham số tiền xử lý (từ model.cfg.preprocessor — KHÁC Nemotron!)

| Tham số | Parakeet vi | Nemotron (hiện tại) |
|---------|-------------|---------------------|
| n_mels / features | **80** | 128 |
| sample_rate | 16000 | 16000 |
| n_fft | 512 | 512 |
| window_size | 0.025s = **400 samples** | 400 |
| window_stride (hop) | 0.010s = **160 samples** | 160 |
| window | hann | hann |
| **preemphasis** | **KHÔNG (None)** | 0.97 (áp ở `stt/mod.rs:128`) |
| **normalize** | **`per_feature`** | (khác) |
| dither | 1e-5 | — |
| mag_power | 2.0 (power = \|FFT\|²) | 2.0 |
| log | log(mel) sau normalize | log(mel+eps) |

**`per_feature` normalize (NeMo):** với mỗi mel-bin (mỗi trong 80 chiều), tính mean & std TRÊN TOÀN BỘ T frames của utterance, rồi `(x - mean) / (std + 1e-5)`. Tức chuẩn hóa từng feature-dimension độc lập, dùng thống kê của chính câu đó.
Thứ tự NeMo: waveform → (dither) → STFT → power → mel filterbank (80) → **log** → **per_feature normalize**. (Lưu ý: KHÔNG preemphasis, KHÁC Nemotron.)

## 4. Kiến trúc đề xuất

Parakeet-CTC là model **OFFLINE / cả câu** (không cache-aware streaming như Nemotron). Vì vậy:

- **Dùng cho đường "transcribe cả câu sau VAD-end"** — đúng chỗ wake-gate STT (`main.rs` nhánh ngủ) và transcribe cuối câu. Ở đó ta đã có buffer audio trọn câu.
- **KHÔNG dùng cho streaming partial** trong lúc user đang nói (giữ Nemotron cho việc đó nếu cần), vì CTC offline cần cả câu.
- Chọn engine theo ngôn ngữ/ config: khi session là tiếng Việt → Parakeet; tiếng Anh → giữ Nemotron. Env đề xuất: `LIVA_STT_VI_ENGINE=parakeet|nemotron` (default nemotron để không đổi hành vi).

→ v1 tối giản: thêm module `stt/parakeet.rs` độc lập; `SttManager` gọi nó cho transcribe-cả-câu khi bật + ngôn ngữ vi. Không đụng đường Nemotron streaming.

## 5. Các bước code (Rust) — chi tiết

### B1. Module `liva-native-core/src/stt/parakeet.rs`
Struct `ParakeetVi { session: ort::Session, vocab: Vec<String>, dsp: ParakeetDsp }`.

**DSP 80-mel per_feature** (viết mới hoặc tổng quát hoá `stt/dsp.rs` — nhưng `SttDsp` hiện hardcode `num_frames=65`/`samples=10640` cho chunk Nemotron, nên VIẾT DSP RIÊNG linh hoạt T động):
```
fn log_mel_per_feature(samples: &[f32]) -> (Vec<f32> /*[80*T] row-major (mel-major hay time-major?)*/, usize T)
  1. (optional) dither: samples[i] += 1e-5 * randn()   // có thể bỏ ở inference
  2. framing: center-pad n_fft/2; hop=160; win=400 hann; STFT → power |.|^2 (257 bins)
  3. mel: nhân mel filterbank 80×257 (dùng lại compute_mel_filterbank(512, 80, 16000) trong dsp.rs — nó đã có sẵn, chỉ đổi num_mels=80)
  4. log: ln(mel + guard)   // NeMo: log của mel (guard nhỏ)
  5. per_feature normalize: cho mỗi trong 80 bin, tính mean/std qua T, (x-mean)/(std+1e-5)
  → output layout khớp input onnx: [1, 80, T] (mel-major: feature index chậm, time nhanh) — KIỂM TRA layout bằng cách so 1 câu với NeMo python (xem B4).
```
Lưu ý: `compute_mel_filterbank` trong `dsp.rs` DÙNG LẠI ĐƯỢC với num_mels=80 (nó nhận num_mels tham số). Nhưng kiểm tra thang mel (NeMo dùng Slaney/HTK? — Nemotron code hiện dùng công thức trong dsp.rs; Parakeet/NeMo mặc định **htk=False → Slaney**. Nếu lệch, mel bank phải khớp NeMo). **Đây là rủi ro chính xác — verify bằng B4.**

**Load model + external data:** `Session::builder()?.commit_from_file("models/parakeet_vi.onnx")` — ort tự load `.onnx.data` cùng dir. (Đã verify load OK.)

**Inference:**
```
inputs: audio_signal = Value::from_array(([1,80,T], mel_flat));
        length = Value::from_array(([1], vec![T as i64]));
out logprobs = [1, T, 1025]
```

**CTC greedy decode:**
```
ids = []
prev = -1
for t in 0..T:
    a = argmax over 1025 of logprobs[0,t,:]
    if a != 1024 /*blank*/ and a != prev: ids.push(a)   // collapse repeats + drop blank
    prev = a
tokens = ids.map(|i| vocab[i])          // BPE pieces (có ▁ = word boundary)
text = detokenize(tokens)                // ghép: ▁ → space; nối phần còn lại
```
Detokenize giống logic đã có ở `stt/tokenizer.rs` (SentencePiece: `▁`=space-prefix, byte-fallback `<0xNN>` nếu có, filter control `<...>`). **Cân nhắc TÁI SỬ DỤNG `stt/tokenizer.rs::decode`** nếu vocab tương thích (nó đã xử lý ▁/byte-fallback/control tokens đúng). Chỉ cần nạp vocab Parakeet vào cùng cấu trúc id→token.

### B2. Wiring vào `SttManager` (`stt/mod.rs`)
- Thêm field `parakeet: Option<ParakeetVi>`, init khi `LIVA_STT_VI_ENGINE=parakeet` + file tồn tại.
- Ở đường transcribe-cả-câu (hàm `feed_audio(..., final=true)` hoặc chỗ wake-gate gọi STT): nếu ngôn ngữ vi + parakeet có → dùng ParakeetVi thay Nemotron; else giữ Nemotron.
- Giữ `LIVA_STT_LANGUAGE` như hiện tại.

### B3. Env + docs
- `.env.example`: thêm `LIVA_STT_VI_ENGINE=nemotron` (mặc định), `LIVA_PARAKEET_MODEL_PATH=` (để trống = models/parakeet_vi.onnx).
- `models/README.md`: đã có dòng Parakeet.

### B4. VERIFY chính xác (bắt buộc — tránh DSP lệch)
Đây là bước dễ sai nhất (mel bank/normalize). Cách kiểm chứng vàng:
1. Trong WSL, chạy NeMo python transcribe 1 file wav mẫu → lấy text đúng + (nếu được) dump mel features [80,T] ra .npy.
2. Trong Rust, chạy ParakeetVi trên CÙNG file → so text. Nếu khác → so mel features với .npy của NeMo (bước 1) để tìm chỗ lệch (mel scale? log? normalize? layout?).
3. Viết probe bin `parakeet_probe.rs` (giống `stt_lang_probe`): nhận wav → in transcript. Test trên vài câu tiếng Việt thật.
4. Round-trip nhanh: Piper vi đọc câu → ParakeetVi nghe lại → so với Nemotron (kỳ vọng Parakeet chuẩn hơn).

## 6. Gotchas đã biết (đừng vấp lại)

- **External data**: giữ `parakeet_vi.onnx` + `parakeet_vi.onnx.data` CÙNG dir; field `location` trong graph = `"parakeet_vi.onnx.data"`. Nếu re-gộp, dùng `size_threshold=1024` (KHÔNG 0 — ort rc.9 fail shape-inference nếu constant nhỏ ra external).
- **Blank id = 1024** (index cuối của 1025).
- **KHÔNG áp preemphasis** cho Parakeet (khác Nemotron). Đừng vô tình dùng lại đường preemph ở `stt/mod.rs:128`.
- **per_feature normalize** dùng thống kê của chính câu (không phải global) — phải tính mean/std qua T cho từng mel-bin.
- Mel filterbank scale (Slaney vs HTK) là rủi ro chính xác — verify bằng B4.
- Model chậm hơn Nemotron (600M params, offline, CPU) — đo RTF; nếu chậm cân nhắc chạy chỉ khi cần (transcribe cuối câu) hoặc dùng GPU (ort cuda) cho đường này.

## 7. Test plan
- Unit: CTC decode (chuỗi logprobs giả → text kỳ vọng); per_feature normalize (mean≈0/std≈1 mỗi bin).
- Integration: `parakeet_probe.exe` trên 3-5 câu tiếng Việt thật → so Nemotron.
- Regression: `cargo test` full; verify_duplex/integrations không đổi (Parakeet là đường thêm, opt-in).

## 8. Ước tính công sức
~1-2 ngày: DSP 80-mel per_feature (nửa ngày, phần dễ sai) + CTC decode (vài giờ) + wiring + verify B4 (nửa ngày). Rủi ro chính: khớp mel/normalize với NeMo.

## 9. Dọn dẹp WSL (khi chắc không re-export nữa)
`~/parakeet_export/` còn vài GB (scattered tensor files + pk_graph + .data + .nemo cache). Giữ `venv` + `export_vi.py` + `hf_cache` (.nemo) nếu có thể re-export; xóa `parakeet_vi.onnx.data`, `pk_graph.onnx`, các file `encoder.*`/`onnx__*`/`decoder.*`/`Constant_*` scattered.

---
*Ghi chú: mọi thay đổi code đợt này (hybrid wake, denoise/AEC/turn-shadow, wake models) đã commit lên `github.com/DuongNAD/LIVA` main. File model Parakeet ở local, gitignored.*
