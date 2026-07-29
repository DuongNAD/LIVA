---
title: "Wake word — việc còn lại sau bản vá 27/07/2026"
updated: 2026-07-29
commit: 0fd816c
status: living
owns:
  - wake-word-viec-con-lai
covers:
  - liva-native-core/src/wake.rs
  - liva-native-core/src/wake_model.rs
  - liva-native-core/src/webrtc/frame.rs
  - liva-native-core/src/websocket.rs
  - liva-native-core/src/stt/mod.rs
  - liva-ui/src/composables/useVoicePipeline.ts
  - liva-ui/src/workers/LivaWakeWorker.ts
  - liva-ui/src/utils/voiceFrame.ts
  - scripts/e2e-wake-probe.mjs
---
# Wake word — việc còn lại sau bản vá 27/07/2026

[⬆ Mục lục](../README.md) · [◀ Nhắn tin ra ngoài](06-nhan-tin-ra-ngoai.md)

---

> **Đọc cái này trước.** Lỗi gốc ("widget nhảy với mọi tiếng nói") **đã đóng và đã
> nghiệm thu trên máy thật**.
>
> Câu hỏi *"Hey Liva đứng một mình có đánh thức được không?"* **đã đo xong ngày
> 27/07/2026 và câu trả lời là KHÔNG** — với bộ model hiện có. Đừng đi đo lại (§2),
> đừng đi hạ ngưỡng (§3, W2-a đã bị loại). Việc còn lại là **chọn giữa W2-b và W2-c**
> (§3) — một quyết định về sản phẩm, không phải về code.

Kiến trúc và chi tiết cài đặt: [Đường ống thoại §9](../01-ban-ve/03-duong-ong-thoai.md#9-wake-word--hai-hệ-nay-đã-nối-vào-nhau).
Hợp đồng dây `OP_WAKE_PROBE`: [Giao thức IPC và WebSocket §5.3](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md).

---

## 1. Đã xong — đừng làm lại

| Việc | Trạng thái | Bằng chứng |
|---|---|---|
| Bỏ bộ dò MLP-RMS mù âm vị trong browser | **[OK]** | `LivaWakeWorker.ts` nay chỉ cắt câu, không nạp model nào |
| `OP_WAKE_PROBE` (0x05): widget hỏi, core quyết | **[OK]** | 9/9 test worker, 273/273 vitest, 460/460 cargo test |
| Hai tầng xác minh: classifier HOẶC STT+so cụm từ | **[OK]** | `wake.rs::score_clip` + `matches_phrase` |
| Log chẩn đoán ở core (transcript, điểm classifier, rms/đỉnh clip) | **[OK]** | Cần thiết vì `logger` của UI chỉ ghi vào console webview Tauri — terminal không thấy |
| Đường dẫn model chạy qua Tauri | **[OK]** | Qua `resolve_resource_path`; log thật in `../..\models/wake_liva_en.onnx` |
| Không còn nhảy bừa | **[OK]** — nghiệm thu trên máy thật | 28/28 câu tiếng Việt trong phòng (nền TV/video) đều bị từ chối, 0 lần đánh thức sai |
| Câu dài có "Liva" đánh thức được | **[OK]** | "Này Liva ơi, bật nhạc lên giúp tôi" → đánh thức, qua socket thật |

**Trạng thái commit (tính đến `2b12125`):** phần lớn thay đổi **đã được commit** — nhưng bị một
phiên khác gộp nhầm vào các commit không liên quan (`OP_WAKE_PROBE` nằm trong `9c0d0e8`
*"fix(outbox)…"*). Còn **chưa commit**: `wake.rs` + `websocket.rs` (phần log điểm classifier).
Đừng tưởng thiếu — kiểm bằng `grep -r OP_WAKE_PROBE` trước khi kết luận.

---

## 2. Số đo đã có — đừng đo lại

Tất cả đo ngày 27/07/2026, qua socket thật, STT Nemotron thật.

| Đo | Kết quả | Ý nghĩa |
|---|---|---|
| Clip "Hey Liva" đơn lẻ, 3,05 s, **rms 0,0625, đỉnh 0,507** | STT trả **chuỗi rỗng** | Audio **to và sạch**. Nemotron không phiên âm nổi cụm ngắn đứng riêng. **Đệm thêm im lặng KHÔNG cứu được** — đã thử. |
| Cùng nội dung, cắt còn 0,8 s | rỗng | Sàn dưới của STT ≈ **1,3 s** *tiếng nói thật* |
| Cùng nội dung, 1,3 s / 1,6 s / 2,4 s | ra chữ | — |
| Câu dài liên tục 2,6 s | ra chữ, đánh thức | Đây là ca đang chạy được |
| Classifier cần | **196 mel frame ≈ 1,96 s** | Dưới mức đó `predict_raw` trả rỗng ⇒ `minProbeMs = 2300` trong worker |
| Classifier chấm "Hey Liva" giọng **Piper en-US** | **0,023** (ngưỡng 0,68) | Một mình thì là bằng chứng yếu (giọng tổng hợp, tên bịa) — nhưng khớp hệt số đo giọng thật bên dưới |
| **Classifier chấm "Hey Liva" GIỌNG THẬT, 8 mẫu** | **0,004 – 0,025, TB 0,014** (ngưỡng 0,68) | **Đây là phép đo quyết định.** Clip đỉnh 0,61–0,96 rms 0,017–0,032 ⇒ tiếng nói rõ, gần mic. Model chấm cụm đánh thức **ngang tiếng ồn** (fixture negative: 0,0045). **Không phải lệch ngưỡng** — cách 27 lần. ⚠️ *Một điểm chưa chốt: 8 clip này được suy ra là chủ máy đang nói "Hey Liva" theo thời điểm và mức tín hiệu, chứ không có ai xác nhận từng clip. Nếu muốn chắc, thu một WAV rồi chạy `scripts/e2e-wake-probe.mjs` — nhưng kết luận khó đổi, vì giọng Piper cũng cho 0,023.* |
| Classifier trên fixture tham chiếu (`hey_livekit`) | 0,9997 pos / 0,0009 neg | Pipeline mel→embedding→classify **đúng**; vấn đề (nếu có) nằm ở giọng, không ở code |
| Tỷ lệ probe cho transcript rỗng, môi trường thật | **22/28 (79 %)** | Phần lớn là đoạn ngắn/nhỏ vượt sàn RMS trong chốc lát |
| Phiên âm "Liva" của Nemotron | "Li Vơ" (giọng Piper VN) | ⇒ đã thêm `li vơ` vào `LIVA_WAKE_PHRASES` mặc định |

---

## 3. Việc còn lại, theo thứ tự

### W1 — ĐO classifier trên giọng người thật — **XONG 27/07/2026**

Kết quả: **0,004–0,025 trên 8 mẫu** (ngưỡng 0,68). Rơi vào nhánh "< 0,1" ⇒ model không
nhận giọng này. Chi tiết ở §2. **Không cần đo lại.**

Nếu vẫn muốn đo trên một giọng khác (người khác trong nhà, mic khác), dòng log là:

```
Wake probe rejected — nghe ra "" | classifier wake_liva_en 0.xxx/0.68 | clip 2.30s rms ... đỉnh ...
```

### W2 — Quyết cho "Hey Liva" trần

- **~~W2-a — Hạ `LIVA_WAKE_THRESHOLD`~~ — ĐÃ LOẠI.** Chỉ hợp lệ nếu điểm rơi 0,4–0,68.
  Thực đo là 0,014, tức phải hạ ngưỡng xuống **dưới cả điểm của tiếng ồn thuần**
  (0,0045). Làm vậy là **mở lại đúng lỗi vừa sửa**, chỉ khác cái tên. Đừng làm.

- **W2-b — Train classifier lại bằng giọng chủ máy.** Đây là cách **duy nhất** lấy lại
  được "Hey Liva" trần. Toolkit Python `livekit-wakeword` (xem doc-comment
  `wake_model.rs`); cần thu vài chục mẫu giọng thật + augment. Bản `wake_liva_vi.onnx`
  sẵn có **KHÔNG** dùng thay được: FPPH 19,4 (`models/README.md`), bật lên là tự chuốc
  lại lỗi nhảy bừa.
  ⚠️ **Cấm thêm crate `livekit-wakeword` vào `Cargo.toml`** — nó bật
  `ort/alternative-backend`, Cargo hợp nhất feature toàn graph và giết **mọi**
  `ort::Session` khác trong process (VAD/GTCRN/Smart Turn/STT/TTS). Train bằng toolkit
  Python **ngoài** repo, chỉ đem file `.onnx` vào.
  Nghiệm thu: recall ≥ 90 % trên 10 lần nói **và** FPPH ≤ 1 khi để TV bật ≥ 1 giờ.

- **W2-c — Chấp nhận ràng buộc, đổi UX.** Yêu cầu cụm dài hơn: *"Liva ơi, bật nhạc lên"*.
  **Đã chạy được, không cần code gì thêm** — khớp thiết kế một-hơi sẵn có của `wake.rs`.
  Chi phí thật: phải nói rõ trong UI, vì hiện người dùng nói "Hey Liva" rồi tưởng hỏng.
  Đây là **lựa chọn mặc định** nếu không ai bỏ công làm W2-b.

**Ghi chú thẳng thắn cho người quyết:** W2-c không phải giải pháp tạm — với bộ model
hiện có nó là *đúng* cách dùng. Cụm càng dài, STT càng có cái để bám; "Hey Liva" trần
thất bại vì nó ngắn, chứ không phải vì code sai.

### W3 — Giảm 79 % probe vô ích *(hiệu năng, không chặn)*

Mỗi probe rỗng vẫn tốn một lượt STT. Với TV bật, đó là ~1 lượt/1,2 s liên tục.

**Hướng:** siết bộ cắt câu trong `LivaWakeWorker.ts` — yêu cầu đoạn nói dài hơn
`minUtteranceMs`, hoặc bỏ qua khi năng lượng chỉ vượt sàn trong chốc lát.
**Nghiệm thu:** tỷ lệ transcript rỗng < 30 % mà **không** làm hỏng W1.

**Rủi ro cần canh:** cooldown 1200 ms nghĩa là khi có tiếng nói liên tục trong phòng,
"Hey Liva" thật của người dùng **có thể bị nuốt** vì probe trước còn trong cooldown.
Chưa đo. Nếu W1 cho recall thấp *chỉ khi có TV bật*, đây là nghi phạm số một.

### W4 — Dọn xác *(nhỏ)*

Còn nằm trên đĩa, không ai dùng: `liva-ui/public/models/hey_liva*.onnx`,
`liva-ui/src/workers/hey_liva_weights.json`, `scripts/generate_hey_liva_model.py`.
Xoá kèm cập nhật `covers:` của [Đường ống thoại](../01-ban-ve/03-duong-ong-thoai.md).

---

## 4. Bẫy đã dính — đừng dính lại

1. **`models/asr_example.wav` là file IM LẶNG** (dummy tạo để test đường dẫn). Test STT
   bằng nó luôn ra rỗng và trông y hệt code hỏng. Tôi đã mất một vòng vì cái này.
2. **`stt_lang_probe` ép `lang_id` thô** nên cho kết quả **tệ hơn hẳn** đường thật của
   gateway. Cùng một clip: probe ra `"Leave of what is…"`, gateway ra
   `"Hey Liva, what is the weather today in Hanoi"`. **Chỉ tin đường gateway.**
3. **Grep `": error:"` KHÔNG bắt `error[E0308]:`.** Tôi từng báo "clippy 0 issues" trong
   khi code không compile. **Luôn dùng exit code**, đừng đếm dòng grep.
4. **Đặt tên file test bằng số trong JS**: `'trunc_'+2.0+'.wav'` ra `trunc_2.wav`, không
   phải `trunc_2.0.wav`. File thiếu ⇒ output rỗng ⇒ trông như model hỏng. Đã suýt kết
   luận sai về "STT nhạy biên chunk" vì lỗi này. **Luôn kiểm file tồn tại trước khi đo.**
5. **`logger` của UI chỉ ghi vào console webview Tauri** — terminal không thấy. Mọi thứ
   cần chẩn đoán **phải** log ở phía core.
6. **Đường dẫn model tương đối phải qua `resolve_resource_path`** — `tauri dev` đặt cwd ở
   `liva-desktop/src-tauri`.

---

## 5. Cách kiểm lại

```bash
node scripts/e2e-wake-probe.mjs <file.wav> 8002
```

Thoát 0 = đánh thức, 2 = từ chối. Tự hạ mẫu về 16 kHz. Chưa có clip thì sinh bằng chính
TTS của LIVA (`tts_piper_probe.exe`) — xem doc-comment đầu script.

**Bộ ba clip đã dùng để nghiệm thu (3/3 đúng):** "Này Liva ơi, bật nhạc lên giúp tôi" →
đánh thức · "Hôm nay trời đẹp quá, đi ăn cơm không" → từ chối · "Hey Liva, what is the
weather today in Hanoi" → đánh thức.

⚠️ Cả ba đều là **giọng Piper tổng hợp**. Chúng chứng minh **đường dây** đúng, **không**
chứng minh gì về giọng người thật — đó chính là W1.
