---
title: "Việc cần làm — rà soát 25/08/2026: cổng kiểm và thoại real-time"
updated: 2026-08-25
commit: 0a02b9cd
status: living
owns:
  - viec-can-lam-2026-08-25
covers:
  - .github/workflows/test.yml
  - liva-native-core/Cargo.toml
  - liva-native-core/src/preflight.rs
  - liva-native-core/src/tts/mod.rs
  - liva-native-core/src/wake.rs
  - liva-native-core/src/webrtc/aec.rs
  - liva-native-core/src/webrtc/pipeline.rs
  - liva-native-core/src/webrtc/session.rs
  - liva-native-core/src/webrtc/turn_shadow.rs
  - liva-native-core/src/websocket.rs
  - liva-ui/src/composables/useVoicePipeline.ts
  - teamwork_projects/obsidian_llm_wiki/src/vault.ts
  - teamwork_projects/obsidian_llm_wiki/tests/verification-challenger.test.ts
---
# Việc cần làm — rà soát 25/08/2026: cổng kiểm và thoại real-time

[⬆ Mục lục](../README.md) · [Master roadmap](roadmap.md) · [Việc còn lại `mac-v2`](viec-con-lai-mac-v2.md) · [Backlog nâng cấp](../03-danh-gia/05-nang-cap-toan-dien.md) · [Voice runtime](../03-he-thong-con/voice.md) · [Voice SLO](../05-chat-luong/voice-slo.md)

## 0. Phạm vi — đọc trước, kẻo làm trùng

Tài liệu này sở hữu **tám việc phát sinh từ rà soát ngày 25/08/2026**: ba việc cổng
kiểm (VC-1…VC-3), bốn việc thoại real-time (VC-4…VC-7), và [VC-8](#vc-8--lip-sync-theo-phoneme-thay-vì-theo-biên-độ)
(lip-sync theo phoneme) thăng lên từ lượt rà nguồn mở. Những việc này **chưa có chủ ở bất kỳ
tài liệu nào** trước lượt rà soát — đó là lý do có tài liệu mới thay vì chèn vào tài liệu cũ.

[§6](#6-kho-ứng-viên-đã-sàng--rà-nguồn-mở-25082026) ghi **kết quả sàng** của lượt rà nguồn mở:
mười một ứng viên bị loại kèm lý do đo được. Đó là tri thức phủ định — đọc trước khi đề xuất
thư viện mới, kẻo đề xuất lại thứ repo đã có.

| Loại việc | Chủ sở hữu | Ranh giới |
|---|---|---|
| Hạ tầng nhánh `mac-v2`, vệ sinh repo, xác thực CI | [Việc còn lại `mac-v2`](viec-con-lai-mac-v2.md) (MV-1…MV-13) | VC-1 và VC-3 **là việc mã nguồn** phát sinh *từ* MV-1/MV-2; số đo của run CI thuộc về MV |
| Năng lực sản phẩm U1–U33 | [Backlog nâng cấp](../03-danh-gia/05-nang-cap-toan-dien.md) | Đừng chép mục U nào vào đây. VC-4 là **hạ tầng đo** cho [U21](../03-danh-gia/05-nang-cap-toan-dien.md), không thay thế U21 |
| Ngưỡng runtime thoại và SLO | [Voice SLO](../05-chat-luong/voice-slo.md) | Bảng ngưỡng nằm ở đó; tài liệu này chỉ nói **việc phải làm** để §3 của nó có số |
| Kiến trúc as-built thoại | [Voice runtime](../03-he-thong-con/voice.md) | Sửa xong VC-4…VC-7 thì cập nhật *tài liệu đó*, không mô tả lại ở đây |
| Việc "không nên làm" | [Backlog §9](../03-danh-gia/05-nang-cap-toan-dien.md) | Đọc trước khi tự nghĩ ra việc |

---

## 1. Bằng chứng — đo 25/08/2026 tại `0a02b9cd`, trên macOS, cây làm việc sạch

⚠️ **Đo trên macOS. Runner CI là `windows-latest`.** Xanh ở đây không chứng minh xanh ở CI.
Đây là lượt đo độc lập, không kế thừa kết luận của phiên trước.

| Cổng | Lệnh | Kết quả |
|---|---|---|
| Format | `cargo fmt --all -- --check` | ✅ exit 0 |
| Clippy | `cargo clippy --all-targets` | ✅ 0 warning |
| Typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` | ✅ 0 lỗi |
| ESLint | `npx eslint . --max-warnings 0` | ✅ 0 |
| Test UI | `npx vitest run` (trong `liva-ui/`) | ✅ **402/402**, 38 file |
| Lỗ hổng npm | `npm audit --audit-level=high` | ✅ 0 |
| Tài liệu | `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` | ✅ exit 0 |
| Trích dẫn | `node scripts/docs-citations.mjs --max-unchecked=207` | ✅ 0 neo hỏng |
| Kiểm kê | `node scripts/docs-inventory.mjs --check` · `docs-capabilities.mjs --check` | ✅ 102 tài liệu · 19 năng lực |
| **Rust deny** | `cargo deny check -W unmaintained -W unsound advisories licenses sources` | ❌ **1 vulnerability** → [VC-1](#vc-1--h2-0327-rustsec-2026-0258) |
| **Test Rust** | `cargo test --workspace` | ❌ **571 pass · 1 fail** → [VC-2](#vc-2--cargo-test-đỏ-ngầm-từ-07082026) |
| **Knowledge vault** | `npm test -w obsidian-llm-wiki` | ❌ **24 pass · 3 fail** → [VC-3](#vc-3--ba-test-vault-đỏ-trên-macos--đã-truy-được-nguyên-nhân) |

**Không đo được trên máy này:** mọi thứ cần model. `models/` chỉ chứa config —
`find models -name "*.gguf" -o -name "*.onnx"` trả **0 file**, `models/nemotron-asr` rỗng
0 byte. Nên **không có số độ trễ thoại nào trong tài liệu này**, và VC-4 tồn tại chính
vì lý do đó.

---

## 2. Tám việc và thứ tự thi hành

| Thứ tự | ID | Việc | Nhóm | Vì sao ở vị trí này |
|---|---|---|---|---|
| 1 | [VC-4](#vc-4--đo-lượt-thoại-bốn-mốc-tracing) | Đo lượt thoại — bốn mốc `tracing` | Thoại | Sáu SLO thoại **không thể nghiệm thu** khi thiếu nó; VC-5…VC-7 đều không chứng minh được kết quả |
| 2 | [VC-1](#vc-1--h2-0327-rustsec-2026-0258) | Vá advisory `h2` | Cổng | Đang chặn CI ở bước 9/25; 16 bước sau **chưa từng chạy** trên nhánh này |
| 3 | [VC-2](#vc-2--cargo-test-đỏ-ngầm-từ-07082026) | Gỡ phụ thuộc đĩa của test `preflight` | Cổng | CI sẽ đỏ **ngay tại đây** sau khi VC-1 xong — đừng để phát hiện điều đó bằng một run CI nữa |
| 4 | [VC-3](#vc-3--ba-test-vault-đỏ-trên-macos--đã-truy-được-nguyên-nhân) | Sửa 3 test vault + thêm job macOS vào CI | Cổng | Chẩn đoán đã xong, phần còn lại là cơ học |
| 5 | [VC-6](#vc-6--mẩu-tts-đầu-tiên-của-mỗi-lượt-quá-dài) | Mẩu TTS đầu tiên ngắn lại | Thoại | Ăn thẳng vào cảm nhận người dùng, phạm vi một hàm, đã có test bao quanh |
| 6 | [VC-5](#vc-5--barge-in-tự-cắn-và-aec-nhiều-khả-năng-không-hội-tụ) | Tham chiếu far-end liên tục + test AEC có răng | Thoại | Bật AEC trước khi sửa cái này là bật một thứ chưa chắc chạy |
| 7 | [VC-7](#vc-7--kết-lượt-là-704-ms-im-lặng-cố-định) | Smart Turn **chỉ được rút ngắn** | Thoại | Cần VC-4 để chứng minh nó thật sự rút, và cần log shadow để hiệu chỉnh ngưỡng |
| 8 | [VC-8](#vc-8--lip-sync-theo-phoneme-thay-vì-theo-biên-độ) | Lip-sync theo phoneme thay vì biên độ | Avatar | Không thêm phụ thuộc, không cần đo trước; độc lập với các cổng đỏ nên chạy song song được — nhưng đừng bắt đầu khi CI còn đỏ |

**Quy tắc chung.** Một commit một chủ đề. **Không trộn mã nguồn với tài liệu trong một
commit** — `docs-check` so `git log <commit>..HEAD` nên commit gộp làm nó đỏ. Hook
pre-commit cần `SKIP_AI_HOOK=1` khi máy không có `.env`. Theo `CLAUDE.md`: chạy
`impact({target, direction:"upstream"})` **trước** khi sửa bất kỳ hàm nào, và
`detect_changes()` trước khi commit.

---

## 3. Nhóm A — cổng kiểm

### VC-1 — `h2 0.3.27`, RUSTSEC-2026-0258

> **✅ Trạng thái 25/08/2026 (phiên thi công) — VC-1 ĐÃ ĐÓNG.** Làm đúng đường vá ghi ở
> đây: `reqwest` 0.11 → **0.12.28**, `teloxide` 0.13 → **0.17.0**, giữ nguyên hyper 0.14
> cho server OpenAI. Đổi mã duy nhất: `Requester::get_file` ở teloxide 0.17 nhận
> `FileId` theo giá trị (`telegram.rs#process_voice_message`). Bằng chứng đo cùng ngày:
> `Cargo.lock` giờ là **h2 0.4.19** (≥ 0.4.16 đã vá); `cargo tree -i h2@0.3.27` → *did not
> match any packages*; `cargo deny check …` → **exit 0** (`advisories ok, licenses ok,
> sources ok`); `cargo test --workspace` **587 pass · 1 fail** (đúng 1 fail đỏ sẵn của
> VC-2); clippy exit 0 · 0 warning; fmt xanh. Bot Telegram build và qua được unit test,
> **chưa chạy thật** (máy không có `TELEGRAM_BOT_TOKEN`).
**Bằng chứng.** `cargo deny check -W unmaintained -W unsound advisories` → đúng **một**
`error[vulnerability]`, chín mục còn lại là `warning[unmaintained]` đã được `-W` cho qua
(bincode, paste, proc-macro-error, rustls-pemfile, năm crate `unic-*`). Advisory
[RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258): h2 xếp hàng vô hạn
các khung DATA rỗng ⇒ tăng bộ nhớ không giới hạn hoặc panic khi tràn độ dài. Bản vá:
`h2 >= 0.4.16`. Không có bản vá nào trong dải 0.3.

Run CI thật [`32824625512`](https://github.com/DuongNAD/LIVA/actions/runs/32824625512):
13 bước đầu xanh, **FAILURE tại `cargo-deny`**, các bước sau không chạy vì fail-fast.

**Đường vá — rẻ hơn nhiều so với phương án ghi ở phiên trước.** Cây phụ thuộc:

```
h2 0.3.27 ← hyper 0.14.32 ← hyper-tls 0.5 ← reqwest 0.11.27
          ← reqwest 0.11.27 (phụ thuộc trực tiếp)
```

`liva-native-core` khai báo `hyper 0.14` với `features = ["server","http1","tcp","stream"]`
— **không có `http2`** (`liva-native-core/Cargo.toml`). Nên hyper *tự nó*
không kéo h2 về. h2 vào cây **chỉ vì `reqwest 0.11` bật hộ feature `http2` của hyper**
(feature unification của Cargo). Bỏ `reqwest 0.11` là cạnh đó biến mất.

⇒ **Không cần di trú `hyper 1.x`.** Chỉ cần:

1. `reqwest` `0.11` → `0.12` trong `liva-native-core/Cargo.toml` (giữ features `stream`, `multipart`).
2. `teloxide` `0.13` → `0.17` — đã kiểm: teloxide 0.17.0 nằm trên `reqwest 0.12.x`
   (`~/.cargo/registry/.../teloxide-0.17.0/Cargo.toml`), còn `teloxide-core 0.10.1` mà bản
   0.13 kéo về thì vẫn ở `reqwest 0.11`.
3. **Giữ nguyên `hyper 0.14`** cho server tương thích OpenAI ở `openai_api.rs`. Không phải
   viết lại `Body::wrap_stream`, không phải thêm `hyper-util`/`http-body-util`.

**Đổi lại:** cây sẽ có hai major hyper song song (0.14 cho server, 1.x qua reqwest) — build
to hơn, nhưng 0 advisory. Nếu sau này muốn dọn thì đó là **một mục khác**, không gộp vào đây.

**Rủi ro cần lường.** teloxide 0.13 → 0.17 là bốn minor, API bot có thể đổi
(`telegram.rs`, 747 dòng). Nếu vỡ quá nhiều thì phương án lùi: chỉ nâng `reqwest` lên 0.12
và tìm bản `teloxide` gần nhất còn tương thích — h2 0.3 vẫn còn qua teloxide, tức **chưa
đóng được VC-1**; khi đó ghi rõ trạng thái thay vì im lặng.

**Nghiệm thu.**
- `cargo deny check -W unmaintained -W unsound advisories licenses sources` → **exit 0**.
- `cargo tree -i h2@0.3.27` → *nothing to print*.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check` không hồi quy.
- Bot Telegram còn build và test được; nếu không có token để chạy thật thì **ghi rõ "chưa chạy thật"**, đừng suy ra "chắc chạy".

> 🚫 **Không thêm mục `ignore` vào `deny.toml` để làm nó xanh.** Đó là dập một cảnh báo
> thật bằng một dòng cấu hình. Nếu thật sự không vá được thì để đỏ và ghi lý do ở đây.

### VC-2 — `cargo test` đỏ ngầm từ 07/08/2026

**Bằng chứng.** `cargo test --workspace` → 571 pass, **1 fail**:
`preflight::tests::n_gpu_layers_bang_0_khong_bao_gio_la_xanh`.

Đây **không phải** "thiếu model nên đỏ, chấp nhận được". Đây là **một unit test đọc đĩa**.
`liva-native-core/src/preflight.rs#muc_vision` rẽ nhánh theo
`crate::configured_router_model_path().is_none_or(|p| !p.is_file())`:

- **Có** file model → thông điệp chứa `LIVA_LLM_N_GPU_LAYERS` — đúng thứ test khẳng định.
- **Không** có → thông điệp `"CHƯA CÓ MODEL nên chưa tính được…"` — test đỏ.

`liva-native-core/src/paths.rs#configured_router_model_path` đọc `data/liva-config.json`
rồi ghép đường dẫn; trả `None` khi provider không phải `local`. Cả hai ngả đều rơi vào
nhánh "chưa có model", nên **không có cấu hình nào cứu được test này khi thiếu file model**.

**Vì sao chưa ai thấy — và vì sao nó sẽ nổ ở CI.**

| Mốc | Sự kiện |
|---|---|
| `4de56c38` | Viết test, khi ấy `muc_vision` chưa đọc đĩa |
| `260c643` (01/08/2026) | Lần cuối CI được xác nhận **25/25 xanh** |
| **`95d641ab` (07/08/2026)** | Thêm nhánh `CHƯA CÓ MODEL` ⇒ test thành phụ thuộc môi trường |
| 25/08/2026 | Run CI đầu tiên của nhánh chết ở bước 9 (`cargo-deny`), **không tới bước 20** (`cargo test`) |

CI **không tải model** — đã grep toàn bộ `.github/workflows/test.yml`, không có bước
`setup:models` nào. Nên bước 20 sẽ đỏ y hệt ngay khi VC-1 mở đường cho nó chạy.

**Việc.** Bỏ phép đọc đĩa ra khỏi `muc_vision`: đưa "có model hay không" thành **tham số**,
để hàm thuần và test quyết định được cả hai nhánh. Điểm gọi thật tự tra đĩa rồi truyền vào.
Đổi chữ ký ⇒ **bắt buộc** `impact({target: "muc_vision", direction: "upstream"})` trước khi sửa.

**Nghiệm thu.**
- `cargo test` xanh **trên máy không có file model nào** (`find models -name "*.gguf"` = 0).
- Có **hai** test: một cho nhánh có model (khẳng định `LIVA_LLM_N_GPU_LAYERS`), một cho
  nhánh thiếu model (khẳng định `CHƯA CÓ MODEL`). Cả hai chạy được ở mọi máy.
- Hành vi lúc chạy thật **không đổi**: `--preflight` trên máy thiếu model vẫn in đúng thông
  điệp "CHƯA CÓ MODEL", trên máy có model vẫn in đúng thông điệp GPU.

> 🚫 **Không hạ ngưỡng khẳng định để làm test xanh**, và cũng **không** giải bài này bằng
> `npm run setup:models`. Tải model làm test xanh trên *một* máy; nó vẫn đỏ ở CI và ở mọi
> clone sạch. Cảnh báo gốc ở [MV-7](viec-con-lai-mac-v2.md) đúng về tinh thần —
> tiền đề "đây là chỉ báo môi trường có chủ đích" mới là chỗ sai.

> **✅ Trạng thái 25/08/2026 (phiên thi công) — VC-2 ĐÃ ĐÓNG.** Làm đúng hướng: `muc_vision`
> nhận thêm tham số thuần `co_model` (không đọc đĩa trong thân hàm); điểm gọi thật (`run`)
> tra `configured_router_model_path().is_some_and(is_file)` rồi truyền vào. Test cũ tách thành
> **hai**: `…co_model_thi_khuyen_ep_gpu_layers` (khẳng định `LIVA_LLM_N_GPU_LAYERS`) và
> `…thieu_model_thi_do_model_chat` (khẳng định `CHƯA CÓ MODEL`). **Đột biến** đảo điều kiện
> nhánh ⇒ đúng 2 test đó đỏ; hoàn nguyên ⇒ preflight **10/10** xanh trên máy không có file
> model nào. Hành vi chạy thật không đổi: cùng một phép tra đĩa, chỉ chuyển chỗ gọi.

### VC-3 — Ba test vault đỏ trên macOS — đã truy được nguyên nhân

**Bằng chứng.** `npm test -w obsidian-llm-wiki` → 24 pass, **3 fail**, tất cả trong
`teamwork_projects/obsidian_llm_wiki/tests/verification-challenger.test.ts`.
[MV-2](viec-con-lai-mac-v2.md) ghi "lệch nền hay lỗi thật thì chỉ runner Windows trả lời
được — chưa suy ra gì". **Đã trả lời được mà không cần Windows. Cả ba là lỗi của phép kiểm,
không phải lỗ hổng.**

**(a) Hai test symlink-loop — mock không bao giờ bắn.**

`tempVaultPath = mkdtemp(os.tmpdir()/…)` → trên macOS là `/var/folders/…`. Mock `lstatSync`
khoá theo `path.resolve(tempVaultPath, 'Skills/self_loop.md')`, tức `/var/folders/…`.
Nhưng `teamwork_projects/obsidian_llm_wiki/src/vault.ts#validateAndResolvePath` canonicalise
vault root bằng `fs.realpathSync` → `/private/var/folders/…`, vì trên macOS `/var` **là một
symlink** tới `/private/var`. Hai chuỗi không khớp ⇒ mock trả `lstatSync` thật ⇒ không có
symlink ⇒ không có vòng lặp ⇒ không throw ⇒ test đỏ.

Kiểm chứng một dòng trên máy này:

```bash
node -e "const os=require('os'),fs=require('fs'),path=require('path');const d=fs.mkdtempSync(path.join(os.tmpdir(),'x-'));console.log(d);console.log(fs.realpathSync(d));fs.rmSync(d,{recursive:true})"
```

**Sửa:** khoá mock bằng `fs.realpathSync(tempVaultPath)` thay vì `tempVaultPath`. Chỉ đụng
file test.

**(b) Test UNC — khẳng định gắn chặt vào Windows.**

Đầu vào `\\localhost\c$\Windows\win.ini`. Trên POSIX, `\` là **ký tự tên file hợp lệ**, nên
`path.resolve` giữ nguyên chuỗi đó **bên trong** vault ⇒ đi qua kiểm bao hàm ⇒ `lstat` trả
`ENOENT` ⇒ `File not found`. **Không có gì thoát ra khỏi vault** — đó là hành vi đúng trên
macOS, chỉ là thông điệp khác. Test anh em ngay phía trên (khối `complexTraversals`) đã chấp
nhận `/Access denied|File not found/`; test này thì chưa.

**Sửa:** cho khẳng định biết nền — `Access denied` trên `win32`, `Access denied|File not found`
ở nơi khác. Chỉ đụng file test.

**(c) Việc thật sự đáng làm: CI không có job macOS.**

`.github/workflows/test.yml` chỉ có `runs-on: windows-latest`, và trigger chỉ
`push`/`pull_request` vào `main`/`master`. Ba lỗi ở trên là **đúng loại lỗi mà chỉ runner
macOS bắt được** — và với cấu hình hiện tại sẽ không bao giờ bắt được. Trên một nhánh có
mục đích là port macOS, đó là khoảng trống lớn nhất trong hạ tầng kiểm.

**Nghiệm thu.**
- `npm test -w obsidian-llm-wiki` → **27/27** trên macOS, và vẫn 27/27 trên Windows
  (nếu không có máy Windows thì để job CI mới trả lời, và **ghi rõ là chưa tự kiểm**).
- Có job `runs-on: macos-latest` chạy ít nhất: `docs-check`, `docs-citations`, `npm ci`,
  ESLint, `vue-tsc`, test UI, knowledge vault, `cargo test`. **Không** cần chạy lại toàn bộ
  25 bước — llama.cpp + LLVM trên macOS runner là một mục riêng, đừng gộp.
- Ghi rõ trong `docs/02-van-hanh/04-kiem-thu-va-ci.md` bước nào chạy trên nền nào.

> ⚠️ **Không "sửa" `vault.ts` cho ba test này.** Mã sản phẩm đang đúng: `validateAndResolvePath`
> canonicalise root, chặn traversal, phát hiện vòng symlink qua tập `visited`, và kiểm bao hàm
> **hai lần** (trước và sau khi giải symlink). Sửa mã sản phẩm để chiều một cái mock hỏng là
> đổi một test đỏ lấy một lỗ hổng thật.

> **✅ Trạng thái 25/08/2026 (phiên thi công) — VC-3 ĐÃ ĐÓNG trên macOS; Windows chưa tự kiểm.**
>
> - **(a)** Hai test symlink-loop: mock khoá bằng `fs.realpathSync(tempVaultPath)` thay vì root
>   thô — đúng một dòng mỗi test, chỉ đụng file test.
> - **(b)** Test UNC: khẳng định biết nền — `win32` đòi `Access denied`, nền khác nhận
>   `/Access denied|File not found/`. Chỉ đụng file test.
> - Kết quả máy này: `npm test -w obsidian-llm-wiki` → **27/27** (trước: 24 pass · 3 fail).
> - **(c)** Job `test-macos` (`macos-latest`) đã thêm vào `.github/workflows/test.yml` với đúng
>   9 bước gọn (docs-check · docs-citations · npm ci · ESLint · vue-tsc · UI coverage · vault
>   typecheck+test · cargo test); phân bổ nền ghi ở
>   [04-kiem-thu-va-ci.md §2](../02-van-hanh/04-kiem-thu-va-ci.md).
> - **Chưa tự kiểm trên Windows**: job macOS mới sẽ trả lời qua run CI đầu tiên của nhánh này;
>   chưa có runner Windows cục bộ để đối chiếu — ghi rõ thay vì suy ra.
> - **Kiểm bằng đột biến**: hỏng lại neo realpath của test vòng-lặp đơn ⇒ đúng test đó đỏ
>   (1 failed / 10), hoàn nguyên ⇒ 27/27.

---

## 4. Nhóm B — thoại real-time

> **Trạng thái trung thực trước khi đọc tiếp.** [Voice SLO §3](../05-chat-luong/voice-slo.md)
> liệt kê sáu SLO và cả sáu đều ghi "chưa đo trên Tauri release". Bốn mục dưới đây **không**
> dựa trên phép đo độ trễ nào — chúng đọc ra từ mã nguồn. Đó là điểm của VC-4: chừng nào
> chưa có số, mọi tối ưu ở VC-5…VC-7 đều không nghiệm thu được.

### VC-4 — Đo lượt thoại: bốn mốc `tracing`

**Bằng chứng.** `Instant::now()` **duy nhất** trong toàn bộ `liva-native-core/src/webrtc/`
nằm trong một unit test (`pipeline.rs`, khối `#[cfg(test)] mod outbound_tests`). Đường chạy
thật không có mốc thời gian nào. Nghĩa là:

- SLO "Turn latency: VAD `SpeechEnd` → speaker frame đầu tiên" **không đo được**.
- SLO "Barge-in stop", "TTS TTFS", "Drop/backpressure" cũng vậy.
- [Gate nâng capability lên `working`](../05-chat-luong/voice-slo.md) đòi p50/p95 cho ba
  trong số đó ⇒ **không thể đạt** ở trạng thái hiện tại.

**Việc.** Bốn mốc, structured field, gắn `turn_epoch` vốn đã có sẵn trong
`liva-native-core/src/webrtc/pipeline.rs#WebRTCActor::run`:

| Mốc | Điểm ghi | Đo cái gì |
|---|---|---|
| `vad_end` | `handle_vad_end` lúc vào | Gốc thời gian của lượt |
| `stt_done` | `handle_stt_completed` | Thời gian giải mã STT |
| `first_token` | Token đầu tiên **nhận được từ `text_rx`** trong `spawn_tts_receiver` | TTFT nhìn thấy được |
| `first_speaker_frame` | Frame đầu tiên qua `VoiceOutbound::blocking_send_speaker_if_current` | **TTFA** — con số người dùng cảm nhận |

> ⚠️ **Đính chính 25/08/2026 (sau phản biện của phiên thi công).** Bản đầu của bảng này ghi
> `first_token` = "mảnh không rỗng đầu tiên **vào `TtsChunker`**". **Sai**, vì hai lý do đo
> được:
>
> 1. **Đo sau `AvatarSpeechFilter` là đo nhầm thứ.** `pipeline.rs#push_tts_token` chỉ đẩy vào
>    chunker phần bộ lọc *nhả ra*. Bộ lọc **đệm tag điều khiển qua nhiều token** — test hiện
>    có `avatar_control_bi_loc_truoc_khi_chia_clause_tts` chứng minh: `"[wa"` trả rỗng, phải
>    tới `"ve]Xin chào."` mới có chữ. Câu trả lời mở đầu bằng `[happy]` sẽ bị tính thêm cả
>    quãng sinh tag vào **thời gian LLM**, làm hỏng phép phân rã.
> 2. **Luật "không rỗng" là thừa ở điểm đó.** Nhịp tim chuỗi rỗng **không bao giờ tới được**
>    `text_rx`: `agent/graph/pipeline.rs#send_llm_chunk_if_current` đã trả sớm với
>    `if chunk.is_empty()`. Viết một test khẳng định "chuỗi rỗng không khởi động bộ đếm" ở
>    đây là viết một test **không thể đỏ vì lý do đúng**.
>
> ⇒ Đo tại **`text_rx.blocking_recv()` trả về token đầu tiên**. Đó là điểm tương đương với
> "TTFT nhìn thấy" của `liva-native-core/src/bin/ttft_bench.rs`, và giữ được ý nghĩa của
> quãng `first_token → first_speaker_frame` (tổng hợp TTS + chờ đủ mẩu). Vẫn giữ một phép
> kiểm `!token.is_empty()` rẻ tiền làm lá chắn — nhưng **đừng** dựng test quanh nó.

**Hai cái bẫy khi nối dây — cả hai đều im lặng nếu làm sai.**

1. **`turn_epoch` phải lấy SAU `cancel_active_operations()`.** `handle_vad_end` gọi
   `cancel_active_operations()` **ngay dòng đầu**, và hàm đó `session_id += 1`. Ghi mốc gốc
   ở đầu hàm rồi ghép với epoch đọc *trước* lệnh đó thì mọi mốc sau sẽ lệch epoch và bị
   loại hết — không lỗi, không cảnh báo, chỉ là một tệp log không có lượt nào hợp lệ. Đúng:
   lấy `Instant::now()` ở đầu hàm (đó mới là t0 thật của lượt), nhưng **ghép với
   `self.session_id` đọc sau khi huỷ**.
2. **`spawn_tts_receiver` có HAI điểm gọi.** Ngoài đường thoại (`spawn_llm_and_tts`), nó còn
   được gọi từ `handle_speak_text` — đường LIVA nói thẳng một câu, **không có `vad_end` nào
   đứng trước**. Phải quyết định tường minh: đường đó **không phát mốc** đo theo gốc lượt.
   Để nguyên thì nó sẽ đo từ gốc của một lượt cũ và cho ra một con số vô nghĩa nhưng trông
   hợp lệ.

**Ràng buộc.**
- Chỉ `tracing`, **không** thêm bảng DB. Sổ `turn_telemetry` là
  [U21](../03-danh-gia/05-nang-cap-toan-dien.md) và có điều kiện chen ngang riêng — VC-4
  là tầng dưới của nó, không phải bản thay thế.
- **Không log transcript hay audio.** [Voice SLO §6](../05-chat-luong/voice-slo.md) cấm, và
  đây là dữ liệu người dùng.
- Chi phí thêm ở p50 phải **< 5 ms** — cùng ngưỡng U21 đặt ra.

**Bộ gom số phải là script Node kèm test fixture, không phải một bin Rust.** Lý do không
phải sở thích: `scripts/*.test.mjs` chạy trong bước CI "Run Script-Adjacent Node Tests", còn
**binary probe Rust KHÔNG nằm trong CI** — CI chạy `cargo test`, không chạy probe. Đó chính là
cách `voice_stress` ngồi đỏ sẵn ở HEAD mà không ai biết, ghi ở
[§U7 backlog](../03-danh-gia/05-nang-cap-toan-dien.md). Một bộ gom số không ai kiểm sẽ trôi
lệch đúng như vậy. Bắt buộc có `scripts/<tên>.test.mjs` với **log mẫu cố định** để phần phân
tích cú pháp có cổng giữ.

**Nghiệm thu — tách hai vế, vế B chưa đóng được trên máy hiện tại.**

*VC-4a — đóng được ngay, không cần model:*
- Bốn mốc có mặt trên đường chạy thật, mỗi mốc có `turn_epoch` và `elapsed_ms` là trường
  `tracing` có cấu trúc.
- Bộ gom số chạy trên **log fixture** ra đúng p50/p95 đã biết trước; `node --test` xanh.
- Bộ gom **từ chối** lượt thiếu mốc và **nói ra** số lượt bị loại, không âm thầm bỏ qua.
- Test: mốc chỉ phát **một lần** mỗi lượt; đường `handle_speak_text` **không** phát mốc theo
  gốc lượt; epoch ghép đúng sau `cancel_active_operations()`.
- `cargo test` không hồi quy; `cargo clippy --all-targets --message-format=short` 0 warning;
  `cargo fmt --all -- --check` xanh.

*VC-4b — chờ có model, KHÔNG được đánh dấu xong bằng suy luận:*
- Một lượt thoại thật in đủ bốn mốc, đọc được bằng `RUST_LOG`.
- Điền ít nhất hai hàng của [Voice SLO §3](../05-chat-luong/voice-slo.md) bằng **số thật**,
  kèm commit + model manifest + env override. **Không điền target trước khi có baseline.**
- Chừng nào chưa chạy được: ghi thẳng "chưa đo được — `models/` trống trên máy đo" vào hàng
  đó. Đó là câu trả lời hợp lệ; một con số đoán thì không.

> Chi phí thêm phải **< 5 ms ở p50** — đo bằng cách so `cargo test` trước/sau, hoặc bằng
> `ttft_bench` nếu có model. Chưa đo được thì lập luận từ mã nguồn (chỉ `Instant::elapsed()`,
> không khoá, không cấp phát) và **ghi rõ là lập luận, không phải phép đo**.

> **✅ Trạng thái 25/08/2026 (phiên thi công) — VC-4a ĐÃ XONG, VC-4b còn mở.**
>
> - Bốn mốc đã có trên đường chạy thật: `TurnMilestone` trong
>   `liva-native-core/src/webrtc/pipeline.rs` — `vad_end` lúc vào `handle_vad_end`
>   (epoch ghép **sau** `cancel_active_operations()`; hàm nay *trả* epoch đã tăng,
>   sai thứ tự là vô lý về mặt cấu trúc), `stt_done` ở `handle_stt_completed`,
>   `first_token` tại token đầu tiên **nhận từ `text_rx`** (đúng đính chính; đường
>   `handle_speak_text` truyền gốc `None` — không phát mốc), `first_speaker_frame`
>   ngay sau `blocking_send_speaker_if_current`. Chi phí: chỉ `Instant::elapsed()`,
>   không khoá, không cấp phát — **lập luận từ mã nguồn, chưa phải phép đo**.
> - Bộ gom số: [scripts/voice-turn-latency.mjs](../../scripts/voice-turn-latency.mjs)
>   kèm test fixture
>   [scripts/voice-turn-latency.test.mjs](../../scripts/voice-turn-latency.test.mjs)
>   (log mẫu cố định, p50/p95 tính tay trước): `node --test` **6/6 xanh**. Đã cắm
>   vào CI bước "Run Script-Adjacent Node Tests" qua `npm run test:voice-turn` —
>   lưu ý bước này **không glob** `scripts/*.test.mjs`, nó gọi tên từng script npm;
>   thêm file mà quên thêm dòng là cổng không tồn tại (đúng kiểu `voice_stress`).
> - Cổng đo 25/08/2026, cùng cây làm việc sau thay đổi: `cargo fmt --all -- --check`
>   ✅; `cargo clippy --all-targets --message-format=short` exit 0 · **0 warning**;
>   `cargo test --workspace` **574 pass · 1 fail** — đúng 1 fail đã biết của VC-2
>   (`preflight::tests::n_gpu_layers_bang_0_khong_bao_gio_la_xanh`), không hồi quy
>   mới (571 cũ + 3 test mốc mới = 574 pass).
> - **VC-4b chưa đóng**: chưa đo được — `models/` trống trên máy này, chưa có lượt
>   thoại thật in đủ bốn mốc. Hai hàng Voice SLO §3 vẫn giữ "chưa đo"; đừng điền số
>   bằng suy luận.
>
> **Bổ sung 25/08/2026 (sau kiểm lại):** log thật có ANSI — tracing-subscriber
> 0.3.23, khối tô trường trong `fmt/format/mod.rs` (dòng 1332–1338) bọc tên trường
> italic và dấu '=' dimmed,
> hai lần paint riêng, nên chuỗi `turn_epoch=` không bao giờ nguyên vẹn trong log,
> kể cả đổ ra file (không ai gọi `.with_ansi(false)`). Bộ gom số giờ **lột ANSI
> trước khi khớp**; fixture có hẳn một lượt dựng theo đúng khuôn đó (tiền tố chép
> từ log thật của binary debug đo cùng ngày; dòng mốc dựng tay vì không có model),
> kèm test hồi quy "dòng log THẬT có ANSI vẫn parse đúng" — test này ĐỎ trên bản
> parser chưa lột ANSI. Thêm hàng phân rã "Huỷ lượt cũ + flush" (elapsed_ms của
> `vad_end`) để bốn hàng cộng đúng bằng turn latency. `TurnMilestone` bỏ
> `Clone/Copy` — bản sao vô tình không được phép phát lại mốc. Fixture có **hai
> lượt `vad_end` khác 0** (12 và 35 — đúng hình log thật, vì elapsed_ms của
> `vad_end` chính là chi phí huỷ/flush) và test khẳng định `decompose(turn)` bằng
> số tính tay từng lượt; vòng kiểm "bốn hàng cộng = turn latency" kiểu hằng đẳng
> thức đã bỏ. **Kiểm bằng đột biến**: bỏ trừ `vad_end` ở `summarize` → 1 test đỏ;
> bỏ trừ ở `decompose` → 2 test đỏ; hoàn nguyên → 8/8 xanh. Cổng chạy lại:
> fmt ✅ · clippy exit 0 · 0 warning · `cargo test --workspace` **587 pass · 1 fail**
> (đúng 1 fail VC-2 cũ, lần này chạy trọn cả các bin integration) · node --test **8/8**.

### VC-5 — Barge-in tự cắn, và AEC nhiều khả năng không hội tụ

**Bằng chứng — chuỗi khép kín, mỗi mắt xích đọc được từ mã nguồn.**

1. UI **vẫn bắn mic** khi LIVA đang nói: van gửi `OP_MIC_IN` mở khi
   `state === 'ACTIVE' || state === 'PROCESSING'`
   (`liva-ui/src/composables/useVoicePipeline.ts#useVoicePipeline`). Đúng ý đồ — barge-in cần thế.
2. `echoCancellation` của trình duyệt **không phủ** đường ra AudioContext trong webview Tauri
   — chính mã nguồn ghi nhận điều này trong khối "Chống tự nghe" của cùng file. Bộ chặn hiện
   có (`speakerActive` + `WAKE_WORD_ECHO_TAIL_MS`) chỉ khoá **bộ wake-word ở chế độ PASSIVE**,
   không chạm nhánh `OP_MIC_IN`.
3. Server: AEC **tắt mặc định** —
   `liva-native-core/src/webrtc/session.rs#VoiceRuntimeConfig::from_env` đọc
   `LIVA_AEC_ENABLED` với mặc định `false`.
4. `liva-native-core/src/wake.rs#WakeGate::is_awake` trả **`true` vô điều kiện** ở
   `WakeMode::Off` — và `Off` là mặc định.
5. `liva-native-core/src/webrtc/pipeline.rs#WebRTCActor::handle_vad_start` gọi
   `cancel_active_operations()` **không điều kiện**.

⇒ Giọng LIVA vọng từ loa vào mic đủ để LIVA **tự huỷ lượt của chính mình**. Có nổ hay không
tuỳ phòng, loa và âm lượng; điều chắc chắn là **không có gì trong mã nguồn ngăn nó**. Đeo tai
nghe thì triệu chứng biến mất — đó là lý do dễ tưởng là ổn.

**Và bật AEC lên nhiều khả năng vẫn chưa đủ.**
`liva-native-core/src/webrtc/aec.rs#SelfEchoCanceller::process_capture` rút **một** khung
render cho mỗi khung capture, và **chỉ khi** hàng đợi render đủ 160 mẫu. Hàng đợi lại được
nạp bởi `push_render` **ở thời điểm tổng hợp TTS**, không phải thời điểm phát. Hệ quả: giữa
hai clause hàng đợi cạn ⇒ khung render bị **bỏ qua**, trong khi loa thật vẫn đang phát khoảng
lặng đó. Mốc thời gian far-end bị **nén lại** so với thực tế và trôi dần suốt lượt. AEC3 ước
lượng trễ trên một tham chiếu như vậy sẽ rất khó bám.

⚠️ **Đoạn trên là suy luận từ mã nguồn, chưa đo.** Phải kiểm trước khi sửa — xem nghiệm thu.

**Ba test AEC hiện có không thể đỏ vì lý do đúng.** Cả ba chỉ khẳng định *độ dài đầu ra bằng
đầu vào* và *mọi mẫu là số hữu hạn*. Một `SelfEchoCanceller` chép thẳng đầu vào ra đầu ra vẫn
qua sạch cả ba. Tài liệu ghi AEC "✅ Đã ship, có test" là ghi quá.

**Việc, theo đúng thứ tự.**

1. **Test có răng trước.** Thêm test: render = tín hiệu, capture = **cùng tín hiệu đó có trễ**,
   khẳng định năng lượng đầu ra **giảm rõ rệt** so với đầu vào. Test này phải **đỏ** nếu ai gỡ
   AEC ra. Chạy nó trên mã nguồn hiện tại trước khi sửa gì — kết quả của nó là **phép đo** cho
   giả thuyết "không hội tụ" ở trên.
2. **Tham chiếu far-end liên tục.** `push_render` phải sinh ra một dòng liên tục theo thời gian
   thực: chèn im lặng khi không phát, thay vì để hàng đợi cạn rồi bỏ qua khung render. Cân nhắc
   cả đường đi ngược: cấp render từ điểm *phát* thay vì điểm *tổng hợp*.
3. **Chỉ khi hai bước trên xanh** mới bàn tới việc đổi mặc định `LIVA_AEC_ENABLED`. Và đổi mặc
   định là một quyết định cần số đo CPU kèm theo, không phải một dòng sửa.

**Nghiệm thu.**
- Test giảm năng lượng echo **xanh**, và **đỏ** khi cố tình vô hiệu hoá AEC (tự kiểm bằng cách
  tạm cho `process_capture` trả thẳng đầu vào).
- Có số CPU cho nhánh AEC bật, đo trên cùng máy với nhánh tắt.
- Ghi lại kết quả bước 1 **dù nó bác bỏ giả thuyết** — "AEC hội tụ tốt, giả thuyết nén thời
  gian sai" là một kết quả hợp lệ và phải được ghi.
- Cập nhật bảng ở [Voice runtime §5](../03-he-thong-con/voice.md) và
  [Voice SLO §2](../05-chat-luong/voice-slo.md) nếu mặc định đổi.

### VC-6 — Mẩu TTS đầu tiên của mỗi lượt quá dài

**Bằng chứng.** `liva-native-core/src/tts/mod.rs#TtsChunker::push` cắt khi gặp:
`.` `!` `?` (luôn luôn) · `,` `;` `:` `—` (**chỉ khi đã đủ 6 từ**) · trần 25 từ.
Không có khái niệm "mẩu đầu tiên của lượt".

Với câu tiếng Việt điển hình — *"Chào bạn, mình có thể giúp gì cho bạn?"* — dấu phẩy rơi vào
từ thứ hai nên **không cắt**; mẩu đầu tiên chỉ ra đời khi gặp `?` ở cuối. Tức TTS **phải chờ
LLM sinh xong cả câu** rồi mới bắt đầu tổng hợp. Persona chốt "1–3 câu", nên đây là hình dạng
phổ biến chứ không phải ca hiếm.

**Việc.** Cho `TtsChunker` biết mẩu nào là mẩu **đầu** của lượt, và chỉ với mẩu đó thì hạ
ngưỡng (dấu phẩy cắt từ **≥2 từ**, trần **9 từ** — xem đính chính dưới đây). Từ mẩu thứ hai trở
đi giữ nguyên luật hiện tại để ngữ điệu không vỡ. `TtsChunker::reset` đã tồn tại — dùng nó làm
ranh giới lượt.

> ✏️ **Đính chính 25/08/2026 (phiên thi công):** bản đầu viết "dấu phẩy cắt từ ≥3 từ". Sai so
> với chính nghiệm thu bên dưới: câu ví dụ "Chào bạn, mình…" có dấu phẩy sau từ **thứ hai**, nên
> ngưỡng 3 làm điều kiện "mẫu đầu ra đời sớm hơn" tự mâu thuẫn — đã triển khai ngưỡng **≥2**,
> trần mẫu-đầu **9 từ**.

**Nghiệm thu.**
- Test: chuỗi `"Chào bạn, mình có thể giúp gì cho bạn?"` đẩy vào theo từng token cho ra mẩu
  đầu tiên **sớm hơn** hiện tại, và tổng văn bản của mọi mẩu ghép lại **không đổi một ký tự**.
- Mẩu thứ hai trở đi cho kết quả **giống hệt** luật cũ — có test khẳng định điều đó.
- Với VC-4 đã xong: **TTFA p50 giảm**, đo trên cùng máy, cùng model, cùng prompt.
  Không có số này thì mục chỉ đóng ở mức "đúng theo test", ghi rõ như vậy.

> **Trạng thái 25/08/2026 (phiên thi công) — VC-6 ĐÃ CODE XONG, đóng ở mức "đúng theo test".**
>
> - `TtsChunker` có cờ `dang_la_mau_dau` (bật ở `new()`/`reset()`, tắt khi mẫu đầu ra đời);
>   ngưỡng mẫu-đầu: dấu phẩy ≥2 từ · trần 9 từ; từ mẩu thứ hai: đúng luật cũ ≥6/25.
> - Nghiệm thu từng dòng: câu ví dụ đẩy theo token ⇒ mẩu đầu ra đời tại token dấu phẩy
>   (index 2) thay vì "?" — ✅ test; tổng văn bản ghép lại nguyên văn — ✅ test; mẩu thứ hai
>   trở đi giống luật cũ — ✅ test; không hồi quy `AvatarSpeechFilter` — ✅ test cũ vẫn xanh;
>   cargo test/clippy/fmt — ✅. **TTFA p50 giảm — CHƯA ĐO ĐƯỢC** (`models/` trống), chờ VC-4b.
> - **Kiểm bằng đột biến**: xoá đặc quyền mẫu-đầu (ngưỡng = luật cũ) ⇒ test "mẩu đầu ra đời
>   sớm" đỏ; bỏ cờ flip sau mẫu đầu ⇒ test "mẫu thứ hai giữ luật cũ" đỏ. Hoàn nguyên ⇒
>   tts:: 87/87 xanh.
- Không hồi quy `AvatarSpeechFilter`: tag điều khiển bị cắt đôi giữa hai token vẫn phải ghép
  đúng (đã có test `avatar_control_bi_loc_truoc_khi_chia_clause_tts`).

### VC-7 — Kết lượt là 704 ms im lặng cố định

**Bằng chứng.** `liva-native-core/src/webrtc/vad.rs#VadConfig::from_env` đặt
`speech_end_threshold` mặc định **22 frame ≈ 704 ms** (frame 512 mẫu = 32 ms @16 kHz).
Đây là toàn bộ logic kết lượt hiện tại.

Smart Turn v3.2 đã port đầy đủ — feature recipe 80×800 log-mel, ngưỡng sigmoid — nhưng chạy
**shadow mode**: `liva-native-core/src/webrtc/turn_shadow.rs` chỉ ghi log, và
`liva-native-core/src/websocket.rs` gọi nó fire-and-forget sau khi VAD **đã** quyết định.
Lý do tắt là đúng: tiếng Việt là ngôn ngữ yếu nhất của model (81,27% so với 94,31% tiếng Anh
theo benchmark v3.0).

**Việc — cách dùng chỉ-có-lợi.** Cho classifier quyền **rút ngắn**, không bao giờ kéo dài:

- Nó nói "turn complete" ⇒ chốt sớm (ví dụ ~300 ms thay vì 704 ms).
- Nó nói "chưa xong", hoặc model không nạp được, hoặc suy luận lỗi ⇒ **chờ đủ 704 ms như cũ**.

Sai lệch tệ nhất là quay về đúng hành vi hôm nay. Đó là điều làm mục này khác với "bật
SmartTurn lên".

**Điều kiện vào.** Phải có [VC-4](#vc-4--đo-lượt-thoại-bốn-mốc-tracing) trước, nếu không thì
không chứng minh được nó thật sự rút ngắn. Và phải có **log shadow từ tiếng Việt thật** để
chọn ngưỡng — đường log đã sẵn sàng, chỉ cần bật `LIVA_TURN_SHADOW_ENABLED` và dùng.

**Nghiệm thu.**
- Turn latency p50 **giảm** so với baseline VC-4, trên cùng corpus.
- **Không** có ca nào bị cắt lời giữa câu nhiều hơn baseline — cần một corpus câu dài, ngập
  ngừng, có khoảng lặng giữa câu; đếm tay nếu chưa có công cụ.
- Mọi nhánh hỏng (model thiếu, suy luận lỗi, quá hạn) rơi về 704 ms, **có test**.
- Ngưỡng và số liệu ghi vào [Voice SLO §2](../05-chat-luong/voice-slo.md).

---

## 5. Cái KHÔNG nên làm trong phạm vi tài liệu này

1. **Đừng nâng model STT/TTS để "giảm độ trễ"** khi chưa có VC-4. STT tiếng Việt
   (Parakeet CTC) không streaming được vì CTC không nhân quả — đó là ràng buộc của model, ghi
   rõ ở `liva-native-core/src/stt/mod.rs#SttManager::feed_audio_inner`. Đổi model là việc lớn,
   và không có số đo thì không biết nó có phải điểm nghẽn hay không.
2. **Đừng nâng hàng loạt npm major** (pixi 6→8, typescript 6→7, three, jsdom). Đó là công việc
   riêng, và trộn vào đây thì không ai biết cổng nào đỏ vì mục nào.
3. **Đừng chạy `npm run setup:models` như một cách sửa VC-2.** Xem hộp cảnh báo ở mục đó.
4. **Đừng đụng `docs/03-danh-gia/`** trừ khi có số đo mới thật. Ở thư mục đó, tài liệu lỗi thời
   **làm đỏ build**.
5. **Đừng gộp VC-1 với VC-2 trong một commit.** Chúng đỏ ở hai bước CI khác nhau; gộp lại là
   mất khả năng biết cái nào sửa được cái gì.

---

## 6. Kho ứng viên đã sàng — rà nguồn mở 25/08/2026

Một lượt rà mã nguồn mở/bài báo đề xuất 12 ứng viên nâng cấp. Ghi lại **kết quả sàng**,
không phải danh sách việc: phần lớn bị loại, và **lý do loại mới là thứ có giá trị** — nó
ngăn lượt sau đề xuất lại y hệt. Đúng tinh thần [§9 backlog](../03-danh-gia/05-nang-cap-toan-dien.md).

Mọi dòng dưới đây đo tại `0a02b9cd` trên máy này, không trích từ báo cáo.

### 6.1. Ba ứng viên bị loại vì **đã có sẵn trong repo**

| Đề xuất | Thực tế đo được |
|---|---|
| "Thêm `sqlite-vec` để thay cosine brute-force" | **Đã có và đang gánh tải.** `sqlite-vec@0.1.9` nằm ở `package.json`; `liva-native-core/src/db.rs` dựng `CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[…])`, ghi bằng `vec_quantize_int8(?, 'unit')` và truy hồi KNN bằng `MATCH`. `scripts/check-installer-config.mjs` còn bắt buộc `vec0.dylib`/`vec0.dll` có trong bundle với SHA-256 trong manifest — thiếu nó là **không mở được DB, chặn khởi động**. Không có việc gì ở đây |
| "Lượng tử hoá int8/binary cho vector" | **Đã có** — chính `vec_quantize_int8` ở trên |
| "Kiến trúc truy hồi đa tín hiệu BM25 + vector" | Nguyên liệu **đã có cả hai**: cạnh vector như trên, cạnh từ khoá là FTS `MATCH` trên `content` trong cùng `db.rs`. Việc còn lại (nếu có) là **hợp nhất thứ hạng**, và đó là một mục cần số recall thật trước — không phải một dependency mới |

⇒ **Bài học ghi lại:** trước khi đề xuất một thư viện, `grep` tên nó trong `package.json`
và `Cargo.toml`. Ba trong mười hai ứng viên chết ở bước này.

### 6.2. Hai ứng viên có thật nhưng **lý do đưa ra sai**

**`llama-cpp-2` 0.1.151 → 0.1.154** — bản mới có thật (đã kiểm: `cargo info llama-cpp-2`
báo `latest 0.1.154`). Nhưng lý do "speculative decoding là đòn bẩy lớn nhất cho quãng
`first_token → first_speaker_frame`" **sai hai lần**:

1. **Sai quãng.** Speculative decoding tăng tốc *sinh token*, trong khi TTFT bị chi phối bởi
   *prefill*. Còn quãng `first_token → first_speaker_frame` phần lớn là **tổng hợp TTS + chờ
   đủ một mẩu** — thứ mà [VC-6](#vc-6--mẩu-tts-đầu-tiên-của-mỗi-lượt-quá-dài) đánh thẳng vào,
   rẻ hơn nhiều.
2. **Sai kiến trúc.** Speculative decoding cần một **draft model thứ hai** nạp song song. Giới
   hạn cốt lõi của tầng LLM hiện tại là *một engine / một context / một Mutex dùng chung*
   ([04-he-llm-va-prompt.md](../01-ban-ve/04-he-llm-va-prompt.md)). Đó không phải "patch bump".

⇒ Nâng phiên bản thì **được**, rẻ và nên làm khi CI xanh. Nhưng ghi đúng lý do: *bắt kịp
bản vá ngược dòng*, không phải *giảm TTFA*. Nhận nhầm công là cách một mục vô hại biến thành
một lời hứa không giữ được.

**`ort` → rc.13** — bản mới có thật, nhưng **điểm xuất phát bị ghi sai**: `Cargo.toml` khai
`2.0.0-rc.9` còn `Cargo.lock` đã ở **rc.11**, nên bước nhảy thật là rc.11 → rc.13. Vẫn là
breaking (kéo theo `ndarray` 0.15 → 0.17, chạm **mọi** điểm gọi ONNX: Parakeet, GTCRN, Silero,
Kokoro, VieNeu, smart-turn, wake model). Phần thưởng nêu ra là `ModelCompiler` giảm thời gian
khởi động — mà **thời gian khởi động chưa ai đo**. Tối ưu một thứ chưa đo là đúng loại việc
[§5](#5-cái-không-nên-làm-trong-phạm-vi-tài-liệu-này) chặn. **Hoãn tới khi có số.**

### 6.3. Ba ứng viên bị loại đúng — giữ lại làm tri thức phủ định

| Đề xuất | Vì sao loại |
|---|---|
| `parakeet-tdt-0.6b-v3` để "streaming hoá" STT tiếng Việt | Bảng WER của model **không có tiếng Việt**. Ràng buộc CTC không nhân quả ở `liva-native-core/src/stt/mod.rs` vẫn đứng nguyên |
| Moonshine cho tiếng Việt | Model ngoài tiếng Anh mang **giấy phép phi thương mại**; hỗ trợ tiếng Việt chưa rõ |
| Framework voice agent kiểu Pipecat / LiveKit Agents | Python, cloud-first orchestration. Lõi LIVA đã là Rust thuần chạy cục bộ — nhét vào là đi lùi kiến trúc |

### 6.4. Ứng viên tham chiếu, không phải phụ thuộc

- **sherpa-onnx** — đọc cách họ tổ chức streaming ASR + keyword spotting. ⚠️ Khẳng định "có
  model zipformer tiếng Việt" **chưa xác minh**; đừng lên kế hoạch dựa vào nó.
- **VieNeu-TTS ngược dòng** — LIVA đang vendor VieNeu tại `liva-native-core/src/tts/vieneu/`.
  Đối chiếu xem có bản vá nào chưa port ngược. Rẻ, và là việc *đọc*, không phải việc *sửa*.
- **`@pixiv/three-vrm`** `^3.5.2` → 3.5.5 — patch bảo trì. Đừng kỳ vọng hiệu năng; tối ưu
  avatar nằm ở render pipeline (U31 của backlog).
- **PGO + BOLT** cho binary release, **tokio-console** cho dev. Cả hai hợp lý, cả hai **cần
  baseline trước** — tức sau [VC-4b](#vc-4--đo-lượt-thoại-bốn-mốc-tracing).

### VC-8 — Lip-sync theo phoneme thay vì theo biên độ

**Đây là ứng viên duy nhất trong mười hai mục vừa qua được thăng lên thành việc.** Nó không
thêm phụ thuộc nào, không cần phép đo nào trước, và dùng thứ đã nằm sẵn trong repo.

**Bằng chứng.** Lip-sync hiện tại là **audio-driven**: `liva-ui/src/composables/use3DModel.ts`
tự ghi trong khối của nó là "Audio-Driven Lip-Sync (RMS Viseme Mapping)" — miệng mở theo
**năng lượng** của khung âm thanh. RMS không phân biệt được `a` với `m`: mọi âm to đều há
miệng, mọi âm nhỏ đều khép, nên khẩu hình không bao giờ khớp phụ âm môi.

Trong khi đó **phoneme đã được sinh ra rồi** ở phía lõi: mọi backend TTS đều đi qua G2P —
`liva-native-core/src/tts/g2p.rs` và `liva-native-core/src/tts/vieneu/g2p.rs`. Chuỗi phoneme
là đầu vào của chính bộ tổng hợp; nó đang bị vứt đi sau khi dùng.

**Việc.** Đưa chuỗi phoneme (kèm mốc thời gian tương đối trong mẩu) ra cùng đường với PCM
loa, rồi ánh xạ phoneme → viseme VRM ở `use3DModel.ts` thay cho RMS. Khung `OP_SPEAKER_OUT`
đã mang `turn_epoch` + `sample_rate`; cần quyết định tường minh là mở rộng khung đó hay dùng
một op-code điều khiển riêng — **đây là hợp đồng giao thức, phải chốt trước khi code**
([02-giao-thuc-ipc-va-websocket.md](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md)).

**Rủi ro thật cần lường.** Piper và VieNeu **không cùng bộ phoneme**, và đường Kokoro fallback
lại khác nữa. Ánh xạ cứng theo một backend sẽ vỡ khi fallback đổi backend giữa lượt — mà
`synthesis_plan` có đúng cơ chế fallback đó. Hoặc ánh xạ qua một tập viseme trung gian, hoặc
giới hạn phạm vi vào một backend và **nói rõ backend nào**.

**Nghiệm thu.**
- Khẩu hình phân biệt được ít nhất nhóm môi (`m`/`b`/`p`) với nguyên âm mở (`a`) — kiểm bằng
  mắt trên một câu tiếng Việt có cả hai, kèm ảnh chụp hai khung hình.
- Backend đổi giữa lượt (ép fallback) **không** làm khẩu hình đứng hình hay nhảy loạn.
- Không thêm phụ thuộc npm hay crate nào.
- Đường RMS cũ còn nguyên làm dự phòng khi không có chuỗi phoneme (ví dụ audio phát lại từ
  nguồn khác), và có công tắc chọn.
- `vue-tsc`, ESLint, `npm run test:coverage -w liva-ui` không hồi quy.

> **Thứ tự:** VC-8 **xếp sau** VC-1…VC-3. Nó độc lập với các cổng đang đỏ nên *chạy song song
> được*, nhưng đừng bắt đầu khi CI còn đỏ ở advisory — lúc đó không phân biệt được lỗi mới với
> lỗi sẵn có.

> **Trạng thái 25/08/2026 (phiên thi công) — VC-8 ĐÃ CODE, một dòng nghiệm thu CHƯA ĐO ĐƯỢC.**
>
> - **Hợp đồng đã chốt: op-code riêng `OP_VISME = 0x06`** (`webrtc/frame.rs`), KHÔNG mở rộng
>   khung `OP_SPEAKER_OUT`. Payload JSON `{"turn_epoch","base_seq_id","visemes":[{"v","t_ms"}]}`
>   gửi qua CÙNG kênh speaker ngay trước audio của mẩu (FIFO bảo đảm thứ tự); client cũ bỏ qua
>   opcode lạ an toàn (đã đọc router: nhánh `else { return; }`). Bảng opcode ghi đủ ở
>   [02-giao-thuc-ipc-va-websocket.md §5.3](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md).
> - **Tập viseme trung gian**: `aa/ee/ih/oh/ou + nil`, MỘT bảng IPA chung cho Piper lẫn VieNeu —
>   ký tự lạ về `nil` (miệng đóng) thay vì đoán sai, nên fallback đổi backend không vỡ bảng.
>   **Kokoro fallback không phát timeline** ⇒ client tự rơi về RMS. Phạm vi phoneme: VieNeu
>   (sea-g2p) và Piper (espeak IPA); `t_ms` là **phân bố đều** theo độ dài mẩu (VITS/codec không
>   nhả căn chỉnh per-phoneme — xấp xỉ có chủ đích, đủ phân biệt môi/nguyên âm).
> - Công tắc: `LIVA_LIPSYNC=phoneme` (mặc định `rms` = hành vi cũ). Không thêm crate/npm nào.
> - **Kiểm bằng đột biến** (bắt buộc theo luật test): Rust `'m'→Aa` ⇒ 2 đỏ; bỏ chia `/n`
>   timeline ⇒ 1 đỏ; client `currentViseme` trả cứng `"aa"` ⇒ 2 đỏ; bỏ whitelist viseme ⇒ 1 đỏ.
>   Hoàn nguyên ⇒ Rust viseme 4/4 xanh, UI phonemeLipSync 6/6 xanh.
> - **Nghiệm thu "kiểm bằng mắt trên câu tiếng Việt có m/b/p và a" — CHƯA ĐO ĐƯỢC**: máy này
>   `models/` trống, không chạy được lượt thoại thật. Đã bảo đảm bằng unit test mapping +
>   timeline + registry; cần xác nhận bằng mắt khi có model.


---

## Liên quan

- [Việc còn lại trên nhánh `mac-v2`](viec-con-lai-mac-v2.md) — MV-1…MV-13, hạ tầng nhánh và số đo run CI
- [Backlog nâng cấp U1–U33](../03-danh-gia/05-nang-cap-toan-dien.md) — năng lực sản phẩm và §9 "cái KHÔNG nên làm"
- [Master roadmap](roadmap.md) — mốc chiến lược; tài liệu này không đặt lại ưu tiên
- [Voice runtime](../03-he-thong-con/voice.md) — as-built cần cập nhật sau VC-5 và VC-7
- [Voice SLO](../05-chat-luong/voice-slo.md) — bảng ngưỡng và sáu SLO mà VC-4 mở khoá
- [Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) — 25 bước CI, nơi ghi job macOS mới của VC-3
- [Phát triển trên macOS](../02-van-hanh/07-macos-dev.md) — ba bẫy đã trả giá khi đưa cổng kiểm sang macOS
