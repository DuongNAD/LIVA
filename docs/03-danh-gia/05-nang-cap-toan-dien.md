---
title: "Nâng cấp toàn diện — việc cần làm, theo thứ tự"
updated: 2026-07-26
commit: 45e2e58
status: living
owns:
  - duong-co-so-do-luong
  - backlog-nang-cap-U1-U15
  - goi-trinh-dien-U16-U20
covers:
  - .github/workflows/test.yml
  - AGENTS.md
  - liva-desktop/src-tauri/src/lib.rs
  - liva-native-core/src/governor.rs
  - liva-native-core/src/integrations/smart_home.rs
  - liva-native-core/src/lib.rs
  - liva-native-core/src/llm/tool_calling.rs
  - liva-native-core/src/main.rs
  - liva-native-core/src/mcp/client.rs
  - liva-native-core/src/sysinfo.rs
  - liva-native-core/src/tts/normalizer.rs
  - liva-native-core/src/tts/vieneu/g2p.rs
  - liva-native-core/src/vision/mod.rs
  - liva-ui/src/WidgetApp.vue
  - liva-ui/vitest.config.ts
  - scripts/docs-check.mjs
  - scripts/e2e-gateway.mjs
  - scripts/e2e-memory.mjs
---
# Nâng cấp toàn diện — việc cần làm, theo thứ tự

[⬆ Mục lục](../README.md) · [◀ Lộ trình sửa lỗi và nâng cấp](03-lo-trinh-sua-loi-va-nang-cap.md)

---

> **Tài liệu này là gì.** Một **backlog thi hành** dành cho các phiên làm việc sau (người hoặc agent). Mỗi mục nêu: vì sao đáng làm, sửa file nào, và **điều kiện nghiệm thu đo được**.
>
> **Tài liệu này KHÔNG phải gì.** Không phải bản đánh giá — phần đánh giá nằm ở [01-doi-chieu-tuyen-bo-vs-thuc-te.md](01-doi-chieu-tuyen-bo-vs-thuc-te.md) và [02-no-ky-thuat-va-rui-ro.md](02-no-ky-thuat-va-rui-ro.md). Không thay thế [03-lo-trinh-sua-loi-va-nang-cap.md](03-lo-trinh-sua-loi-va-nang-cap.md): tài liệu đó theo dõi **sửa lỗi** GĐ0–GĐ4; tài liệu này theo dõi **nâng cấp chất lượng** sau khi lớp bug chặn phát hành đã đóng.

---

## 0. Dành cho phiên làm việc sau — đọc và làm thế nào

**Giao thức 5 bước. Đừng bỏ bước 1.**

1. **Chạy lại đường cơ sở ở §1.** Nếu một con số lệch xuống, đó là **hồi quy** — xử lý trước mọi mục trong backlog. Số ở §1 đo ngày 26/07/2026; càng xa ngày đó càng phải nghi ngờ.
2. **Chọn mục cao nhất chưa gạch trong bảng §2.** Thứ tự đã tính theo "hỏng khi dùng thật → làm hồ sơ nói sai → khó chịu lâu dài", cùng nguyên tắc với [§1 của lộ trình](03-lo-trinh-sua-loi-va-nang-cap.md).
3. **Làm theo dòng "Nghiệm thu" của mục đó.** Nghiệm thu là **hợp đồng**, không phải gợi ý. Chưa chạy được lệnh nghiệm thu thì mục chưa xong.
4. **Đánh dấu xong đúng cách:** gạch ngang số hiệu, thêm ✅ + ngày + **output thật đã đo**. Viết "đã xong" mà không kèm bằng chứng là vi phạm nguyên tắc [không bịa số](../README.md#không-bịa-số-liệu) của dự án.
5. **Cập nhật `updated:` và `commit:`** trong front-matter file này.

**Ba luật cứng khi thi hành backlog này:**

- **Không commit tự động.** `git commit`/`push`/`pull` là hành động của người dùng (`AGENTS.md`).
- **Chạy `impact()` trước khi sửa symbol.** Bắt buộc theo `CLAUDE.md`; các mục U10/U11 chạm vào symbol có nhiều người gọi.
- **Không hạ ngưỡng để cổng xanh.** Ngưỡng coverage trong `vitest.config.ts:42` là bánh cóc — chỉ đi lên.

---

## 1. Đường cơ sở đã đo — 26/07/2026

Tất cả các số dưới đây do **chạy thật**, không trích từ tài liệu. Lệnh kèm theo để tái lập.

| Cổng | Lệnh | Kết quả 26/07/2026 |
|---|---|---|
| Test Rust | `cargo test --no-fail-fast` (trong `liva-native-core/`) | **348 pass · 0 fail · 1 ignored**, 16 binary |
| Clippy (gate cứng) | `cargo clippy --all-targets --message-format=short` rồi đếm `": warning:"` | **0 warning** |
| Typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` (trong `liva-ui/`) | **0 lỗi** |
| ESLint | `npx eslint . --max-warnings 0` | **0 warning** |
| Coverage UI | `npm run test:coverage -w liva-ui` | **63,17 % stmt · 45,84 % branch · 49,67 % func · 65,09 % line** |
| Sức khoẻ tài liệu | `node scripts/docs-check.mjs` | pass, **20 tài liệu bị đánh dấu lỗi thời** |
| E2E WebSocket | gateway :8099 + `node scripts/e2e-gateway.mjs` | **8/8 đạt** |
| E2E bộ nhớ | gateway :8099 + `node scripts/e2e-memory.mjs` | **6/6 phép kiểm cứng đạt** |

**Quy mô mã nguồn** (đếm bằng số dòng, thời điểm cùng ngày): Rust `liva-native-core/src` 81 file · ~26 800 dòng; `liva-ui/src` 48 file · ~14 500 dòng; vỏ Tauri `liva-desktop/src-tauri/src` 833 dòng. 858 crate trong `Cargo.lock`. 29 namespace lệnh trong `handle_command`.

**Mật độ panic:** `.unwrap()` xuất hiện **112 lần trong code production** và 332 lần trong khối `#[cfg(test)]`. Lệnh đếm lại nằm ở [U7](#u7--dọn-unwrap-trên-đường-thoại).

**Hai điều kiện đo cần biết để không hiểu nhầm số trên:**

1. Đo trên **build debug**. Không có `target/release/` tại thời điểm đo — xem [U1](#u1--build-release-và-kiểm-visionask-thật).
2. Đo trên **cây làm việc có công việc chưa commit**: `llm/tool_calling.rs` (836 dòng, file mới), `sysinfo.rs` (143 dòng, file mới) và ~399 dòng diff ở `lib.rs`/`websocket.rs`/`telegram.rs`/`tts/`/`governor.rs`. Toàn bộ cổng vẫn xanh **kèm** phần chưa commit này. Khi phần đó được commit, chạy lại §1 và cập nhật.

> 📌 Nguồn đầy đủ: [Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md)

---

## 2. Bảng ưu tiên tổng hợp

| # | Việc | Nhóm | Chặn cái gì | Công sức |
|---|---|---|---|---|
| **U1** | [Build release + kiểm `vision:ask` thật](#u1--build-release-và-kiểm-visionask-thật) | A | Beta · Hồ sơ | 1 buổi (build lâu) |
| **U2** | [Installer hiện hành + thử trên máy sạch](#u2--installer-hiện-hành-và-thử-trên-máy-sạch) | A | Beta | 1–2 ngày |
| **U3** | [Lệnh `preflight` báo trạng thái tài nguyên](#u3--lệnh-preflight-báo-trạng-thái-tài-nguyên) | A | Beta | 0,5 ngày |
| **U4** | [Đồng bộ `03-danh-gia/` với code](#u4--đồng-bộ-03-danh-gia-với-code) | B | Hồ sơ | 1 ngày |
| **U5** | [Biến drift tài liệu thành gate thật](#u5--biến-drift-tài-liệu-thành-gate-thật) | B | — | 0,5 ngày |
| **U6** | [Sửa con trỏ chết trong AGENTS.md](#u6--sửa-con-trỏ-chết-trong-agentsmd) | B | — | 10 phút |
| **U7** | [Dọn `unwrap()` trên đường thoại](#u7--dọn-unwrap-trên-đường-thoại) | C | Beta | 1–2 ngày |
| **U8** | [Thu hẹp khoảng cách hai profile chạy](#u8--thu-hẹp-khoảng-cách-hai-profile-chạy) | C | Beta · Hồ sơ | 2–4 ngày |
| **U9** | [Một con số TTFT đo được](#u9--một-con-số-ttft-đo-được) | C | Hồ sơ | 0,5 ngày |
| **U10** | [Tách `handle_command`](#u10--tách-handle_command) | D | — | 2–3 ngày |
| **U11** | [Lấp lỗ test WidgetApp.vue](#u11--lấp-lỗ-test-widgetappvue) | D | — | 2–3 ngày |
| **U12** | [Tool calling (đang làm dở)](#u12--tool-calling-đang-làm-dở) | E | — | đang chạy |
| **U13** | [Consolidation ngữ nghĩa L2 → L3](#u13--consolidation-ngữ-nghĩa-l2--l3) | E | — | 1–2 tuần |
| **U14** | [Tự động chuyển router ↔ expert](#u14--tự-động-chuyển-router--expert) | E | — | 3–5 ngày |
| **U15** | [Nối `CodeAgent` vào LLM thật](#u15--nối-codeagent-vào-llm-thật) | E | — | 1 tuần |
| **U16** | [Gói demo "không alt-tab", có hiện chi phí](#u16--gói-demo-không-alt-tab-có-hiện-chi-phí) | F | Hồ sơ | 2–3 ngày |
| **U17** | [Onboarding 10 giây — LIVA nói bằng giọng người dùng](#u17--onboarding-10-giây--liva-nói-bằng-giọng-người-dùng) | F | Hồ sơ | 1–2 ngày |
| **U18** | [Trí nhớ nhìn thấy được, ngay trên UI](#u18--trí-nhớ-nhìn-thấy-được-ngay-trên-ui) | F | — | 1 ngày |
| **U19** | [Ba tool OS thật](#u19--ba-tool-os-thật) | F | — | 2–3 ngày |
| **U20** | [Bộ nhớ thị giác offline *(tuỳ chọn, đắt, có mìn)*](#u20--bộ-nhớ-thị-giác-offline-tuỳ-chọn-đắt-có-mìn) | F | — | 3–4 tuần |

**Quy tắc chặn:** không phát hành cho beta khi A chưa xong; không nộp hồ sơ khi U4 chưa xong. **Không đụng nhóm D khi A/B/C còn dở** — tái cấu trúc trong lúc còn bug là cách nhanh nhất để mất cả hai. **Nhóm F cần U1 + U8 làm nguyên liệu** — làm F trước sẽ ra một video quay cảnh tính năng chưa chạy.

---

## 3. Nhóm A — Giao được cho người khác

### U1 — Build release và kiểm `vision:ask` thật

**Vì sao.** `target/release/` **trống**. E2E ngày 26/07/2026 trả về nguyên văn: `Vision requires a release build (debug CRT assertion in the mmproj loader)`. Nghĩa là "LIVA thấy màn hình" — một trong ba trụ cột được quảng bá — **không dùng được ở bất kỳ trạng thái nào của repo hiện tại**. Đây là khoảng cách tuyên bố ↔ thực tế nghiêm trọng nhất còn lại.

**Việc.**
1. `cargo build --release` (lâu: llama.cpp bị ghim `opt-level=3` ngay cả ở profile dev, nên release không rẻ hơn bao nhiêu — đặt cả buổi).
2. Chạy lại `scripts/e2e-gateway.mjs` với binary release.
3. **Truy nguyên nhân gốc** của CRT assertion trong bộ nạp mmproj, đừng chỉ né bằng release. Né thì debug build vĩnh viễn không kiểm được vision, và đó chính là build mà mọi phiên phát triển dùng.

**File.** `liva-native-core/src/vision/mod.rs`, đường nạp mmproj trong `liva-native-core/src/llm/engine.rs`, `Cargo.toml` (profile).

**Nghiệm thu.** `e2e-gateway.mjs` báo `vision:ask` trả **nội dung mô tả ảnh**, không phải chuỗi lỗi. Ghi thời gian đo được vào §1. Nếu quyết định chấp nhận "vision chỉ chạy ở release", phải ghi điều đó vào `README.md` **và** `02-van-hanh/03-trien-khai-va-runtime.md` như một giới hạn tường minh.

---

### U2 — Installer hiện hành và thử trên máy sạch

**Vì sao.** `release/` chỉ chứa artifact ngày 25–27/06/2026. `desktop-client-setup.exe` nặng **2,5 MB** — quá nhỏ cho một app Tauri nhúng lõi Rust có llama.cpp, gần như chắc chắn là di sản thời Node.js. Beta = 5 người bạn cài trên laptop của họ; đây là thứ đầu tiên họ chạm vào, và hiện nó không tồn tại.

**Việc.**
1. `tauri build` ra installer thật; ghi lại kích thước và thời gian build.
2. Cài trên một máy hoặc VM **chưa từng có LIVA** — không có Rust, không có LLVM, không có model.
3. Ghi lại **chính xác** những gì thiếu và thông điệp người dùng nhận được ở từng bước.
4. Xoá hoặc chuyển `release/desktop-client*.exe` vào lưu trữ nếu xác nhận là di sản.

**File.** `liva-desktop/src-tauri/tauri.conf.json`, `release/`, `README.md` (mục Cài đặt).

**Nghiệm thu.** Một checklist "máy sạch" có thật trong `02-van-hanh/03-trien-khai-va-runtime.md`: model nào bắt buộc, model nào tuỳ chọn, thiếu từng cái thì hành vi ra sao. Checklist phải do **chạy thử** sinh ra, không do suy luận từ code.

---

### U3 — Lệnh `preflight` báo trạng thái tài nguyên

**Vì sao.** Khi khởi động lõi ngày 26/07/2026, thiếu voice embedding Kokoro chỉ tạo ra **một dòng WARN** lẫn giữa hàng trăm dòng log ONNX. Người dùng thật sẽ không thấy. Đã biết ít nhất ba thứ **suy giảm im lặng**: model embedding thiếu → RAG thành no-op; voice Kokoro thiếu → mất một backend TTS; model LLM sai đường dẫn → không có não.

Suy giảm im lặng là kiểu lỗi tệ nhất cho beta tester offline: sản phẩm "chạy" nhưng cụt tính năng, và họ không có cách nào biết vì sao.

**Việc.** Thêm cờ `--preflight` cho binary lõi (và một lệnh tương ứng cho vỏ Tauri) in ra bảng: từng tài nguyên · tìm ở đường dẫn nào · có/không · **hệ quả khi thiếu**. Đã có sẵn nguyên liệu: `db_error_hint` trong `liva-native-core/src/lib.rs` là đúng tinh thần này, chỉ cần mở rộng ra toàn bộ tài nguyên.

**File.** `liva-native-core/src/main.rs`, `liva-native-core/src/lib.rs`, một màn hình trong `liva-ui/src/components/dashboard/`.

**Nghiệm thu.** Chạy trên máy thiếu **mọi** model → in bảng đầy đủ, exit 0 (báo cáo chứ không chết). UI có chỗ hiện đúng bảng đó. `scripts/start_all.ps1 -CheckOnly` gọi nó.

---

## 4. Nhóm B — Tài liệu khớp code

### U4 — Đồng bộ `03-danh-gia/` với code

**Vì sao — đây là rủi ro hồ sơ lớn nhất, và nó lệch theo chiều bất ngờ.** `docs-check.mjs` báo **20 tài liệu lỗi thời**. Vấn đề không phải tài liệu thổi phồng sản phẩm, mà **tài liệu đang nói xấu sản phẩm hơn thực tế**. Ba chỗ đã xác minh ngày 26/07/2026:

| Tài liệu nói | Code thật | Bằng chứng |
|---|---|---|
| `integration:smart_home_control` "trả chuỗi thành công **vô điều kiện**… nguy hiểm hơn cả việc thiếu" | Trả thông báo trung thực "chưa kết nối tích hợp nào", **có test ép** không được báo thành công giả | `execute()` trong `integrations/smart_home.rs`; test `test_execute_bao_trung_thuc_khong_thanh_cong_gia` |
| "`chat:completion` hiện **chưa** tự động lưu ký ức" | Lưu và nhớ đúng qua cả ba cửa vào | `e2e-memory.mjs` 6/6 đạt, 26/07/2026 |
| README: `TELEGRAM_ALLOWED_IDS` rỗng = **cho phép tất cả** | Fail-closed: danh sách rỗng thì **từ chối tất cả** | `is_authorized` trong `liva-native-core/src/telegram.rs` |

Nếu ban giám khảo đọc `01-doi-chieu-tuyen-bo-vs-thuc-te.md`, họ sẽ thấy một sản phẩm tệ hơn sản phẩm thật. Đó là thiệt hại tự gây, và không sửa hồi tố được sau khi nộp.

**Việc.** Sửa ba dòng trên trước (30 phút), rồi rà 20 file theo đúng danh sách `docs-check.mjs` in ra. Mỗi file sửa xong cập nhật `updated:` + `commit:`.

**Nghiệm thu.** `node scripts/docs-check.mjs` không còn liệt kê file nào thuộc `03-danh-gia/` trong khối cảnh báo lỗi thời.

---

### U5 — Biến drift tài liệu thành gate thật

**Vì sao.** Drift hiện chỉ là **cảnh báo**, nên nó tích tụ mãi mãi — 20 file là bằng chứng thực nghiệm cho điều đó. Dự án đã tự học bài học này hai lần với CI ("xanh giả" ở typecheck và coverage, xem `.github/workflows/test.yml`); đây là lần thứ ba của cùng một mô thức: **một cái kiểm không bao giờ đỏ thì không phải là cái kiểm**.

**Việc.** Chọn một trong hai, đừng làm cả hai:
- **(a)** Thêm cờ `--strict-stale=<thư mục>` cho `scripts/docs-check.mjs`, bật trong CI **chỉ cho `03-danh-gia/`**.
- **(b)** Thêm trường front-matter `stale-ok: <commit>` nghĩa là "đã rà tới commit này, chấp nhận lệch" — rồi bắt lỗi khi không có nó.

**⚠️ Đừng bật strict cho toàn bộ `docs/` cùng lúc.** 20 file đỏ ngay lập tức sẽ khiến người ta tắt gate, và ta mất luôn cả cảnh báo đang có. Siết từng thư mục một, bắt đầu từ `03-danh-gia/` sau khi U4 xong.

**Bằng chứng tươi cho thấy khoảng trống nằm ở đâu.** Ngay trong ngày soạn tài liệu này, một trích dẫn `smart_home.rs:75` đã trôi sang dòng 84 vì file được sửa song song — `docs-citations.mjs` **không báo gì**, vì nó chỉ kiểm số dòng có vượt độ dài file hay không, đúng như giới hạn nó tự khai. Toạ độ vẫn "hợp lệ" nhưng trỏ vào một dòng chú thích. Bài học đã đưa vào thực hành trong chính tài liệu này: **chỗ nào file còn đang sửa thì trích theo tên symbol, đừng trích số dòng.** Nếu U5 có mở rộng, đây là hướng đáng giá hơn cả việc siết cờ stale.

**Nghiệm thu.** CI đỏ khi cố tình sửa một file mã nguồn nằm trong `covers` của một tài liệu `03-danh-gia/` mà không cập nhật tài liệu đó.

---

### U6 — Sửa con trỏ chết trong AGENTS.md

**Vì sao.** Mục "Rust Migration Plan" trong `AGENTS.md` trỏ tới `LIVA_NATIVE_MIGRATION_PLAN.md` ở **gốc repo** — file đó đã chuyển vào `docs/99-luu-tru/ke-hoach-da-hoan-thanh/`. `docs-check.mjs` chỉ quét trong `docs/` nên không bắt được. Mọi phiên agent đều đọc `AGENTS.md`, nên một con trỏ chết ở đây tốn thời gian của **mọi** phiên sau.

**Việc.** Sửa đường dẫn. Cân nhắc mở rộng `docs-check.mjs` quét thêm `AGENTS.md`, `CLAUDE.md`, `README.md` — ba file này được đọc nhiều nhất mà lại nằm ngoài mọi cổng kiểm liên kết.

**Nghiệm thu.** Không còn liên kết tương đối hỏng trong ba file gốc repo.

---

## 5. Nhóm C — Độ bền khi người lạ dùng

### U7 — Dọn `unwrap()` trên đường thoại

**Vì sao.** 112 `.unwrap()` trong code production, tập trung dày nhất ở `tts/vieneu/g2p.rs` (20) và `tts/normalizer.rs` (18) — **38 điểm panic tiềm tàng nằm đúng trên đường chạy của mọi lượt nói**. Với beta tester chạy offline trên máy của họ, panic ở đây là loại lỗi khó lấy log nhất: LIVA im lặng, người dùng không biết vì sao, và ta không có gì để đọc.

Lệnh đếm lại (PowerShell, chạy từ gốc repo):

```powershell
$src = Get-ChildItem liva-native-core\src -Recurse -Filter *.rs
($src | ForEach-Object { ([regex]::Matches((Get-Content $_.FullName -Raw),'\.unwrap\(\)')).Count } | Measure-Object -Sum).Sum
```

**Việc.** Chuyển sang `Result` / `unwrap_or` / `ok_or_else` theo thứ tự: `normalizer.rs` → `g2p.rs` → phần còn lại trong `tts/`. **Binary trong `src/bin/` để sau** — chúng là công cụ đo, panic ở đó là chấp nhận được.

**Nghiệm thu.** Số đếm ở đường thoại về 0; `verify_round2.exe` và `voice_stress.exe` vẫn xanh; **thêm test đầu vào rác** (chuỗi rỗng, chỉ emoji, ký tự điều khiển, văn bản 100 KB) cho `normalizer` và `g2p` — không có test này thì việc dọn chỉ là chuyển panic sang một dạng khác.

---

### U8 — Thu hẹp khoảng cách hai profile chạy

**Vì sao — đây là mục có tỷ lệ ấn-tượng/công-sức cao nhất trong toàn bộ backlog.** Beta tester chạy vỏ Tauri (`npm run dev`). Nhưng đường VAD/barge-in **và** bot Telegram chỉ sống ở binary standalone. Hệ quả: **thứ ấn tượng nhất của sản phẩm — cướp lời giữa câu — người dùng thật không bao giờ thấy.**

Dự án đã tự nhận diện vấn đề "hai profile không tương đương" ở [§0 kiến trúc tổng thể](../01-ban-ve/01-kien-truc-tong-the.md) và README. Nhận diện rồi thì bước tiếp là **quyết định**, không phải mô tả tiếp.

**Việc.** Chọn dứt khoát một trong hai:
- **(a) Nối đường VAD vào vỏ Tauri.** Đắt hơn, nhưng đây là năng lực bán được sản phẩm.
- **(b) Gỡ barge-in khỏi mô tả dành cho người dùng cuối** và nêu rõ nó là năng lực của chế độ headless.

Đừng chọn "để sau" — đó là lựa chọn (b) nhưng không ai ghi lại.

**File.** `liva-desktop/src-tauri/src/lib.rs`, `liva-native-core/src/webrtc/`, `liva-ui/src/composables/useVoicePipeline.ts`.

**Nghiệm thu.** Có **một** bảng "năng lực theo profile" duy nhất trong `01-ban-ve/01-kien-truc-tong-the.md`, và trải nghiệm mặc định của `npm run dev` khớp đúng bảng đó. Nếu chọn (a): quay được video barge-in trên bản Tauri.

---

### U9 — Một con số TTFT đo được

**Vì sao.** README tự nhận dự án **chưa có benchmark TTFT**. Với hồ sơ dự thi, một con số đo được và tái lập được mạnh hơn bất kỳ tính từ nào. Đây cũng là mục cuối còn lại trong danh sách "Near-term" của README.

**Việc.** Một binary `liva-native-core/src/bin/ttft_bench.rs`: nạp model → prompt cố định → đo thời gian tới **token đầu tiên**, lặp N lần, in p50/p95 kèm cấu hình máy (CPU, GPU, `n_gpu_layers`, `n_ctx`, debug/release).

**Nghiệm thu.** Chạy được bằng một lệnh; số ghi vào §1 của tài liệu này **kèm cấu hình máy đo**. Tuyệt đối không viết số nào chưa chạy.

**Số tham chiếu — KHÔNG phải TTFT.** Ngày 26/07/2026, `e2e-memory.mjs` ghi lượt hội thoại đầu 3,7 s (41 chunk) và lượt sau 2,2 s. Đó là **tổng thời gian một lượt** gồm cả embedding + truy hồi + sinh toàn bộ câu, trên **build debug**, model Qwen3-VL-2B Q4_K_M, CPU. Không được trích con số này như TTFT.

---

## 6. Nhóm D — Cấu trúc

> **Điều kiện vào nhóm này: A, B, C đã xong.** Tái cấu trúc khi còn bug chặn phát hành là cách nhanh nhất để mất cả hai.

### U10 — Tách `handle_command`

**Vì sao.** `liva-native-core/src/lib.rs` đã vượt 2 700 dòng và **đang tăng**, phần lớn là một `match` duy nhất trải trên 29 namespace lệnh. Mỗi tính năng mới lại nới rộng cùng một hàm — `tool_calling.rs` ([U12](#u12--tool-calling-đang-làm-dở)) sẽ nới tiếp.

**Việc.** Tách theo namespace (`vision:`, `memory:`, `voice:`, `llm:`, `mcp:`, `mcp_client:`, `telegram:`, `integration:`, `chat:`) thành module `commands/`, giữ nguyên chữ ký `handle_command` làm điểm vào mỏng.

**⚠️ Bắt buộc trước khi sửa:** `impact({target: "handle_command", direction: "upstream"})`. Hàm này là điểm hội tụ của cả hai profile chạy — blast radius lớn.

**⚠️ Thời điểm:** không làm khi `lib.rs` đang có công việc chưa commit. Ngày 26/07/2026 có ~1 380 dòng đang dở.

**Nghiệm thu.** `handle_command` còn lại là bộ định tuyến mỏng; `cargo test` vẫn **348 pass**; `e2e-gateway.mjs` vẫn **8/8**; clippy vẫn **0**.

---

### U11 — Lấp lỗ test WidgetApp.vue

**Vì sao.** `liva-ui/src/WidgetApp.vue` dài 1 443 dòng và chỉ đạt **29,13 % statement** — thấp nhất toàn UI. Trớ trêu là đây chính là **cửa sổ người dùng nhìn thấy suốt ngày** (Ghost Mode overlay). Hạng nhì là `MemoryViewer.vue` (1 373 dòng, 60,69 %) và `VisionView.vue` (20 %).

**Việc.** Tách logic ra composable rồi test composable — rẻ hơn nhiều so với test component, và `liva-ui/src/composables/` đã có sẵn khuôn mẫu tốt (`useFaceTracking.ts` đạt 92 %).

**Nghiệm thu.** `WidgetApp.vue` ≥ 60 % statement, **và nâng ngưỡng trong `vitest.config.ts:42` lên mức mới đạt được**. Bánh cóc chỉ đi lên.

---

## 7. Nhóm E — Năng lực mới

> Chỉ vào nhóm này sau khi A–C xong. Đây là các mục đã nằm trong "Roadmap · Near-term" của README, ghi lại ở đây để có nghiệm thu.

### U12 — Tool calling (đang làm dở)

**Trạng thái 26/07/2026: ĐANG CHẠY, không phải đề xuất.** `liva-native-core/src/llm/tool_calling.rs` tồn tại với 836 dòng và **chưa được commit**. Ghi vào đây để phiên sau **không làm trùng**. Trước khi bắt đầu bất cứ việc gì liên quan tới tool/skill calling, chạy `git status` và đọc file đó.

---

### U13 — Consolidation ngữ nghĩa L2 → L3

Bộ tiêu thụ projection có giới hạn đã chạy (validate lineage, checkpoint, DLQ 3 lần thử). Bước tiếp: trích **fact/quan hệ bền** từ các event đã `consolidated`, không tái tạo bản sao plaintext và không chặn đường nóng chat/LLM. Bảng `turn_layer_nodes` / `l3_nodes` hiện **chưa có writer nào**.

**Nghiệm thu.** Một e2e kiểu `e2e-memory.mjs`: nói ba sự thật rời rạc qua ba phiên khác nhau → LIVA nối được chúng trong câu trả lời thứ tư. Chưa chứng minh được điều đó thì L3 chỉ là schema.

---

### U14 — Tự động chuyển router ↔ expert

`llm:swap_model` chạy được; **quyết định khi nào swap** thì chưa có. Cần một tín hiệu đánh giá độ khó câu hỏi, và một chính sách chống dao động (đổi model qua lại liên tục còn tệ hơn không đổi).

**Nghiệm thu.** Đo được: tỷ lệ câu đi vào expert, độ trễ thêm do swap, và **một trường hợp cụ thể** mà router 2B trả lời sai còn expert 12B trả lời đúng.

---

### U15 — Nối `CodeAgent` vào LLM thật

Vòng lặp tự sửa lỗi đã hoàn chỉnh và có test; `trait CodeAgent` chỉ có bản mock. Nối adapter vào engine thật rồi đưa `evolution/` ra khỏi `--features experimental`.

**Nghiệm thu.** Một bug có thật, cố ý gieo vào một hàm nhỏ, được vòng lặp tự vá và `cargo test` xanh lại — chạy lại được, không phải trình diễn một lần.

---

## 8. Nhóm F — Biến năng lực thành khoảnh khắc

> **Điều kiện vào nhóm này: [U1](#u1--build-release-và-kiểm-visionask-thật) và [U8](#u8--thu-hẹp-khoảng-cách-hai-profile-chạy) đã xong.** Không có `vision:ask` chạy được và không có barge-in trên vỏ Tauri thì mọi mục dưới đây đều thiếu nguyên liệu — kết quả sẽ là một video quay cảnh tính năng chưa chạy.

**Nhóm này không thêm năng lực nào.** Nó lấy năng lực đã có và biến thành thứ người khác **cảm được trong 60 giây**. Lý do nó cần tồn tại: tính tới 26/07/2026 LIVA có 29 namespace lệnh, 18 binary kiểm chứng và ba trụ cột được quảng bá — nhưng **không có một khoảnh khắc nào demo được liền mạch**. Một danh sách năng lực không gây ấn tượng; một khoảnh khắc thì có. Đây cũng là nhóm trả lời trực tiếp cho câu hỏi "làm sao để người nhìn vào phải ngạc nhiên".

**Nguyên tắc chọn — đọc trước khi thêm bất kỳ mục nào vào nhóm này:**

> Chỉ chọn khoảnh khắc mà chất lượng của nó bị giới hạn bởi **kỹ thuật**, không bởi **độ thông minh của model**.

Beta chạy model 2–4B trên laptop người khác. Model **sẽ** trả lời kém ở suy luận mở, và đó là thứ không sửa được bằng công sức trong phạm vi dự án này. Nhưng độ trễ, sự hiện diện, tính cục bộ và chi phí tài nguyên thì sửa được — bằng đúng thứ dự án này đang giỏi. Cả năm mục U16–U20 đều nằm ở vế thứ hai: **không mục nào đặt cược vào việc model trả lời hay.** Mục nào vi phạm nguyên tắc này thì không thuộc nhóm F.

---

### U16 — Gói demo "không alt-tab", có hiện chi phí

**Vì sao.** Mọi demo trợ lý AI đều giấu cái giá tài nguyên. LIVA là dự án hiếm hoi có đủ số liệu để **hiện** nó: governor đọc tải ngoài **có trừ phần của chính LIVA**, và `sysinfo.rs` từ chối đoán khi không đo được. Một đồng hồ tài nguyên gần như đứng yên trong lúc trợ lý đang nói là bằng chứng **không dựng được bằng cắt ghép** — và nó chứng minh đúng cái trụ cột khó tin nhất, "sống chung với tải nặng". Ba trụ cột hiện đang được kể như ba tính năng rời; mục này gộp chúng vào một cảnh duy nhất.

**Việc.**
1. Một dải nhỏ trong widget hiện ba số realtime lấy từ `get_system_status`: CPU ngoài · GPU ngoài · phần LIVA đang chiếm. Không đo được thì hiện `--` — đó là quy ước của `sysinfo.rs`, **đừng phá nó để video đẹp hơn**.
2. Một kịch bản cố định: mở tải nặng fullscreen → **không thu nhỏ** → hỏi bằng giọng → LIVA nhìn đúng vùng quanh con trỏ, trả lời, và cướp lời được giữa câu.
3. Quay một lần.

**File.** `liva-ui/src/WidgetApp.vue`, `liva-native-core/src/sysinfo.rs`, `liva-native-core/src/governor.rs`.

**Nghiệm thu.** Video ≤ 90 giây, **một lần quay liền mạch, không cắt**, trong đó đồng hồ tài nguyên và ứng dụng nặng nằm cùng khung hình. Số hiển thị phải đọc từ `get_system_status` — ảnh chụp Task Manager ghép vào là **không đạt**, vì nó phá đúng thứ khiến cảnh này đáng tin.

---

### U17 — Onboarding 10 giây — LIVA nói bằng giọng người dùng

**Vì sao.** VieNeu-TTS đã được port thuần Rust và clone giọng từ 3–8 giây mẫu, nhưng đang nằm sau `LIVA_TTS_VIENEU=1`. Không beta tester nào đặt biến môi trường để thử một tính năng họ chưa biết là có. Đây là mục có **tỷ lệ ấn tượng trên công sức cao nhất** trong nhóm: engine đã xong, phần thiếu chỉ là một luồng onboard. Nghe chính giọng mình trả lời mình, hoàn toàn offline, trên một chiếc laptop — đó là thứ người ta kể lại cho người khác nghe.

**Việc.** Một màn hình lúc chạy lần đầu: đọc một câu → tạo style vector → đặt làm giọng mặc định. Bỏ cổng env cho **riêng luồng này** (vẫn giữ env cho chế độ nâng cao).

**File.** `liva-native-core/src/tts/vieneu/`, `liva-native-core/src/tts/mod.rs`, một màn hình trong `liva-ui/src/components/dashboard/`.

**Nghiệm thu.** Một người **chưa từng thấy LIVA**, từ lúc mở app tới lúc nghe giọng mình phát ra: **dưới 2 phút**, không sửa file cấu hình nào, không đặt biến môi trường nào.

**⚠️ Rủi ro phải kiểm trước khi làm.** Chất lượng giọng VieNeu **chưa được duyệt bằng tai** trên máy này. Nếu giọng clone nghe méo hoặc robot, mục này gây ấn tượng **ngược** — và ấn tượng ngược về giọng nói thì không gỡ lại được. Nghe thử trước, quyết sau.

---

### U18 — Trí nhớ nhìn thấy được, ngay trên UI

**Vì sao.** `e2e-memory.mjs` đã chứng minh 6/6 rằng LIVA nhớ **xuyên qua một lần khởi động lại tiến trình** — nhưng bằng chứng đó nằm trong terminal, nơi không người xem nào nhìn. Đồng thời Memory Dashboard là màn hình **đã có sẵn**. Đây là mục rẻ nhất nhóm F: không viết năng lực mới, chỉ đưa bằng chứng sẵn có lên chỗ nhìn thấy được. Ranh giới giữa "chatbot" và "trợ lý" trong đầu người xem nằm đúng ở khoảnh khắc này.

**Việc.**
1. Một toast ngắn "LIVA vừa nhớ: …" khi một lượt được persist thành công.
2. Nút khởi động lại lõi ngay trên dashboard, để diễn được trước mặt người xem mà không cần mở terminal.

**File.** `liva-ui/src/components/dashboard/`, `liva-native-core/src/lib.rs` (sự kiện persist).

**Nghiệm thu.** Toàn bộ thao tác "nói một sự thật → khởi động lại → hỏi lại → trả lời đúng" làm được **bằng chuột**, không chạm terminal.

**⚠️ Ràng buộc.** Chỉ hiện các tầng **có dữ liệu thật** (L2). Tầng `l0_5`/L3 hiện chưa có writer ([U13](#u13--consolidation-ngữ-nghĩa-l2--l3)); vẽ ô rỗng cho chúng là đi ngược đúng nguyên tắc mà [U3](#u3--lệnh-preflight-báo-trạng-thái-tài-nguyên) và `sysinfo.rs` vừa dựng lên.

---

### U19 — Ba tool OS thật

**Vì sao.** Cơ chế tool-calling đã xong ([U12](#u12--tool-calling-đang-làm-dở)) nhưng **catalog gần như rỗng**: chỉ có bốn tool nội bộ, trong đó hai là thao tác vault và một là smart-home chưa có phần cứng. Cơ chế không có nội dung thì không ai thấy nó tồn tại. "Đang bận tay, nói *nhỏ nhạc lại* → nó làm ngay" gây ấn tượng mạnh hơn mọi câu chat, vì nó chứng minh trợ lý **chạm được vào máy**, không chỉ nói.

**Việc.** Ba tool Win32, không thêm dependency nặng: âm lượng hệ thống · độ sáng màn hình · play/pause media. Cả ba đều **hoàn tác được**, nên đủ điều kiện vào `NATIVE_AUTOEXEC`.

**File.** `liva-native-core/src/llm/tool_calling.rs`, `liva-native-core/src/mcp/client.rs`, một module mới dưới `liva-native-core/src/integrations/`.

**Nghiệm thu.** `tool_calling_probe` chọn **đúng** tool cho 10 câu tiếng Việt tự nhiên với model 2B thật, và mọi tool đều hoàn tác được. Đây **chính là** corpus mà U12 đang chờ để bật `LIVA_TOOL_CALLING` mặc định — làm U19 là làm luôn cổng nghiệm thu của U12.

**⚠️ Đừng thêm tool có hậu quả không đảo ngược** (xoá file, gửi tin, mua bán) vào `NATIVE_AUTOEXEC`. Ranh giới chọn/chạy trong `ExecPolicy` tồn tại đúng để chặn chuyện đó — giữ nó.

---

### U20 — Bộ nhớ thị giác offline *(tuỳ chọn, đắt, có mìn)*

**Vì sao.** Trần ấn tượng cao nhất trong toàn bộ tài liệu này: hỏi "hôm qua mình build lỗi gì ấy nhỉ" và LIVA nhớ, **vì nó đã thấy**. Đây đúng là ý tưởng đã bị công chúng ném đá khi một hãng lớn làm — và lý do bị ném đá là **dữ liệu rời khỏi máy**. LIVA offline là câu trả lời trực tiếp cho lời chê đó.

**⚠️ Đây là mục nguy hiểm nhất tài liệu này.** Nó dẫm vào `passive/hook.rs` — một keylogger toàn hệ thống hiện đã bị đưa ra ngoài build mặc định, và theo ghi chú của chính dự án thì hook bàn phím cấp OS sẽ khiến anti-cheat **ban phần cứng** máy người chơi game. **Không được** bật lại module đó để làm mục này.

**Việc (nếu làm).** Thu thập qua OS Accessibility / UIAutomation — tên cửa sổ, tiến trình, cấu trúc text UI — **không hook bàn phím**. Cộng với một cổng đồng ý tường minh và một chỉ báo "đang ghi" luôn hiển thị.

**Nghiệm thu — theo thứ tự bắt buộc.** Cổng đồng ý và công tắc tắt phải **tồn tại và hoạt động trước khi viết dòng code thu thập đầu tiên**. Ngược thứ tự là tự tạo ra thứ không thể phát hành.

---

## 9. Cái KHÔNG nên làm

Mục này tồn tại để phiên sau không đốt thời gian vào việc trông có vẻ hữu ích.

- **Đừng viết thêm tài liệu mới.** Bộ tài liệu đã ~1,1 MB với 20 file lỗi thời. Thêm file làm tỷ lệ lỗi thời **tệ hơn**, không tốt hơn. Sửa file có sẵn; tài liệu này đã là ngoại lệ cuối cùng nên có.
- **Đừng dọn `unwrap()` trong khối `#[cfg(test)]`** (332 điểm). Trong test, `unwrap()` panic **chính là** cơ chế báo lỗi. Đụng vào là làm test yếu đi trong khi trông như đang cải thiện.
- **Đừng hạ ngưỡng coverage** trong `vitest.config.ts` để cổng xanh.
- **Đừng bỏ `DEFAULT_ENCRYPTION_KEY`** (`crypto.rs:16`) mà chưa có đường di trú dữ liệu. Nó cảnh báo lớn nhưng không chặn boot — đó là **quyết định có chủ đích**, đã ghim, không phải sơ suất.
- **Đừng tin số ở §1 nếu ngày đã cũ.** Chạy lại lệnh. Đây là điều kiện tiên quyết, không phải lời khuyên.
- **Đừng gộp nhiều mục U vào một lần sửa.** Mỗi mục có nghiệm thu riêng; gộp lại thì không biết cái nào hỏng khi cổng đỏ.

---

## Liên quan

- [01-doi-chieu-tuyen-bo-vs-thuc-te.md](01-doi-chieu-tuyen-bo-vs-thuc-te.md) — tuyên bố vs bằng chứng `file:dòng` (**cần U4**)
- [02-no-ky-thuat-va-rui-ro.md](02-no-ky-thuat-va-rui-ro.md) — rủi ro xếp hạng và code mồ côi
- [03-lo-trinh-sua-loi-va-nang-cap.md](03-lo-trinh-sua-loi-va-nang-cap.md) — lộ trình sửa lỗi GĐ0–GĐ4 (tài liệu này nối tiếp nó)
- [../02-van-hanh/04-kiem-thu-va-ci.md](../02-van-hanh/04-kiem-thu-va-ci.md) — bề mặt kiểm thử và CI
- [../02-van-hanh/03-trien-khai-va-runtime.md](../02-van-hanh/03-trien-khai-va-runtime.md) — cách chạy đúng (**cần U2**)
- [../01-ban-ve/01-kien-truc-tong-the.md](../01-ban-ve/01-kien-truc-tong-the.md) — hai profile chạy (**cần U8**)
- [../README.md](../README.md) — mục lục và quy ước tài liệu
