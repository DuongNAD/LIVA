---
title: "WER tiếng Việt qua đường ống STT sản xuất"
updated: 2026-08-03
commit: 596e8b6
status: living
owns:
  - wer-tieng-viet-do-thuc-te
covers:
  - liva-native-core/src/bin/wer_bench.rs
  - liva-native-core/src/stt/mod.rs
  - scripts/prepare-fleurs-vi.py
---

# WER tiếng Việt qua đường ống STT sản xuất

Ngày đo: 2026-08-03

Dataset: `google/fleurs`, config `vi_vn`, split `test`

Revision: `70bb2e84b976b7e960aa89f1c648e09c59f894dd`

Cỡ mẫu: 100 câu đầu của test split, 3.024 từ tham chiếu

## Kết quả

| Engine | Substitution | Deletion | Insertion | WER | RTF |
| --- | ---: | ---: | ---: | ---: | ---: |
| Nemotron | 326 | 58 | 70 | **15,01%** | 0,414 |
| Parakeet | 234 | 48 | 81 | **12,00%** | 0,077 |

Kết quả máy đọc được nằm ở [`wer-fleurs-vi.json`](wer-fleurs-vi.json).

Hai lượt đo đều gọi
`liva_native_core::stt::SttManager::feed_audio(audio, true)`. Đây là API mà
đường WebRTC/Telegram dùng khi kết thúc một utterance, nên benchmark đi qua
đúng lựa chọn engine, DSP và tokenizer/decode của LIVA. Binary không mở session
ONNX trực tiếp.

## Tái lập

Chạy từ repo root trên PowerShell:

```powershell
py -m pip install datasets soundfile
py scripts/prepare-fleurs-vi.py --limit 100
cargo run --release -p liva-native-core --bin wer_bench -- `
  --manifest data/benchmarks/fleurs-vi/fleurs-vi-test.jsonl `
  --engine both `
  --limit 100 `
  --output docs/05-chat-luong/wer-fleurs-vi.json
```

Audio được materialize thành PCM16 mono 16 kHz trong
`data/benchmarks/fleurs-vi/` và không được commit. `wer_bench` mặc định từ chối
chạy nếu có dưới 100 câu.

## Quy tắc chấm

- Chuẩn hoá Unicode bằng lowercase, giữ chữ/số (bao gồm dấu tiếng Việt), đổi
  dấu câu thành khoảng trắng và gộp whitespace.
- WER là tổng substitution + deletion + insertion chia tổng từ tham chiếu trên
  toàn bộ corpus, không phải trung bình WER từng câu.
- Các số này không so trực tiếp với model card nếu model card dùng tập con,
  text normalizer hoặc điều kiện inference khác.
