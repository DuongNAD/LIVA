---
title: "WER tiếng Việt qua đường ống STT sản xuất"
updated: 2026-08-04
commit: 596e8b6
stale-ok: bd11c84
status: living
owns:
  - wer-tieng-viet-do-thuc-te
covers:
  - liva-native-core/src/bin/wer_bench.rs
  - liva-native-core/src/stt/mod.rs
  - scripts/prepare-fleurs-vi.py
---

# WER tiếng Việt qua đường ống STT sản xuất

Ngày đo: 2026-08-04

Dataset: `google/fleurs`, config `vi_vn`, split `test`

Revision: `70bb2e84b976b7e960aa89f1c648e09c59f894dd`

Cỡ mẫu: 100 câu đầu của test split, 3.024 từ tham chiếu

Parakeet được đo bằng đúng artifact OpenVoiceOS mà profile `full` phát hành tại revision
`240d82cc243f7cf47d100b293c7dff96e65a04c2`. SHA-256 graph
`parakeet_vi.onnx` là
`aa5658c3499fc991780e44ad5ccd9d4393d1266727a281cb3e4ca39be42334c4`, khớp manifest.
Hai trường xuất xứ trong JSON do `wer_bench` đọc trực tiếp từ manifest và từ file graph
thật; binary từ chối chạy nếu SHA-256 của graph không khớp entry chứa revision đó.

## Kết quả

| Engine | Substitution | Deletion | Insertion | WER | RTF | VAD-end → transcript p50 | VAD-end → transcript p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Nemotron | 326 | 58 | 70 | **15,01%** | 0,310 | 3.772 ms | 6.111 ms |
| Parakeet | 234 | 48 | 81 | **12,00%** | 0,059 | 667 ms | 1.170 ms |

Kết quả máy đọc được nằm ở [`wer-fleurs-vi.json`](wer-fleurs-vi.json).

Hai lượt đo đều gọi
`liva_native_core::stt::SttManager::feed_audio(audio, true)`. Đây là API mà
đường WebRTC/Telegram dùng khi kết thúc một utterance, nên benchmark đi qua
đúng lựa chọn engine, DSP và tokenizer/decode của LIVA. Binary không mở session
ONNX trực tiếp.

Latency end-of-turn được bấm giờ ngay trước lời gọi `feed_audio(audio, true)` và
dừng khi nhận final transcript; không tính thời gian đọc WAV hay chấm WER. p50/p95
dùng nearest-rank trên đủ 100 utterance. Lần lazy-load model đầu tiên vẫn nằm trong
phân phối đo, đúng với hành vi process mới khởi động.

## Quyết định engine mặc định

Parakeet là mặc định cho STT whole-utterance tiếng Việt. Nó có WER thấp hơn 3,01
điểm phần trăm (Nemotron cao hơn khoảng 1,25 lần), p50 nhanh hơn khoảng 5,7 lần
và p95 nhanh hơn khoảng 5,2 lần trên cùng corpus. Profile `full` phân phối graph
và external weights FP32 từ OpenVoiceOS tại revision cố định
`240d82cc243f7cf47d100b293c7dff96e65a04c2`, có kiểm SHA-256.
Đường wake-word vẫn cưỡng bức Nemotron nhẹ qua `transcribe_for_wake`; lựa chọn
Parakeet chỉ áp dụng cho whole-utterance tiếng Việt sau VAD-end. Khi model chưa
được tải hoặc không nạp được, runtime lùi về Nemotron và `system_status` ghi rõ
fallback. Có thể opt-out bằng `LIVA_STT_VI_ENGINE=nemotron`.

## Tái lập

Chạy từ repo root trên PowerShell:

```powershell
npm run setup:models -- --profile full
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
