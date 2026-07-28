<div align="center">

# LIVA

**Trợ lý AI chạy hoàn toàn trên máy bạn.**

[![License](https://img.shields.io/badge/Giấy_phép-Cá_nhân_·_All_Rights_Reserved-red.svg)](LICENSE)
[![Windows](https://img.shields.io/badge/Windows-10%20%7C%2011%20x64-0078d4.svg)](docs/02-van-hanh/05-cai-dat-cho-nguoi-dung.md)

</div>

LIVA nghe, nói, nhìn màn hình và ghi nhớ — mọi suy luận đều diễn ra trên máy
bạn. Không tài khoản, không gửi dữ liệu ra ngoài, không cần mạng sau khi cài
xong. Lõi là một binary Rust duy nhất; giao diện là một lớp vỏ Tauri mỏng.

Đây là dự án cá nhân của **Nguyễn Anh Dương**, sinh viên Đại học FPT Hà Nội.
Nó chưa hoàn chỉnh, và tài liệu ở đây cố gắng nói rõ chỗ nào chưa xong thay vì
giấu đi. Góp ý và Pull Request đều được hoan nghênh.

---

## Dành cho người dùng

**[→ Hướng dẫn cài đặt và sử dụng đầy đủ](docs/02-van-hanh/05-cai-dat-cho-nguoi-dung.md)**
(tiếng Việt, không cần biết lập trình)

Tóm tắt:

1. Tải `LIVA_<phiên bản>_x64-setup.exe`, chạy, bấm Next tới hết.
   Windows sẽ cảnh báo vì bộ cài **chưa được ký số** — chọn *More info* →
   *Run anyway*, và chỉ cài nếu bạn tin nguồn tải.
2. Mở LIVA. Lần đầu, cửa sổ **Chuẩn bị LIVA** hiện ra: bấm *Tải model*
   (**2,28 GB** cho bộ tối thiểu; 3,65 GB nếu lấy cả phần tuỳ chọn).
   Tải dở không mất — lần sau tải tiếp từ chỗ dừng. Mỗi file được kiểm SHA-256
   trước khi dùng; lệch một byte là bị từ chối.
3. Xong. Rút mạng LIVA vẫn chạy.

Máy cần: Windows 10/11 64-bit, ~4 GB ổ trống, 8 GB RAM (16 GB thì dễ thở).
Không cần cài Node, Python, Rust, Git hay WebView2 — file cài đã gồm sẵn.

Dữ liệu của bạn — model, ký ức, cấu hình — nằm ở
`%LOCALAPPDATA%\com.liva.cognitive-os\`, **ngoài** thư mục cài
(`%LOCALAPPDATA%\LIVA\`), nên nâng cấp hay gỡ cài đặt đều không làm mất.

## Trạng thái kiểm chứng gần nhất

Ảnh chụp **28/07/2026**, một lần đo trên một máy Windows 11 — **không phải**
cam kết CI vĩnh viễn. Bộ cài build bằng `npm run installer:windows`.

- `LIVA_25.0.0_x64-setup.exe` — 235 199 064 byte, **chưa ký số** (xem mục trên).
- SHA-256 `69C6EDA5F796098CA1D0F6D468872B998F82C39D35FAFC86E636F54457DE5CF5`.
  Chỉ đúng cho **đúng file vừa build cục bộ ở trên**; bộ cài lấy từ nguồn khác
  sẽ có mã khác, đừng dùng mã này để đối chiếu.
- Thử cài sạch: cài im lặng exit 0 · đủ 4 resource bắt buộc · chạy 15 giây từ
  một thư mục làm việc khác · gỡ im lặng exit 0 · thư mục cài được xoá · hash
  khoá thiết bị giữ nguyên.
- `cargo test`: 486 unit test cùng integration/doctest, **0 lỗi**;
  `cargo clippy --all-targets -- -D warnings` exit 0.
- Stress hồi quy `voice:tts_stop`: **40/40**, tối đa 8 worker Tokio.

---

## LIVA làm được gì hôm nay

Mục này chỉ liệt kê thứ **đang chạy**, đối chiếu với mã nguồn. Thứ đã thiết kế
mà chưa nối dây nằm ở [Chưa xong](#chưa-xong) bên dưới.

- **Chat offline.** LLM chạy trong tiến trình qua `llama.cpp`, model mặc định
  Qwen3-VL-2B (Q4_K_M). Đổi model bằng một lệnh, không phải khởi động lại.
- **Nghe.** Nhận dạng giọng nói Nemotron (ONNX), chuyển ngôn ngữ Việt/Anh ngay
  lúc đang chạy. Kèm khử ồn GTCRN và cắt lượt nói bằng Silero VAD.
- **Nói.** Piper VITS với giọng Việt và Anh, tự chọn theo dấu tiếng Việt.
  Ngắt lời được: bạn nói chen vào thì LIVA im ngay.
- **Nhìn màn hình.** Chụp và so sánh vùng thay đổi bằng Rust thuần.
  `vision:ask` hỏi thẳng model đa phương thức về những gì đang hiện trên màn
  hình — kèm một giới hạn thật, xem [Chưa xong](#chưa-xong).
- **Nhớ.** SQLite chế độ WAL + chỉ mục vector `sqlite-vec` 384 chiều + FTS5,
  hợp nhất khi truy hồi. Mỗi lượt trò chuyện thành công đều được ghi và gọi lại
  ở lượt sau, qua cả ba đường vào (giọng nói, chat gõ tay, Telegram).
- **Nhường máy khi bạn bận.** Thấy game toàn màn hình, CPU hoặc GPU tải cao thì
  LIVA tự hạ mức ưu tiên để không giành tài nguyên của việc bạn đang làm.
- **Riêng tư theo mặc định.** Lõi Rust không có client AI đám mây nào. WebView
  bị khoá bằng CSP chỉ cho nối loopback. Không auto-update, không telemetry.
  Khoá API nằm trong két Stronghold mã hoá Argon2id; bảng ký ức mã hoá
  AES-256-GCM.

**Ngoại lệ, nói thẳng:** tích hợp Telegram cần Internet theo bản chất (tắt sẵn,
chỉ bật khi bạn đặt token); thư mục `liva-voice/` là sân thử nhân bản giọng nói
có gọi dịch vụ đám mây và **không** nằm trong đường thoại chính; lần tải model
đầu tiên tất nhiên cần mạng.

---

## Chưa xong

- **Nhìn màn hình cần GPU mới thực dùng được.** Trên CPU, một lượt `vision:ask`
  mất khoảng 80 giây — về mặt kỹ thuật là chạy, về mặt hội thoại là không. Với
  GPU NVIDIA nó xuống khoảng 1,2 giây, nhưng bản dựng CUDA kéo theo ~750 MB DLL
  của NVIDIA, nên **bản phát hành hiện là CPU**.
- **Bộ cài chưa được ký số** — Windows cảnh báo mỗi lần cài.
- **Không tự cập nhật.**
- **Chọn model chuyên gia còn thủ công**; định tuyến tự động theo độ khó câu hỏi
  vẫn nằm trên giấy.
- **Chưng cất ký ức, Reflection và đồ thị tri thức L3** đã thiết kế nhưng chưa
  nối dây.
- **Vòng tự sửa lỗi** có đủ khung và test, nhưng phần sinh bản vá chưa nối vào
  LLM; nó nằm sau `--features experimental`, không có trong bản mặc định.
- Chưa có bản macOS/Linux.

---

## Dành cho lập trình viên

```powershell
git clone https://github.com/DuongNAD/LIVA
cd LIVA
npm install                    # gồm sqlite-vec — thiếu nó là không mở nổi DB
npm run setup:models           # tải model (2,28 GB); --profile full → 3,65 GB
npm run doctor                 # thiếu gì → mất tính năng nào → sửa bằng lệnh gì
npm run dev                    # Vite :5173 + vỏ Tauri kèm lõi Rust
```

Cần sẵn: Node ≥ 20, Rust ≥ 1.85 (edition 2024), CMake và LLVM có
`LIBCLANG_PATH` (llama.cpp biên dịch từ C++ — lần đầu rất lâu).

```powershell
npm run installer:windows                          # kiểm cấu hình → build UI → bộ cài NSIS
cd liva-native-core; cargo test
.\target\debug\liva-native-core.exe --preflight    # máy này chạy được những gì
.\target\debug\liva-native-core.exe --setup-models # tải model không cần Node
```

| Chủ đề | Tài liệu |
|---|---|
| Cài và dùng (người dùng cuối) | [`docs/02-van-hanh/05-cai-dat-cho-nguoi-dung.md`](docs/02-van-hanh/05-cai-dat-cho-nguoi-dung.md) |
| Kiến trúc, IPC, đường ống thoại | [`docs/01-ban-ve/`](docs/01-ban-ve/) |
| Cấu hình, model, runtime, CI | [`docs/02-van-hanh/`](docs/02-van-hanh/) |
| Đối chiếu tuyên bố ↔ thực tế, nợ kỹ thuật | [`docs/03-danh-gia/`](docs/03-danh-gia/) |
| Quy ước tài liệu và cổng CI | [`docs/README.md`](docs/README.md) |
| Biến môi trường `LIVA_*` | [`.env.example`](.env.example) |
| Quy tắc cho AI agent | [`AGENTS.md`](AGENTS.md) · [`CLAUDE.md`](CLAUDE.md) |

Kiến trúc gọn trong bốn dòng: `liva-native-core` là lõi Rust (LLM, STT, TTS,
agent, WebSocket `:8002`); `liva-desktop` là vỏ Tauri v2 nhúng lõi đó
in-process; `liva-ui` là giao diện Vue 3; `liva-voice` là dịch vụ Python tách
rời để thử nhân bản giọng. Ngăn xếp Node.js/Python cũ đã được thay hẳn bằng
Rust và **không được khôi phục**.

---

## Đóng góp

Rất hoan nghênh. Trước khi mở PR:

- `npm run docs:check` và `npm run docs:cite` nếu bạn sửa `docs/`
- `cargo test` và `cargo clippy -- -D warnings` (clippy là cổng cứng, 0 cảnh báo)
- `npx vue-tsc --noEmit -p liva-ui/tsconfig.app.json` nếu bạn sửa UI
- `npm run check:installer` nếu bạn sửa cấu hình đóng gói
- Đừng bịa số. Con số nào đưa vào tài liệu cũng phải đo lại được — quy ước ở
  [`docs/README.md`](docs/README.md).

---

## Giấy phép

Bản quyền © 2026 DuongNAD. **All rights reserved.** Được dùng cho mục đích cá
nhân và học tập; không dùng thương mại khi chưa có sự đồng ý. Chi tiết ở
[`LICENSE`](LICENSE).

## Ghi công

llama.cpp · ONNX Runtime · Tauri · Qwen3-VL · NVIDIA Nemotron ASR · Piper ·
Silero VAD · sqlite-vec · multilingual-e5-small — LIVA đứng trên vai những dự
án này.
