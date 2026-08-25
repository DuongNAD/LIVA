---
title: "Việc còn lại trên nhánh mac-v2"
updated: 2026-08-25
commit: 4ae8bfb6
status: living
owns:
  - viec-con-lai-mac-v2
covers:
  - .github/workflows/test.yml
  - .gitignore
  - eslint.config.js
  - scripts/docs-inventory.mjs
  - scripts/docs-capabilities.mjs
  - scripts/e2e-gateway-ci.mjs
---
# Việc còn lại trên nhánh `mac-v2`

[⬆ Mục lục](../README.md) · [Master roadmap](roadmap.md) · [Backlog nâng cấp](../03-danh-gia/05-nang-cap-toan-dien.md)

## 0. Phạm vi — đọc trước, kẻo làm trùng

Tài liệu này **chỉ** sở hữu việc phát sinh từ nhánh `mac-v2`: hạ tầng, cổng kiểm,
vệ sinh repo và xác thực. Nó **không** phải backlog sản phẩm.

| Loại việc | Chủ sở hữu | Đừng làm gì |
|---|---|---|
| Năng lực sản phẩm (U2, U10, U12–U15, U17b, U21, U22, U27, U29, U32, U33) | [Backlog nâng cấp](../03-danh-gia/05-nang-cap-toan-dien.md) | Đừng chép mục U nào vào đây |
| Mốc chiến lược (D0.5, V0.1, I0.1, A31-05) | [Master roadmap](roadmap.md) | Đừng đặt lại thứ tự ưu tiên ở đây |
| Việc "không nên làm" | [Backlog §9](../03-danh-gia/05-nang-cap-toan-dien.md) | Đọc trước khi tự nghĩ ra việc |

Mục MV-10 → MV-13 dưới đây là **con trỏ**, không phải bản sao. Số đo thật nằm ở
tài liệu chủ; sửa ở đây mà không sửa ở đó là tạo ra hai sự thật.

## 1. Trạng thái đã đo — 25/08/2026 tại `f3d418a2`, trên macOS

⚠️ **Mọi số dưới đây đo trên macOS. Runner CI là `windows-latest`.** Đây là điều
quan trọng nhất trong tài liệu: xanh ở đây **không** chứng minh xanh ở CI.

| Cổng | Lệnh | Kết quả |
|---|---|---|
| Tài liệu | `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` | ✅ exit 0, 0 lỗi thời |
| Trích dẫn | `node scripts/docs-citations.mjs --max-unchecked=207` | ✅ exit 0, 0 neo hỏng |
| Typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` (trong `liva-ui/`) | ✅ 0 lỗi |
| ESLint | `npx eslint . --max-warnings 0 --no-warn-ignored` (trong `liva-ui/`) | ✅ 0 |
| Clippy | `cargo clippy --all-targets --message-format=short` | ✅ **0 warning** |
| Format | `cargo fmt --all -- --check` | ✅ 0 |
| Lỗ hổng npm | `npm audit --audit-level=high` | ✅ 0 vulnerabilities |
| Test Rust | `cargo test` | ⚠️ 571 pass · **1 fail** — xem [MV-7](#mv-7--models-trống-nên-một-test-đỏ) |
| Coverage UI | `npm run test:coverage -w liva-ui` | ✅ 80,86 % line |

**Tám bước CI chưa chạy ở bất kỳ đâu:** `cargo deny` · `cargo test -p liva-desktop` ·
`cargo check -p liva-desktop` · `cargo check --all-targets --features experimental` ·
build ba web client · validate knowledge vault · `npm run devkit:lint` ·
**e2e gateway trên Windows**.

---

## 2. Nhóm A — chặn hợp nhất

### MV-1 — CI chưa từng chạy trên nhánh này

**Bằng chứng.** `.github/workflows/test.yml` khai trigger `push: branches: [main, master]`
và `pull_request: branches: [main, master]`. Push `mac-v2` **không kích hoạt gì**.
Toàn bộ commit trên nhánh chưa qua một bước CI nào.

**Việc.** Mở PR `mac-v2` → `main` (draft cũng được). Đây là **cách duy nhất** chạy 25 bước.

**Nghiệm thu.** Có một lần chạy workflow với kết luận rõ ràng. Bước nào đỏ thì ghi lại
số thật vào [§1 của tài liệu này](#1-trạng-thái-đã-đo--25082026-tại-f3d418a2-trên-macos),
**đừng vá vội** — biết mình đỏ ở đâu có giá trị hơn một PR xanh giả.

### MV-2 — Tám bước CI chưa có bằng chứng nào

**Việc.** Hoặc chạy local, hoặc chấp nhận MV-1 trả lời thay. Cái rẻ nhất chạy trước:
`npm run devkit:lint`, validate knowledge vault, `cargo check -p liva-desktop`.

**Nghiệm thu.** Mỗi bước có một con số thật hoặc một dòng ghi rõ "chưa chạy".
**Không suy ra "chắc xanh".**

### MV-3 — Nhánh Windows của `e2e-gateway-ci.mjs` chưa chạy lại

**Bằng chứng.** `scripts/e2e-gateway-ci.mjs` nay chọn tên binary theo `process.platform`.
Nhánh macOS đã kiểm (8/8 đạt); **nhánh Windows của chính đoạn code đó chưa ai chạy lại**
sau khi sửa.

**Nghiệm thu.** e2e gateway xanh trên runner Windows — tức là MV-1 trả lời.

---

## 3. Nhóm B — cổng hỏng về cấu trúc — ✅ **ĐÃ ĐÓNG 25/08/2026**

Cả ba mục dưới đây được vá ngay trong ngày, ở `474421ff` và `2736fa05`. Giữ lại nguyên
văn phần **bằng chứng** vì đó mới là thứ có giá trị về sau — chế độ hỏng và cách nhận ra nó.


### ~~MV-4~~ ✅ — Hai cổng docs không bao giờ xanh được

**Bằng chứng.** `scripts/docs-inventory.mjs` chạy `git rev-parse --short HEAD` rồi ghi
sha đó vào front-matter file sinh ra. Chế độ `--check` sinh lại ở HEAD **mới** rồi so với
file lưu sha **cũ** ⇒ luôn khác ⇒ luôn đỏ sau mọi commit. `scripts/docs-capabilities.mjs`
cùng lỗi.

**Cách sửa — có tiền lệ trong repo.** `scripts/docs-check.mjs` sinh
`_meta/ban-do-code-tai-lieu.md` với `commit: auto` cứng, đúng vì lý do này.

**Đã sửa ở `474421ff`:** hai script sinh `commit: auto` thay vì sha, đúng theo tiền lệ.
Kiểm lại ở `4ae8bfb6`: `docs-inventory --check` và `docs-capabilities --check` **exit 0**,
và file sinh ra ghi `commit: auto` — tức là không còn drift theo HEAD nữa.

> ⚠️ Cổng này **không nằm trong CI** (CI chỉ chạy `docs-check.mjs` và `docs-citations.mjs`),
> nên nó phiền chứ không chặn build. Nhưng một cổng không bao giờ xanh được thì sớm muộn
> cũng bị bỏ qua — cùng họ với bẫy "always-green" mà `CLAUDE.md` cảnh báo.

### ~~MV-5~~ ✅ — ESLint chạy từ gốc repo cho 111 lỗi giả

**Bằng chứng đo 25/08.** `npx eslint . --max-warnings 0 --no-warn-ignored` từ gốc → 111 lỗi.
Phân bố: **79** trong `liva-ai-engine/llama_cpp_src`, **32** trong `target/debug`.
**Không lỗi nào thuộc mã LIVA.** CI chạy eslint với `working-directory: liva-ui` nên không dính.

**Đã sửa ở `2736fa05`.** Kiểm lại 25/08: `npx eslint . --max-warnings 0 --no-warn-ignored`
từ **gốc repo** → **exit 0** (trước đó 111 lỗi).

### ~~MV-6~~ ✅ — `liva-ai-engine/` 3 GB không bị gitignore

**Bằng chứng.** `git status --porcelain -uall liva-ai-engine` → **26 071 file untracked**,
`du -sh` → **3,0 GB**. Bên trong: `venv`, `venv_backup`, một bản clone llama.cpp
(`llama_cpp_src`), log build, script Python. Hoạt động cuối 08–09/06/2026.
`mvc-simulation/` cùng cảnh, 1,1 MB.

**Hệ quả.** Một `git add -A` nuốt trọn 26 071 file.

**Đã sửa ở `2736fa05`:** cả hai vào `.gitignore`. Kiểm lại: `git status` không còn liệt kê
chúng, và **cả hai vẫn nguyên trên đĩa** (`du -sh liva-ai-engine` → 3,0 G) — đúng chủ ý.

> 🚫 **Vẫn KHÔNG xoá.** 3 GB đó **không nằm trong git** — xoá là mất vĩnh viễn. Muốn xoá thì
> phải hỏi người dùng bằng một câu tách bạch, không suy ra từ chữ "dọn rác".

---

## 4. Nhóm C — quyết định của người có máy, không phải của agent

### MV-7 — `models/` trống nên một test đỏ

**Bằng chứng.** `find models -name "*.gguf" -o -name "*.onnx"` → **0 file**. Test
`preflight::n_gpu_layers_bang_0_khong_bao_gio_la_xanh` đỏ vì thế.

**Việc.** `npm run setup:models` — tốn băng thông và ổ đĩa lớn, nên là quyết định của bạn.

> 🚫 **Tuyệt đối không hạ ngưỡng để làm test này xanh.** Thông điệp của chính test đã ghi
> "tải model xong dòng này tự xanh". Hạ ngưỡng là biến một chỉ báo môi trường thành cổng dối.

### MV-8 — `cargo-deny` chưa cài trên máy này

CI tự cài bản ghim `0.20.2`. Cài local chỉ cần khi muốn biết trước MV-1.

### MV-9 — `mac-v2` track nhầm `origin/main`

**Bằng chứng.** `git config branch.mac-v2.merge` → `refs/heads/main`.
Một `git pull` trên nhánh này sẽ kéo `main` vào. Push phải luôn tường minh
`git push origin mac-v2`.

**Sửa (nếu muốn).** `git branch --set-upstream-to=origin/mac-v2 mac-v2`.

---

## 5. Nhóm D — nợ tài liệu còn lại (con trỏ, không phải bản sao)

| ID | Việc | Chủ sở hữu số đo |
|---|---|---|
| **MV-10** | 187 trích dẫn mơ hồ / trần 207 — còn 20 slot. `--suggest` cho 552 ứng viên nhưng công cụ **cố ý không có `--fix`** | [roadmap D0.5](roadmap.md) |
| **MV-11** | Di trú tài liệu v2 còn 16 MERGE + 7 SPLIT chưa làm | [Quy hoạch tài liệu](../07-dong-gop/quy-hoach-tai-lieu.md) |
| **MV-12** | 7 "sự thật chưa có chủ" — `AppState`, vòng đời khởi động, bảng cổng mạng… | [Sổ nguồn sự thật §5](../_meta/nguon-su-that.md) |
| **MV-13** | Hai khoá `owns` đã nghỉ hưu (`lo-trinh-5-giai-doan`, `huong-dan-sua-F1-F5`) vẫn còn tài liệu trỏ tới | [Sổ nguồn sự thật](../_meta/nguon-su-that.md) |

⚠️ **MV-10 là chỗ dễ làm hỏng nhất.** Đổi hàng loạt toạ độ sang neo ký hiệu mà không đọc
văn cảnh từng chỗ là đúng loại việc [§9 backlog](../03-danh-gia/05-nang-cap-toan-dien.md)
cấm. Công cụ đã từng gợi ý sai: `governor.rs:97 → #busy_cpu_threshold` trong khi văn bản
nói `external_cpu_percent`.

---

## 6. Thứ tự thi hành

1. **MV-1** trước tất cả. Mọi việc khác đều rẻ hơn sau khi biết CI nói gì.
2. ~~**MV-4, MV-5, MV-6**~~ ✅ xong 25/08 (`474421ff`, `2736fa05`).
3. **MV-2, MV-3** — do MV-1 trả lời phần lớn.
4. **MV-7, MV-9** — chờ quyết định của người dùng.
5. **MV-10 → MV-13** — nợ dài hạn, không chặn hợp nhất. Làm theo lát nhỏ.

**Quy tắc chung cho mọi mục:** một commit một chủ đề; **không trộn mã nguồn với tài liệu**
trong một commit (`docs-check` so `git log <commit>..HEAD`, gộp làm nó đỏ — đã xảy ra ở
`241e8f9`); hook pre-commit cần `SKIP_AI_HOOK=1` vì máy này không có `.env`.

## Liên quan

- [Master roadmap](roadmap.md) — mốc chiến lược và thứ tự; tài liệu này không đặt lại ưu tiên
- [Backlog nâng cấp U1–U33](../03-danh-gia/05-nang-cap-toan-dien.md) — năng lực sản phẩm và §9 "cái KHÔNG nên làm"
- [Phát triển trên macOS](../02-van-hanh/07-macos-dev.md) — ba bẫy đã trả giá khi đưa cổng kiểm sang macOS
- [Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md) — 25 bước CI và ba gate chỉ-Windows
- [Nợ kỹ thuật và rủi ro](../03-danh-gia/02-no-ky-thuat-va-rui-ro.md) — mẫu trôi advisory npm
