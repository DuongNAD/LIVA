---
title: "Nâng cấp toàn diện — việc cần làm, theo thứ tự"
updated: 2026-07-27
commit: 90c38bf
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
  - liva-native-core/src/preflight.rs
  - liva-native-core/src/sysinfo.rs
  - liva-native-core/src/tts/normalizer.rs
  - liva-native-core/src/tts/vieneu/g2p.rs
  - liva-native-core/src/vision/mod.rs
  - liva-ui/src/WidgetApp.vue
  - liva-ui/vitest.config.ts
  - scripts/docs-check.mjs
  - scripts/e2e-gateway.mjs
  - scripts/e2e-memory.mjs
  - scripts/start_all.ps1
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
| Test Rust | `cargo test --no-fail-fast` (trong `liva-native-core/`) | **405 pass · 0 fail**, 16 binary — *đo lại tại `d88508e`*. Vệt tăng cùng ngày: 348 (`ce1697a`) → 381 (`0b490b9`) → 405 |
| Clippy (gate cứng) | `cargo clippy --all-targets --message-format=short` rồi đếm `": warning:"` | **0 warning** |
| Typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` (trong `liva-ui/`) | **0 lỗi** |
| ESLint | `npx eslint . --max-warnings 0` | **0 warning** |
| Coverage UI | `npm run test:coverage -w liva-ui` | **63,17 % stmt · 45,84 % branch · 49,67 % func · 65,09 % line** |
| Sức khoẻ tài liệu | `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` | pass — lỗi thời ở tầng đánh giá nay là **lỗi chặn** ([U5](#u5--biến-drift-tài-liệu-thành-gate-thật)) |
| Trích dẫn tài liệu | `node scripts/docs-citations.mjs --max-unchecked=508` | pass — **chốt chống thụt lùi**, chỉ được phép giảm |
| E2E WebSocket | gateway :8099 + `node scripts/e2e-gateway.mjs` | **8/8 đạt** |
| E2E bộ nhớ | gateway :8099 + `node scripts/e2e-memory.mjs` | **6/6 phép kiểm cứng đạt** |

**Quy mô mã nguồn** (đếm bằng số dòng, thời điểm cùng ngày): Rust `liva-native-core/src` 81 file · ~26 800 dòng; `liva-ui/src` 48 file · ~14 500 dòng; vỏ Tauri `liva-desktop/src-tauri/src` 833 dòng. 858 crate trong `Cargo.lock`. 29 namespace lệnh trong `handle_command`.

**Mật độ panic:** `.unwrap()` xuất hiện **112 lần trong code production** và 332 lần trong khối `#[cfg(test)]`. Lệnh đếm lại nằm ở [U7](#u7--dọn-unwrap-trên-đường-thoại).

**Bốn điều kiện đo cần biết để không hiểu nhầm số trên:**

1. Đo trên **build debug**. ~~Không có `target/release/` tại thời điểm đo~~ — **đã có từ 26/07/2026** ([U1](#u1--build-release-và-kiểm-visionask-thật)): `target/release/liva-native-core.exe`, và `e2e-gateway.mjs` trên đó cũng **8/8**, khác biệt duy nhất là `vision:ask` trả mô tả thật (~80 s) thay vì lỗi "requires a release build".
2. **Không phải mọi dòng đo cùng một thời điểm.** *Test Rust* đo lại tại `d88508e`; *docs-check* và *docs-citations* phản ánh cấu hình CI hiện hành; **năm dòng còn lại (clippy, typecheck, ESLint, coverage, hai e2e) vẫn đo tại `ce1697a`**, khi cây làm việc có ~1 380 dòng chưa commit. Ghi rõ thay vì để cả bảng trông như một lần đo đồng nhất. **Việc đầu tiên của phiên sau: đo lại đủ chín dòng tại HEAD** — riêng coverage gần như chắc chắn đã đổi, vì U19 thêm 12 unit test và một binary probe mới.
3. **Quy mô mã nguồn ở đoạn dưới cũng đo tại `ce1697a`** và nay đã lạc hậu: từ đó tới `d88508e` có thêm `boot.rs` (~510), `sysinfo.rs` (~160), `llm/tool_calling.rs` (~1 400), `integrations/os_control.rs` (~380) và ba binary probe. Con số "29 namespace lệnh" cũng vậy — đếm lại được **51 nhánh** ở `handle_command`. Đừng trích các số đó mà không đếm lại.
4. **Phân biệt "đỏ ở cây làm việc" với "đỏ ở commit".** Trong phiên 26/07 có lúc `cargo check -p liva-desktop` hỏng vì `tts/vieneu/mod.rs` thiếu field — nhưng file đó **nguyên vẹn ở HEAD**, chỉ là một phiên song song đang sửa dở. Đây đúng loại nhầm lẫn khiến người ta tưởng có hồi quy trong khi không có; luôn kiểm ở commit trước khi kết luận.

> 📌 Nguồn đầy đủ: [Kiểm thử và CI](../02-van-hanh/04-kiem-thu-va-ci.md)

---

## 2. Bảng ưu tiên tổng hợp

| # | Việc | Nhóm | Chặn cái gì | Công sức |
|---|---|---|---|---|
| ~~**U1**~~ ✅ **XONG 26/07/2026** | [Build release + kiểm `vision:ask` thật](#u1--build-release-và-kiểm-visionask-thật) | A | ~~Beta · Hồ sơ~~ | đã xong — **nhưng lộ ra ~80 s/lượt, xem U1a** |
| ~~**U1a**~~ ✅ **XONG 26/07/2026** | [Vision trên CUDA — 80 s → 1,2 s](#u1a--vision-trên-cuda-đo-xong) | A | ~~Demo trực tiếp~~ | đã đo — **nhưng đẻ ra U1b** |
| ~~**U1b**~~ ✅ **XONG 26/07/2026** | [Ghim `CUDAARCHS` + quyết định cách phát hành](#u1b--ghim-cudaarchs-và-quyết-định-cách-phát-hành) | A | ~~Beta · U2~~ | đã đo — **binary −63%; còn 752 MB DLL cuBLAS, xem U1c** |
| ~~**U1c**~~ ✅ **XONG 26/07/2026** | [Thử bỏ phụ thuộc cuBLAS](#u1c--thử-bỏ-phụ-thuộc-cublas-ba-hướng-đều-thất-bại) | A | — | **kết quả ÂM TÍNH**: cả 3 hướng thất bại, cuBLAS là phụ thuộc cứng ⇒ U2 phải tính ~830 MB |
| **U2** | [Installer hiện hành + thử trên máy sạch](#u2--installer-hiện-hành-và-thử-trên-máy-sạch) | A | Beta | 1–2 ngày |
| **U3** ◐ | [Lệnh `preflight` báo trạng thái tài nguyên](#u3--lệnh-preflight-báo-trạng-thái-tài-nguyên--một-phần-cli-xong-26072026-ui-còn-nợ) | A | ~~Beta~~ | **CLI xong 26/07** (`--preflight` + `-CheckOnly`); còn màn hình UI — chặn bởi phiên đang sửa `lib.rs` |
| ~~**U4**~~ ✅ **XONG 26/07/2026** | [Đồng bộ `03-danh-gia/` với code](#u4--đồng-bộ-03-danh-gia-với-code) | B | ~~Hồ sơ~~ | đã xong |
| ~~**U5**~~ ✅ **XONG 26/07/2026** | [Biến drift tài liệu thành gate thật](#u5--biến-drift-tài-liệu-thành-gate-thật) | B | — | đã xong |
| ~~**U6**~~ ✅ **XONG 26/07/2026** | [Sửa con trỏ chết trong AGENTS.md](#u6--sửa-con-trỏ-chết-trong-agentsmd) | B | — | đã xong |
| **U7** | [Dọn `unwrap()` trên đường thoại](#u7--dọn-unwrap-trên-đường-thoại) | C | Beta | 1–2 ngày |
| **U8** ◐ | [Thu hẹp khoảng cách hai profile chạy](#u8--thu-hẹp-khoảng-cách-hai-profile-chạy) | C | ~~Beta · Hồ sơ~~ | **`boot.rs` xong 26/07**; còn bảng "năng lực theo profile" ở `01-ban-ve/01` + `02-van-hanh/03` |
| **U9** | [Một con số TTFT đo được](#u9--một-con-số-ttft-đo-được) | C | Hồ sơ | 0,5 ngày |
| **U10** ◐ | [Tách `handle_command`](#u10--tách-handle_command) | D | — | **đang làm** — 6 miền tách xong, `lib.rs` 2 773 → 1 788 dòng |
| **U11** | [Lấp lỗ test WidgetApp.vue](#u11--lấp-lỗ-test-widgetappvue) | D | — | 2–3 ngày |
| **U12** | [Tool calling (đang làm dở)](#u12--tool-calling-đang-làm-dở) | E | — | đang chạy |
| **U13** | [Consolidation ngữ nghĩa L2 → L3](#u13--consolidation-ngữ-nghĩa-l2--l3) | E | — | 1–2 tuần |
| **U14** | [Tự động chuyển router ↔ expert](#u14--tự-động-chuyển-router--expert) | E | — | 3–5 ngày |
| **U15** | [Nối `CodeAgent` vào LLM thật](#u15--nối-codeagent-vào-llm-thật) | E | — | 1 tuần |
| ◐ **U16** | [Gói demo "không alt-tab", có hiện chi phí](#u16--gói-demo-không-alt-tab-có-hiện-chi-phí) — dụng cụ đo xong 26/07; video chưa quay (vision 80 s chặn kịch bản đầy đủ) | F | Hồ sơ | còn quay |
| ~~**U17a**~~ | [Bộ chọn giọng VieNeu](#u17a--bộ-chọn-giọng-làm-được-ngay-05-ngày) — ✅ **XONG 26/07/2026** | F | — | xong |
| **U17b** | [Clone giọng thật](#u17b--clone-giọng-thật-bị-chặn-chưa-ước-lượng-được) — **BỊ CHẶN**: thiếu 2 model | F | Hồ sơ | chưa ước lượng được |
| ~~**U18**~~ | [Trí nhớ nhìn thấy được, ngay trên UI](#u18--trí-nhớ-nhìn-thấy-được-ngay-trên-ui) — ✅ **nghiệm thu 26/07** (người dùng chạy trên vỏ Tauri) | F | — | xong |
| ~~**U19**~~ | [Ba tool OS thật](#u19--ba-tool-os-thật) — ✅ **nghiệm thu 10/10 ngày 26/07**; độ sáng cố tình bỏ | F | — | xong (2/3 tool) |
| ◐ **U20** | [Bộ nhớ thị giác offline *(tuỳ chọn, đắt, có mìn)*](#u20--bộ-nhớ-thị-giác-offline-tuỳ-chọn-đắt-có-mìn) — **bước 1 (cổng đồng ý) xong 26/07**; chưa có dòng thu thập nào | F | — | còn thu thập |

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

#### ✅ NGHIỆM THU ĐẠT — 26/07/2026, nhưng phát hiện một giới hạn lớn hơn

**Vision CHẠY THẬT.** `cargo build --release --bin liva-native-core` (1 phút 24 — phần C++ đã có sẵn), rồi `e2e-gateway.mjs` trên binary release: **8/8 đạt**, `vision:ask` trả mô tả thật thay vì chuỗi lỗi. Câu trả lời đúng chứ không bịa — nó đọc được trang Facebook đang mở, một bài đăng về AI, và cả tên người trong cuộc trò chuyện.

**Nhưng con số mới là kết quả đáng giá nhất của U1: ~80 giây MỖI LƯỢT.**

| Lượt | Thời gian |
|---|---|
| 1 — nguội (dựng `MtmdContext` từ mmproj 781 MB) | **80,2 s** |
| 2 — ấm | **81,3 s** |
| 3 — ấm | **79,0 s** |

Ba số gần như bằng nhau ⇒ **không phải phí khởi động**. Log llama.cpp cho biết tiền đi đâu:

```
image slice encoded in 56436 ms          ← 56,4 s · bộ mã hoá thị giác (CLIP)
image decoded (batch 1/4) in 5569 ms
image decoded (batch 2/4) in 6433 ms
image decoded (batch 3/4) in 7935 ms
image decoded (batch 4/4) in 9070 ms     ← 29,0 s · 2 040 token ảnh (nx=60 × ny=34) qua LLM
```

Cả hai đều là công việc **trên từng ảnh**, chạy **CPU thuần** — build release này không có `--features cuda`, và `LIVA_LLM_N_GPU_LAYERS` mặc định 0. Máy đo có RTX 5060 Ti 16 GB **hoàn toàn không được dùng**.

⇒ **Kết luận trung thực: vision "chạy được" nhưng chưa "dùng được".** 80 s cho một câu hỏi về màn hình là ngoài ngưỡng chấp nhận của trợ lý thoại. Đừng đưa vào demo trực tiếp khi chưa có GPU.

**Đường đi tiếp, theo thứ tự đòn bẩy giảm dần — chưa đo cái nào:**
1. **Build `--features cuda`.** 56 s mã hoá CLIP là bài toán song song điển hình; đây gần như chắc chắn là đòn bẩy lớn nhất, và nó chỉ là một lần build.
2. **Giảm số token ảnh — knob ĐÃ CÓ SẴN, chỉ đang tắt.** `nx=60 × ny=34` = 2 040 token cho một màn hình 1920×1080. `MtmdContextParams` trong `llm/engine.rs` có `image_min_tokens` và `image_max_tokens`, cả hai đang hardcode `-1` (không giới hạn). Đặt `image_max_tokens` là một dòng, **không cần build lại llama.cpp**, và nó cắt thẳng cả hai tầng chi phí cùng lúc (ít token ⇒ mã hoá nhẹ hơn *và* ít batch giải mã hơn).
   *Đính chính:* bản trước của mục này viết "`VisionConfig` **không có** knob thu nhỏ ảnh" — đúng về `VisionConfig` nhưng **sai về kết luận**: tôi tra nhầm chỗ, knob nằm ở `MtmdContextParams` chứ không phải ở tầng chụp màn hình.
3. Cắt bớt vùng chụp (hỏi về một cửa sổ thay vì cả màn hình).

**Nguyên nhân gốc của "cần release" — đã truy, và comment trong mã nói ĐÚNG.** `[profile.dev.package.llama-cpp-sys-2] opt-level = 3` chỉ là tuỳ chọn **Rust**, nó **không** đụng tới CMake. Crate `cmake` ánh xạ `PROFILE=debug` → CMake `Debug` → MSVC `/MDd` (CRT **debug**), trong khi Rust trên MSVC **luôn** link CRT release. Hai bản CRT ⇒ hai bảng file-descriptor ⇒ bộ nạp clip/mmproj assert rồi abort. Guard `if cfg!(all(windows, debug_assertions))` trong `llm/engine.rs` biến cú abort đó thành một `Err` sạch — đó là xử lý đúng, không phải né tránh.

**Giả thuyết chưa kiểm** để vision chạy được cả ở debug: ép phần C++ dùng CRT release bằng `CXXFLAGS=/MD` + `CFLAGS=/MD` trước khi build. Rẻ để thử (một lần build), nhưng trộn `/MD` với cấu hình CMake `Debug` có thể sinh xung đột ODR khác — **phải đo, không được đoán**.

**Một lỗi nhỏ tìm thấy khi truy nguyên nhân:** doc-comment của `answer_with_image` trỏ tới `` [`quiet_crt_assert`] `` — **hàm đó không tồn tại**. Rustdoc sẽ cảnh báo liên kết hỏng, nhưng `cargo doc` không nằm trong CI nên không ai bắt. Đã sửa.

---

### U1a — Vision trên CUDA, đo xong

**Kết quả: 80 s → 1,2 s.** Cùng ảnh, cùng số token (nx=60 × ny=34 = 2 040), cùng model — thuần tăng tốc tính toán.

| | CPU (`--release`) | CUDA (`--release --features cuda`) | Tỉ lệ |
|---|---|---|---|
| Mã hoá ảnh | 47 842 ms | **563 ms** | **85×** |
| Giải mã 4 batch token ảnh | 27 957 ms | **291 ms** | **96×** |
| **Trọn một lượt `vision:ask`** | **80,2 / 81,3 / 79,0 s** | **2,1 / 1,2 / 1,2 s** | **~67×** |

`e2e-gateway.mjs` trên bản CUDA: **8/8**, `vision:ask` **1 191 ms**. Câu trả lời vẫn đúng và chi tiết (đọc được trang Facebook, nhóm, người gọi đến). VRAM đỉnh **4 510 / 16 311 MiB** — còn rất nhiều chỗ trống.

⇒ **Kết luận của U1 bị đảo ngược: vision KHÔNG chậm, nó chỉ đang chạy sai thiết bị.** 1,2 s là nằm trong ngưỡng hội thoại. Câu "đừng đưa vision vào demo trực tiếp" chỉ còn đúng cho **bản CPU**.

**Cách tái lập:**

```powershell
cd liva-native-core
cargo build --release --features cuda --bin liva-native-core
# rồi chạy gateway với GPU:
$env:LIVA_LLM_N_GPU_LAYERS = "99"   # 28 lớp LLM + 24 lớp tháp thị giác
```

Kiểm GPU có thật sự vào cuộc trước khi tin số đo — log phải có `ggml_cuda_init: found 1 CUDA devices` và `layer N assigned to device CUDA0`. Không có hai dòng đó thì bạn đang đo lại CPU.

**Hai cái giá, đều đo được — và chúng đẻ ra [U1b](#u1b--ghim-cudaarchs-và-quyết-định-cách-phát-hành):**

| | CPU | CUDA |
|---|---|---|
| Thời gian build | 1 phút 24 (tăng dần) | **19 phút 57** |
| Kích thước binary | 43,4 MB | **202,5 MB** (×4,7) |

Riêng `ggml-cuda.lib` là **218 MB**, vì `llama-cpp-sys-2` **không** đặt `CMAKE_CUDA_ARCHITECTURES` (`build.rs` chỉ bật `GGML_CUDA=ON` + `GGML_CUDA_NCCL=OFF`), nên llama.cpp biên dịch 183 file `.cu` cho **toàn bộ danh sách kiến trúc mặc định** thay vì riêng sm_120 của máy này.

---

### U1b — Ghim `CUDAARCHS` và quyết định cách phát hành

**Vì sao.** U1a chứng minh vision cần GPU. Nhưng bản CUDA **202,5 MB** và **~20 phút build** là hai con số phải xử lý trước khi nó tới tay ai, và nó kéo theo một quyết định sản phẩm chứ không chỉ kỹ thuật.

**Việc 1 — ghim kiến trúc (rẻ, đo được ngay).** CMake ≥ 3.20 đọc biến môi trường chuẩn **`CUDAARCHS`** làm mặc định cho `CMAKE_CUDA_ARCHITECTURES`; `build.rs` không định nghĩa biến đó nên đường này còn trống:

```powershell
$env:CUDAARCHS = "120"   # sm_120 = Blackwell, đúng RTX 5060 Ti
cargo build --release --features cuda
```

**Nghiệm thu:** ghi lại thời gian build và kích thước binary mới, đặt cạnh 19 phút 57 / 202,5 MB. Nếu không giảm đáng kể thì giả thuyết sai — **ghi lại là sai**, đừng bỏ lửng.

**Việc 2 — quyết định phát hành, và đây mới là phần khó.** Build script của `llama-cpp-sys-2` (trong registry cargo, ngoài repo nên không trích toạ độ được) phát `cargo:rustc-link-lib=cudart` ⇒ bản CUDA phụ thuộc `cudart64_*.dll`, và **vô dụng hoàn toàn trên máy không có GPU NVIDIA**. Bối cảnh beta là 5 người chạy laptop; không ai bảo đảm họ có card rời. Ba đường, phải chọn có ý thức:

| Đường | Giá |
|---|---|
| Chỉ phát hành bản CPU | Vision 80 s ⇒ trên thực tế là **không có vision** |
| Chỉ phát hành bản CUDA | Người không có NVIDIA **không chạy được LIVA**, không chỉ mất vision |
| Phát hành hai bản | Installer to gấp đôi, phải dò GPU và hướng dẫn chọn đúng |

**Nghiệm thu:** một quyết định được ghi vào `README.md` + [`02-van-hanh/03`](../02-van-hanh/03-trien-khai-va-runtime.md), kèm hành vi khi người dùng chạy bản CUDA trên máy không có NVIDIA — phải **báo lỗi rõ**, không được sập hay im lặng.

---

#### ✅ ĐÃ ĐO XONG — 26/07/2026

**Việc 1 — ghim `CUDAARCHS`: hiệu quả lớn, không mất hiệu năng.**

| | 9 kiến trúc | Ghim `120a-real` | |
|---|---|---|---|
| Thời gian build | 19m57 *(có cache một phần)* | **6m17** *(từ `out/` trắng)* | **−68%** |
| Binary | 202,5 MB | **74,5 MB** | **−63%** |
| `ggml-cuda.lib` | 218,2 MB | **86,5 MB** | **−60%** |
| `vision:ask` | 2,1 / 1,2 / 1,2 s | **2,9 / 1,4 / 1,4 s** | cùng dải |
| `e2e-gateway.mjs` | 8/8 | **8/8** (vision 1 495 ms) | — |

So sánh thời gian là **cận dưới** của mức tiết kiệm thật, không phải con số chính xác: bản 19m57 còn cache một phần, còn bản 6m17 build từ trắng.

**Bằng chứng ghim có tác dụng** (đừng tin nếu không thấy dòng này): `CMakeCache.txt` chuyển từ
`CMAKE_CUDA_ARCHITECTURES=50-virtual;61-virtual;70-virtual;75-virtual;80-virtual;86-real;89-real;90-virtual;120a-real`
sang `CMAKE_CUDA_ARCHITECTURES:STRING=120a-real`. Nguyên nhân nhánh rộng: `GGML_NATIVE:BOOL=OFF` ⇒ `ggml-cuda/CMakeLists.txt` bỏ qua `"native"` và dùng danh sách phủ rộng.

⚠️ **Hai cạm bẫy, và cạm bẫy thứ hai suýt khiến tôi báo cáo "ghim không giúp gì".**
1. `CUDAARCHS` **không** nằm trong `rerun-if-env-changed` của `build.rs` ⇒ đổi nó một mình không kích hoạt build lại.
2. `cargo clean -p llama-cpp-sys-2` **không xoá** `target/release/build/llama-cpp-sys-2-*/out/` ⇒ `CMakeCache.txt` cũ sống sót, cmake không cấu hình lại, `CUDAARCHS` không được đọc. Lần thử đầu "xong" trong **42,9 giây** với kích thước y nguyên — con số quá nhanh là dấu hiệu duy nhất cho biết phép đo hỏng. **Phải xoá tay thư mục đó.**

**Việc 2 — quyết định phát hành: giả định của chính mục này SAI, và câu trả lời đúng gọn hơn.**

| Tình huống | Kết quả | Cách kiểm |
|---|---|---|
| Có GPU + CUDA toolkit | vision 1,4 s | trực tiếp |
| Có runtime, **không thấy GPU** | log `ggml_cuda_init: failed to initialize CUDA: no CUDA-capable device is detected`, rồi **`layer N assigned to device CPU`** — **không sập**, vẫn phục vụ | `CUDA_VISIBLE_DEVICES=-1` |
| **Thiếu CUDA runtime** | **exit 127, không một thông báo nào** — chết ở tầng nạp DLL trước khi mã LIVA chạy | bỏ thư mục CUDA khỏi `PATH` |
| Phát hành kèm đủ DLL | exit 0, chạy bình thường, thấy GPU | đặt DLL cạnh exe + `PATH` đã lược |

Mục này viết *"bản CUDA vô dụng hoàn toàn trên máy không có GPU NVIDIA"* — **sai**. Nó **rơi về CPU đúng cách**. Vấn đề thật không phải GPU mà là **DLL runtime**, và cái giá là:

| DLL phải phát hành kèm | |
|---|---|
| `cudart64_12.dll` | 0,5 MB |
| `cublas64_12.dll` | 108,4 MB |
| `cublasLt64_12.dll` | **643,4 MB** |
| **Tổng** | **752,4 MB** |

Chỉ `nvcuda.dll` và `nvml.dll` đi kèm driver; hai thư viện cuBLAS thì **không** — chúng thuộc toolkit và phải tự phát hành lại.

⇒ **Khung quyết định đúng: MỘT bản CUDA phục vụ được mọi máy** (có GPU thì 1,4 s, không có thì rơi về CPU), giá là **74,5 MB binary + 752 MB DLL ≈ 830 MB** trước model — chứ không phải "phải làm hai bản". Ba đường ban đầu của mục này đặt sai câu hỏi.

**Việc còn lại:** nếu ~830 MB là quá đắt thì hướng đúng là bỏ phụ thuộc cuBLAS — xem [U1c](#u1c--thử-bỏ-phụ-thuộc-cublas-ba-hướng-đều-thất-bại), đã đo và **cả ba hướng đều thất bại**.

---

### U1c — Thử bỏ phụ thuộc cuBLAS: ba hướng đều thất bại

**Kết luận: `cublas64_12.dll` + `cublasLt64_12.dll` (752 MB) là phụ thuộc CỨNG.** Không có đường vòng nào trong ba đường đã thử, và một đường **có hại**.

| Hướng | Kết quả | Bằng chứng |
|---|---|---|
| `GGML_CUDA_FORCE_MMQ=ON` | **Không giúp gì** — nó chỉ đổi kernel được *chọn*, không bỏ *liên kết* | `ggml-cuda/CMakeLists.txt:176` là `target_link_libraries(ggml-cuda PRIVATE CUDA::cudart CUDA::cublas)` — **vô điều kiện** |
| Chỉ phát hành `cublas`, bỏ `cublasLt` 643 MB | **exit 127** — `cublasLt` là phụ thuộc lúc-nạp của chính `cublas` | chạy với 2/3 DLL, `PATH` đã lược |
| `/DELAYLOAD` cả hai DLL | **CÓ HẠI** — xem dưới | đo trực tiếp |

Nhánh `GGML_STATIC` cũng không cứu được: chính CMakeLists ghi *"As of 12.3.1 CUDA Toolkit for Windows does not offer a static cublas library"*, nên trên Windows nó vẫn link cuBLAS động (`:160-161`).

**Vì sao `/DELAYLOAD` là hướng CÓ HẠI, không phải trung tính.** Nó *hoạt động* ở phần dễ: binary khởi động bình thường với **chỉ `cudart` 0,5 MB**, GPU init được, 28 lớp nạp lên `CUDA0`. Rồi `vision:ask` **chết im lặng** đúng tại dòng log `encoding image slice...` — phép nhân ma trận đầu tiên của bộ mã hoá thị giác. Không lỗi, không panic, không một dòng log; tiến trình biến mất, client chờ tới hết 300 s timeout.

Tức là delay-load **đổi một lỗi khởi động rõ ràng (`exit 127`) thành một cái chết âm thầm giữa lượt phục vụ** — đúng mô thức mà dự án này đã bỏ nhiều công để loại bỏ ở `smart_home` và ở ba lần "xanh giả" của CI. **Đừng dùng.** Nếu ai muốn thử lại, phải kèm handler cho SEH exception của delay-load stub để biến nó thành thông điệp thật — nhưng lúc đó vẫn là "vision không chạy", chỉ khác là biết vì sao.

**Một khoản giảm CÓ thật nhưng không nên chọn: dùng runtime CUDA 12.1.**

| | v12.8 | v12.1 |
|---|---|---|
| Tổng DLL | 752 MB | **561 MB** (−191 MB) |
| `vision:ask` nguội | 2,9 s | **9,9 s** |
| `vision:ask` ấm | 1,4 s | **1,8 s** |

Nó **chạy và trả lời đúng** với kernel sm_120. Nhưng đắt hơn về tốc độ (nguội ×3,4), và trộn runtime 12.1 với kernel biên dịch bởi toolkit 12.8 là thứ NVIDIA **không tài liệu hoá là được hỗ trợ**. Đổi 191 MB lấy rủi ro đó cộng độ trễ — không đáng, trừ khi dung lượng là ràng buộc cứng.

⇒ **Chốt lại cho U2:** installer phải tính **74,5 MB binary + 752 MB DLL NVIDIA ≈ 830 MB** trước model, và con số đó **không nén xuống được** bằng cấu hình build. Ba đường đã thử hết. Nếu 830 MB không chấp nhận được thì lựa chọn còn lại nằm ở tầng *sản phẩm*, không phải tầng build: phát hành hai bản (CPU nhẹ / CUDA đầy đủ), hoặc tải DLL về sau lần chạy đầu.

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

### U3 — Lệnh `preflight` báo trạng thái tài nguyên — **[MỘT PHẦN]** CLI xong 26/07/2026, UI còn nợ

**Vì sao.** Khi khởi động lõi ngày 26/07/2026, thiếu voice embedding Kokoro chỉ tạo ra **một dòng WARN** lẫn giữa hàng trăm dòng log ONNX. Người dùng thật sẽ không thấy. Đã biết ít nhất ba thứ **suy giảm im lặng**: model embedding thiếu → RAG thành no-op; voice Kokoro thiếu → mất một backend TTS; model LLM sai đường dẫn → không có não.

Suy giảm im lặng là kiểu lỗi tệ nhất cho beta tester offline: sản phẩm "chạy" nhưng cụt tính năng, và họ không có cách nào biết vì sao.

**Phát hiện khi bắt tay vào làm: một nửa việc đã có sẵn, và nửa còn lại không phải nửa mình tưởng.** `npm run doctor` (`scripts/models.mjs`) đã kiểm 11 năng lực model kèm hệ quả, override và lệnh tải — tức phần "thiếu model" đã xong từ trước. Nhưng nó là script Node, nên **không thể** trả lời được đúng những chế độ hỏng đã ngốn thời gian thật ở U1–U1c:

| Chế độ hỏng thật | `doctor` thấy được? | Hậu quả |
|---|---|---|
| Binary build ở profile `debug` | Không | `vision:ask` trả lỗi ngay, model có đủ vẫn vô dụng |
| Build thiếu `--features cuda`, hoặc có CUDA mà không thấy GPU | Không | vision ~80 s/lượt thay vì ~1,4 s — chạy được nhưng ngoài ngưỡng hội thoại |
| Đủ cả release + CUDA + GPU nhưng `LIVA_LLM_N_GPU_LAYERS` để mặc định `0` | Không | vẫn ~80 s/lượt, GPU đứng không — **cấu hình dễ tưởng là xong nhất**, xem bẫy 3 |
| `espeak-ng` / `ffmpeg` không có trên PATH | Không | mất G2P cho TTS / mất voice Telegram |
| `vec0` không nạp được | Không | **chặn khởi động** — không mở nổi DB |
| `LIVA_ENCRYPTION_KEY` đang là khoá mặc định công khai | Không | mã hoá `facts` không bảo vệ gì |
| `TELEGRAM_ALLOWED_IDS` rỗng khi đã có token | Không | allow-list fail-closed (`liva-native-core/src/telegram.rs#is_authorized`) → bot từ chối **cả chủ máy** |

Không có dòng nào trong bảng trên là "thiếu file model". Nên U3 không phải làm lại `doctor`, mà làm phần bù của nó.

**Đã làm.** `liva-native-core/src/preflight.rs` + cờ `--preflight` xử lý ở đầu `main()`, **trước** khi dựng runtime Tokio và trước mọi khởi tạo — đó là cả điểm của nó, phải trả lời được "máy này thiếu gì" trên đúng cái máy chưa boot nổi. Là module của **binary**, không của lib, nên `lib.rs` không phải mở thêm API công khai nào.

**Luôn `exit 0`** — báo cáo, không phải cổng kiểm; cố ý khác `doctor` (thoát 1 khi thiếu file bắt buộc). Có unit test khoá hợp đồng này lại, để đổi sang `exit 1` phải là một quyết định có chủ đích chứ không phải trôi.

`scripts/start_all.ps1 -CheckOnly` giờ chạy **cả hai**: `--preflight` (ưu tiên đọc bản `release`, vì dòng vision phụ thuộc profile) rồi `npm run doctor`.

**Ba cái bẫy gặp trong lúc làm.** Hai cái đầu là số vô nghĩa chứ không phải lỗi build — cùng một kiểu, ghi lại để nhận ra lần sau:

1. `governor::gpu_vram_bytes()` trả `(tổng, đang dùng)`. Đảo thứ tự thì in ra `16311 / 1843 MiB đang dùng` — vẫn compile, vẫn "chạy", chỉ là dùng nhiều hơn tổng. Bắt được bằng cách **đọc số**, không bằng cách chạy test.
2. `config_file_path()` dò `data/liva-config.json` từ cwd rồi hai cấp trên; hụt thì rơi về `DEFAULT_ROUTER_MODEL` — hiện **vẫn là `gemma-4-E4B`**, tức không phải router thật của dự án (Qwen3-VL). Chạy preflight từ sai thư mục cho ra một dòng ✓ trỏ vào model sai. Đã thêm hàng "Cấu hình" **đứng trước** hai hàng model để bảng tự giải thích; còn bản thân giá trị mặc định lạc hậu trong `lib.rs` là việc riêng, chưa sửa.

3. **Cái thứ ba đáng kể hơn hai cái trên, và nó là một "xanh giả" trong chính U3.** Bản đầu tiên coi vision là đủ khi có **ba** điều kiện release + CUDA + GPU, rồi *nhắc* `LIVA_LLM_N_GPU_LAYERS` trong lời khuyên. Nhưng biến đó mặc định **0** (`liva-native-core/src/boot.rs:177-180`), và `MtmdContextParams.use_gpu` được đặt bằng `n_gpu_layers > 0` — nên một máy đủ cả ba điều kiện, không đặt biến, vẫn chạy vision **~80 s mỗi lượt** trong khi preflight in ✓ kèm dòng "Đo được ~1,4 s". Tức đúng loại lỗi mà U3 sinh ra để bắt, do chính U3 tạo ra.

   Đã sửa thành **bốn** điều kiện, và `n_gpu_layers` giờ là một **phép kiểm** chứ không phải một câu nhắc. Kiểm chứng bằng hai lần chạy cạnh nhau trên cùng máy: không đặt biến → `✗ đủ release + CUDA + GPU, nhưng LIVA_LLM_N_GPU_LAYERS = 0`; đặt `999` → `✓ … n_gpu_layers = 999`.

   **Điều đáng chú ý: tài liệu không hề sai chỗ này.** `02-van-hanh/03` mục 4.3 đã ghi đúng cả cơ chế (`use_gpu` bật theo `n_gpu_layers`), `02-van-hanh/01` ghi đúng mặc định `0` và cả chỗ lệch với `.env.example:37`. Kiến thức có đủ, ở **sáu** chỗ khác nhau. Vấn đề là không ai **kiểm được nó đúng lúc cần** — người dùng chỉ thấy "vision chạy, mà chậm". Nên bài học không phải "tài liệu hoá kỹ hơn" mà là: *kiến thức nằm trong văn bản chỉ có giá trị bằng khả năng kiểm nó ngay trên máy đang hỏng.* Đó chính là lằn ranh giữa U4 (sửa tài liệu) và U3 (biến tài liệu thành phép kiểm).

**Đã kiểm chứng.** Ba môi trường, cả ba `exit 0`:

| Môi trường | Kết quả |
|---|---|
| release + CUDA + GPU, chạy từ gốc repo | 11 hàng, "không thiếu gì trong phạm vi preflight" |
| debug | vision → ✗ kèm đúng lời khuyên `cargo build --release` |
| PATH rút còn `System32`, cwd ngoài repo, khoá mặc định, token Telegram không allow-list | 7 hàng mất năng lực, không panic, vẫn `exit 0` |

Cổng: 27/27 test của bin target xanh, clippy `-D warnings` exit 0 (đã xác nhận clippy **thật sự** đọc `preflight.rs` bằng cách bật `-W clippy::pedantic` và thấy nó bắn lint trong file — nếu chỉ tin con số 0 thì đó đúng là kiểu "xanh giả" mà U5 nói).

**Còn nợ (U3b).** Màn hình UI hiện bảng này. Hoãn có lý do cụ thể, không phải quên: lệnh cho UI phải đi qua `handle_command` trong `lib.rs`, mà `lib.rs` + `liva-ui` đang có phiên khác sửa (việc skill-store G2) — chạm vào là xung đột. `preflight.rs` chỉ dùng API công khai của lib nên chuyển sang lib sau này là việc cơ học. Thứ tự đúng: đợi việc kia đáp xuống, rồi chuyển module + thêm lệnh + màn hình.

**File.** `liva-native-core/src/preflight.rs` (mới), `liva-native-core/src/main.rs`, `scripts/start_all.ps1`.

**Nghiệm thu.** Ba tiêu chí ban đầu, giữ nguyên chữ:

| Tiêu chí | |
|---|---|
| Chạy trên máy thiếu **mọi** model → in bảng đầy đủ, exit 0 (báo cáo chứ không chết) | **đạt** |
| `scripts/start_all.ps1 -CheckOnly` gọi nó | **đạt** |
| UI có chỗ hiện đúng bảng đó | **chưa** — xem U3b |

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

#### ✅ ĐÃ LÀM — 26/07/2026

**Nghiệm thu đạt:** `node scripts/docs-check.mjs` không còn liệt kê file `03-danh-gia/` nào; `docs-citations.mjs` quét 2 184 toạ độ, 0 hỏng; cả hai exit 0.

**Hoá ra tệ hơn ba dòng đã dự đoán — đảo được 11 phán quyết, tất cả cùng một chiều.** Không mục nào là tài liệu thổi phồng sản phẩm; **toàn bộ đều là tài liệu hạ thấp sản phẩm**:

| Tài liệu | Mục | Bản trước nói | Thực tế |
|---|---|---|---|
| `01` | Smart Home | "thành công **vô điều kiện** — nguy hiểm hơn cả việc thiếu" | Báo trung thực, có test ép |
| `01` | MCP server Rust | "**MỒ CÔI**" | Đã nối; e2e sống xác nhận 4 tool |
| `01` | Bộ nhớ dài hạn | "`chat:completion` **chưa** lưu ký ức" | e2e 6/6 với model thật |
| `01` | Router ý định | "`contains()` trên chuỗi thường" | Token trọn vẹn + tiếng Việt + test hồi quy |
| `02` | **C2** | rủi ro CRITICAL đang mở | `validate_model_path` vá ở **hai** điểm |
| `02` | **C3** | "giải mã **fail-open**" | `FactRead::Locked`, fail-**closed**, có test qua dispatcher |
| `02` | **H4** | rủi ro HIGH đang mở | Đã khép; chuỗi tấn công đứt ở **hai** chỗ độc lập |
| `02` | **H6** | "**không có** hệ thống migration DB" | `SCHEMA_VERSION=3` + `run_migrations`; log khởi động chứng minh |
| `02` | `mcp::client` | "**KHÔNG AI GỌI** — mồ côi 49 dòng" | MCP client stdio thật, ~1 035 dòng, e2e 4/4 |
| `03` | **3.5** | "CI **không có** gate fmt/clippy nào" | Clippy là **gate cứng**; đo lại 0 warning |
| `03` | **§9** | 9 dòng đã xong vẫn hiện như tồn đọng | Đã đồng bộ với mục chi tiết |

**Điều đáng ghi lại nhất không phải danh sách trên, mà là chiều lệch của nó.** Một bộ tài liệu tự-phê-bình có xu hướng hỏng **theo hướng bi quan**: mỗi lần vá một rủi ro, người ta sửa code và quên sửa bản kiểm kê rủi ro. Tích luỹ đủ lâu thì tài liệu mô tả một sản phẩm tệ hơn sản phẩm thật — và với hồ sơ dự thi thì đó là thiệt hại tự gây, không sửa hồi tố được. **Bài học vận hành:** `docs-check.mjs` phát hiện được **cả 11** mục, nhưng chỉ ở mức *cảnh báo* — nên chúng sống nhiều ngày. Đó chính là luận cứ cho [U5](#u5--biến-drift-tài-liệu-thành-gate-thật).

**Hai cạm bẫy gặp phải, ghi lại để phiên sau không lặp:**

1. **Không sửa hàng loạt file `docs/` bằng PowerShell.** `Get-Content -Raw` trên PS 5.1 đọc file UTF-8-không-BOM bằng codepage ANSI (1252), rồi `Set-Content -Encoding utf8` ghi lại kèm BOM → **mã hoá hai lần**, toàn bộ tiếng Việt thành mojibake. `docs-check` vẫn *pass* sau đó vì cấu trúc còn nguyên — nó chỉ báo lỗi ở bước BOM, không báo mojibake. Dùng công cụ Edit, hoặc `[System.IO.File]::ReadAllText/WriteAllText` với `UTF8Encoding($false)` tường minh. **Cách phát hiện:** `git diff --stat` — nếu số dòng thay đổi ≈ tổng số dòng file thì mã hoá đã hỏng, không phải bạn sửa nhiều.
2. **HEAD dịch chuyển giữa chừng.** Trong đúng phiên này HEAD đi `ce1697a → 45e2e58 → b07d69d → 0b490b9`. Bump `commit:` xong rồi vẫn bị gắn cờ lỗi thời là chuyện bình thường, không phải lỗi. Bump **cuối cùng**, sau khi đã sửa xong nội dung.

**Chưa làm, cố ý:** MEDIUM và LOW của `02` (21 mục) **không được rà lại** trong đợt này — đã ghi rõ điều đó ngay trong §0 của `02` thay vì để người đọc tưởng cả tài liệu vừa được xác minh. 16 file lỗi thời ngoài `03-danh-gia/` cũng còn nguyên, nằm ngoài phạm vi nghiệm thu của U4.

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

#### ✅ ĐÃ LÀM — 26/07/2026

**Đã LỆCH khỏi đặc tả bên trên, có chủ đích.** Mục này viết "chọn (a) hoặc (b), **đừng làm cả hai**". Sau khi làm [U4](#u4--đồng-bộ-03-danh-gia-với-code) thì thấy lời khuyên đó sai, nên đã làm **(a) với (b) làm van thoát**. Lý do là một phép đo, không phải sở thích:

> **18/30 commit gần nhất chạm `liva-native-core/src/`**, mà `01`, `02`, `03` đều khai `covers: liva-native-core/src/*`. Siết thô theo (a) ⇒ gate đỏ ở **60% commit**, và cách dập duy nhất là sửa một dòng hash. Một gate nổ liên tục mà dập được bằng một dòng hash sẽ bị **dập mù** — biến cảnh báo trung thực hôm nay thành **xanh dối**, tức tệ hơn hiện trạng. (a) một mình không sống được; (b) một mình không chặn gì.

Van thoát hoạt động được là nhờ tách hai lời khẳng định vốn đang bị gộp:

| Trường | Nghĩa | Dùng khi |
|---|---|---|
| `commit:` | "Tôi đã đối chiếu **nội dung** tài liệu tới commit này" | Bạn **có sửa** nội dung |
| `stale-ok:` | "Tôi đã **đọc diff** tới commit này, không cần sửa gì" | Bạn đọc xong và kết luận tài liệu vẫn đúng |

Cả hai đều là một dòng, nhưng chỉ cái sau **trung thực** khi bạn không sửa gì. Bằng chứng đây không phải phân biệt lý thuyết: ngay trong phiên này, `docs/README.md` chỉ được thêm mục điều hướng chứ không đối chiếu lại `covers`, nên `commit:` của nó **cố ý giữ nguyên** — bump lên sẽ dập một cảnh báo thật. Trước U5 thì không có cách nào diễn đạt điều đó ngoài việc… không làm gì.

`stale-ok` còn **kiểm toán được trong một lệnh**: `grep -rn "stale-ok:" docs/` cho biết tài liệu nào đang sống nhờ van thoát và từ commit nào.

**Đã thay đổi:**

| File | Nội dung |
|---|---|
| `scripts/docs-check.mjs` | Cờ `--strict-stale=<thư mục,…>`; trường `stale-ok`; nới regex khoá front-matter cho `-`; in `headSha` sẵn trong thông điệp lỗi để copy-paste; tách báo cáo thành khối **CHẶN** và khối **cảnh báo** |
| `.github/workflows/test.yml` | Bước *Check Documentation* nay chạy `--strict-stale=docs/03-danh-gia` |
| `docs/_meta/huong-dan-bao-tri.md` | Lược đồ front-matter thêm `stale-ok`; mục mới giải thích vì sao hai trường không thay nhau được |
| `docs/README.md`, `CLAUDE.md` | Quy trình cập nhật + lệnh khớp CI |

**Nghiệm thu — đã chạy, 5/5 kịch bản đúng.** Dựng một tài liệu tạm trong đúng phạm vi strict (`covers: bin/tool_calling_probe.rs`, file có đổi ở cả `45e2e58` lẫn `0b490b9`):

| # | Tình huống | Kỳ vọng | Thực tế |
|---|---|---|---|
| 1 | `commit` cũ, không `stale-ok`, strict | chặn | `exit 1` ✅ |
| 2 | `commit` cũ, `stale-ok` = HEAD, strict | qua | `exit 0` ✅ |
| 3 | `commit` cũ, `stale-ok` = `45e2e58` (**phủ thiếu**) | chặn | `exit 1` ✅ |
| 4 | `stale-ok` = sha không tồn tại | chặn, báo rõ | `exit 1` ✅ |
| 5 | Cùng tài liệu đó, **không** có cờ strict | chỉ cảnh báo | `exit 0` ✅ |

Kịch bản 3 là kịch bản đáng giá nhất: `stale-ok` **không** phải công tắc tắt vĩnh viễn — nó chỉ phủ đúng phần diff nó nói tới, thay đổi phát sinh sau vẫn nổi lên. Kịch bản 5 chứng minh tương thích ngược: không truyền cờ thì hành vi y hệt trước.

Trên repo thật: `--strict-stale=docs/03-danh-gia` → **exit 0** (nhờ U4 đã dọn sạch trước). `--map` và `docs-citations` không vỡ.

**Gate đã tự chứng minh trên dữ liệu thật, vài phút sau khi bật.** Trong lúc đang viết mục này, HEAD nhảy sang `bedff83` và gate nổ với cả 5 tài liệu. Kết quả khi áp đúng quy trình — **mỗi tài liệu ra một quyết định khác nhau**, đúng thứ một công tắc tắt-mở không làm được:

| Tài liệu | Đọc diff thấy gì | Quyết định |
|---|---|---|
| `01`, `02`, `03` | Diff là một phép đo + một helper; không phán quyết nào đổi | `stale-ok: bedff83` |
| `04` | Đã được đối chiếu trong **chính commit đó** | `commit: bedff83` |
| `05` | **Nội dung THẬT SỰ sai** — mục U12 đang ghi ngưỡng tiền lọc là "dấu hiệu khả thi", mà `bedff83` vừa đo và bác bỏ nó | Sửa nội dung → `commit: bedff83` |

Nếu chỉ có `commit:` thì cả 5 đã bị bump giống nhau và **cái sai ở `05` lọt** — đúng kịch bản hỏng mà U4 vừa dọn 11 lần. Kiểm toán: `grep -rn "stale-ok:" docs/` → 3 tài liệu, tất cả ở `bedff83`.

**Cố ý KHÔNG làm:** siết cho toàn `docs/` — 16 file ngoài `03-danh-gia/` sẽ đỏ ngay và gate sẽ bị tắt, mất luôn cảnh báo đang có. Siết từng thư mục một, **sau khi** đã dọn thư mục đó.

**Một nguồn nhiễu đã biết, phát hiện ngay khi commit U6.** Tài liệu này khai `AGENTS.md` trong `covers` (vì [U6](#u6--sửa-con-trỏ-chết-trong-agentsmd) nói về nó), mà `AGENTS.md` chứa **khối máy sinh** giữa `<!-- gitnexus:start -->` và `<!-- gitnexus:end -->`. Hệ quả: **mỗi lần chạy lại indexer là mục này bị gắn cờ lỗi thời**, dù không một chữ nào của người viết thay đổi. Bộ dò so theo *file*, không theo *vùng trong file*, nên nó không phân biệt được. Chưa sửa vì hai cách đều có giá: bỏ `AGENTS.md` khỏi `covers` thì mất cảnh báo cho thay đổi THẬT, còn dạy checker bỏ qua vùng máy sinh thì phải biết mọi dấu mốc như vậy. Tạm thời: cứ `stale-ok` khi diff chỉ là dòng chỉ số — nhưng **phải mở diff ra xem**, đừng đoán.

**Một tính chất cấu trúc, không phải lỗi: gate KHÔNG THỂ xanh bên trong chính commit gây ra nó.** Một commit vừa sửa file mã nguồn nằm trong `covers` vừa sửa tài liệu sẽ **luôn** để tài liệu chậm một nhịp — vì tài liệu không thể ghi SHA của commit đang chứa chính nó. Ví dụ có thật: `110587a` (U1) sửa doc-comment trong `llm/engine.rs`, thế là ba tài liệu `01`/`02`/`03` bị gắn cờ ngay sau khi commit, dù nội dung chúng không sai một chữ. Hệ quả thực hành: **đừng cố làm gate xanh trong cùng một commit** — dọn ở commit kế tiếp là đúng quy trình, không phải nợ.

**Phần mở rộng do phiên song song làm, cùng ngày, cùng tinh thần — và nó tìm ra "xanh giả" lần thứ ba.** U5 để ngỏ hướng "drift ngữ nghĩa của trích dẫn `file:dòng`" vì khó. Cách giải hoá ra không phải làm bộ dò thông minh hơn, mà là **đo cái đang bị bỏ qua**: `docs-citations.mjs` vốn có biến `skipped` cho trích dẫn *không kết luận được* (tên file trần như `lib.rs:1055` trùng nhau giữa nhiều file) — nhưng **chưa từng in nó ra**. Hệ quả: **39 % trích dẫn (853/2 184) chưa bao giờ được kiểm**, trong khi cổng vẫn xanh và tài liệu vẫn khoe "2 000 toạ độ có gate".

Nay có `--max-unchecked=<N>` làm **chốt chống thụt lùi** (CI đang đặt `508`): con số hiện ra, **chỉ được phép giảm**, và mỗi lần chuyển một nhóm sang **neo ký hiệu** (`file.rs#symbol`, gợi ý bằng `--suggest`) thì hạ chốt. Đây là cách đúng để tiêu hoá một khoản nợ lớn: không đòi về 0 ngay, chỉ cấm đi lùi.

Ba lần "xanh giả" của dự án này giờ có chung một hình dạng, đáng ghi lại thành luật: **`tsc` kiểm 0 file · `vitest` không áp ngưỡng · `docs-citations` bỏ qua 39 % — cả ba đều là cổng chạy xong, báo thành công, và không hề kiểm thứ người ta tưởng nó kiểm.** Khi thêm bất kỳ cổng nào, câu hỏi bắt buộc không phải "nó có chạy không" mà **"nó có bao giờ đỏ được không, và nó im lặng bỏ qua bao nhiêu?"**

---

### U6 — Sửa con trỏ chết trong AGENTS.md

**Vì sao.** Mục "Rust Migration Plan" trong `AGENTS.md` trỏ tới `LIVA_NATIVE_MIGRATION_PLAN.md` ở **gốc repo** — file đó đã chuyển vào `docs/99-luu-tru/ke-hoach-da-hoan-thanh/`. `docs-check.mjs` chỉ quét trong `docs/` nên không bắt được. Mọi phiên agent đều đọc `AGENTS.md`, nên một con trỏ chết ở đây tốn thời gian của **mọi** phiên sau.

**Việc.** Sửa đường dẫn. Cân nhắc mở rộng `docs-check.mjs` quét thêm `AGENTS.md`, `CLAUDE.md`, `README.md` — ba file này được đọc nhiều nhất mà lại nằm ngoài mọi cổng kiểm liên kết.

**Nghiệm thu.** Không còn liên kết tương đối hỏng trong ba file gốc repo.

---

#### ✅ ĐÃ LÀM — 26/07/2026

**Nghiệm thu đạt:** `docs-check.mjs` báo **0 lỗi** cho `AGENTS.md`, `CLAUDE.md`, `README.md`.

**Khảo sát trước khi sửa cho thấy giả định của mục này chưa đủ.** Ba file gốc có **19 liên kết markdown tương đối và cả 19 đều sống** — không có "liên kết hỏng" nào. Con trỏ chết thật sự là một **đường dẫn trần trong văn xuôi**, không phải link:

```
AGENTS.md:26   `E:\Project\LIVA\LIVA_NATIVE_MIGRATION_PLAN.md`
```

Nghĩa là nếu chỉ mở rộng bộ kiểm **liên kết** sang ba file gốc như đề xuất, nó vẫn **trượt đúng cái bug sinh ra mục này**. Phải thêm một luật thứ hai.

**Đã quét toàn bộ `docs/` + 3 file gốc** cho đường dẫn tuyệt đối trỏ vào repo: 67 chỗ, **8 chết**. Phân loại:

| Loại | Số | Xử lý |
|---|---|---|
| `E:\Project\LIVA\.env` | 3 | **Hợp lệ** — file bị gitignore, tài liệu cố ý mô tả nó |
| `docs\reports\*` trong `00-…khao-sat-goc` | 2 | **Đóng băng** — ảnh chụp lịch sử, không được sửa |
| `docs\reports\*` trong `01-ban-ve/10` | 2 | Bug thật, cùng lớp — **đã sửa** |
| `LIVA_NATIVE_MIGRATION_PLAN.md` trong `AGENTS.md` | 1 | Mục tiêu U6 — **đã sửa** |

Ba chỗ chết thật đều là cùng một sự kiện: đợt quy hoạch tài liệu 21/07/2026 chuyển file vào `99-luu-tru/` mà không vá con trỏ. Tất cả nay trỏ đúng và là **liên kết markdown** (kiểm được tự động), không còn là đường dẫn trần.

**Đã thêm vào `scripts/docs-check.mjs`:**

1. **Ba file gốc vào phạm vi kiểm liên kết.** `AGENTS.md` + `CLAUDE.md` được **mọi phiên agent** đọc, `README.md` được mọi người mới đọc — mà trước đó chúng là ba file duy nhất nằm ngoài mọi cổng kiểm liên kết.
2. **Luật mới: đường dẫn tuyệt đối trỏ vào repo nhưng không tồn tại → LỖI.** Áp cho cả `docs/`, với hai miễn trừ có lý do: file bị **gitignore** (`.env` — vắng mặt là bình thường) và tài liệu **`frozen`** (ảnh chụp lịch sử, cùng lý do `docs-citations.mjs` bỏ qua chúng). Đường dẫn tuyệt đối trỏ **ra ngoài** repo (`E:\AI_Models`, `C:\Program Files\…`) **không** bị bắt — đó là tài liệu hoá môi trường máy người dùng, hợp lệ, và có 59 chỗ như vậy.

**Một lỗi tôi tự gây và tự bắt được, đáng ghi lại.** Bản đầu của luật 2 dùng `stripCode()` — hàm này xoá **cả inline `` `code` ``**. Nhưng quy ước của bộ tài liệu là **bọc mọi đường dẫn trong backtick**, nên bộ dò bị mù đúng thứ nó cần thấy: chốt-kiểm cho thấy nó bắt được liên kết hỏng nhưng **KHÔNG** bắt được đường dẫn chết — tức sẽ trượt đúng con trỏ trong `AGENTS.md`. Đã thêm `stripCode(text, keepInline = true)`: chỉ bỏ khối ``` (ví dụ lệnh), giữ inline code. **Bài học chung: một bộ kiểm phải được thử bằng chính cái bug sinh ra nó**, nếu không thì rất dễ có một cổng xanh mà vô dụng — đúng mô thức "xanh giả" đã hai lần xảy ra với CI của dự án này.

**Kiểm chứng:** chèn tạm vào `AGENTS.md` một liên kết hỏng **và** một đường dẫn chết → checker bắt **cả hai**, 0 dương tính giả ở nơi khác; gỡ ra → sạch.

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

#### ✅ PHẦN LỚN ĐÃ XONG — 26/07/2026 (`e2ecdf1`), và tiền đề của mục này hoá ra đã sai

**Chọn (a) — nhưng phần lớn (a) đã đúng từ trước.** Khi đối chiếu lại mã nguồn, **hai trong ba khẳng định của chính mục này là sai**:

| Mục này khẳng định | Thực tế |
|---|---|
| "Tauri không có WS server" | **Sai từ trước** — vỏ Tauri vẫn spawn `WebSocketServer::bind_from_env()` |
| "Tauri hard-code bốn module thoại thành `None`" | **Sai từ trước** — vỏ Tauri đã gọi `VoiceRuntimeComponents::from_env` |
| "bot Telegram chỉ sống ở standalone" | **Đúng cho tới 26/07/2026** — nay bot chạy ở cả hai vỏ |

Nên câu kết luận kịch tính của mục này — *"cướp lời giữa câu, người dùng thật không bao giờ thấy"* — **không đúng**. VAD và denoise **mặc định BẬT** ở mọi vỏ (`LIVA_VAD_ENABLED`/`LIVA_DENOISE_ENABLED` default `true`); turn-shadow và AEC vẫn opt-in.

**Đã làm thật:** `liva-native-core/src/boot.rs` gộp đường khởi động hai vỏ — `build_app_state()` dựng toàn bộ `AppState`, `spawn_background_services()` bật mọi dịch vụ nền ở **một** chỗ (projection consumer · tự nạp router · governor GPU · **WebSocket** · **TTS idle-unload** · **Telegram**). Hai vỏ co lại **−621 dòng**. Khác biệt thật còn lại đóng khung trong `ServiceOptions`: stdin IPC, `gateway-ready`, cách hiện lỗi/escrow.

Hai lệch **chưa từng được ghi ở đâu** cũng vá cùng đợt, cả hai đều ở vỏ desktop — thứ người dùng thật chạy: **không giải phóng session TTS sau 5 phút rảnh** (giữ session ONNX suốt đời tiến trình) và **không chạy bot Telegram** (đặt token xong bot im lặng). Nguyên nhân gốc ghi thẳng trong doc-comment `boot.rs`: `scripts/e2e-gateway.mjs` kiểm **gateway**, còn người dùng chạy **app desktop** — mọi lệch đều rơi đúng vào phía không ai kiểm. Đó cũng là lý do `e2e-gateway-ci.mjs` được đưa vào CI cùng đợt.

**Còn lại của U8 (nghiệm thu CHƯA đạt):** bảng "năng lực theo profile" ở [01-ban-ve/01](../01-ban-ve/01-kien-truc-tong-the.md) và [02-van-hanh/03](../02-van-hanh/03-trien-khai-va-runtime.md) **vẫn theo trạng thái cũ**; chưa quay được video barge-in trên bản Tauri.

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

**Nghiệm thu.** `handle_command` còn lại là bộ định tuyến mỏng; `cargo test` không giảm; `e2e-gateway.mjs` vẫn **8/8**; clippy vẫn **0**.

---

#### ◐ ĐANG LÀM — 26/07/2026, phiên song song (B1, bước 2→6)

Đã tách sáu miền ra `liva-native-core/src/commands/`: `voice.rs` (200) · `config.rs` (246) · `task.rs` (208) · `llm.rs` (236) · `memory.rs` (445) · `vision.rs` (196), cộng `mod.rs` (46). **`lib.rs` co từ ~2 773 xuống 1 788 dòng**, và `handle_command` chỉ còn **13 nhánh chuỗi trực tiếp**.

**Cảnh báo cho ai đang viết tài liệu:** đợt tách này **làm trôi mọi toạ độ `lib.rs:<dòng>`** trong bộ tài liệu. Ba trích dẫn đã gãy và bị `docs-citations` bắt tại chỗ — ~~`lib.rs:2392`~~, ~~`lib.rs:2234`~~, ~~`lib.rs:2529`~~ — cả ba đều trỏ ra ngoài độ dài mới của file. Đã sửa bằng cách bỏ số dòng và trỏ theo **tên symbol**.

Đây đúng là kịch bản mà quy ước "trích theo symbol, đừng trích số dòng" tồn tại để phòng, và U10 là phép thử lớn nhất của nó cho tới nay. Hai điều đáng ghi: bộ dò **bắt được cả ba**, và khi viết chính đoạn cảnh báo này thì việc *nhắc lại* ba toạ độ hỏng lại làm gate đỏ thêm một lần nữa — phải bọc `~~gạch ngang~~` theo đúng quy ước dành cho trích dẫn lịch sử. Một bộ dò không phân biệt được "trích dẫn" với "đang nói về trích dẫn"; đó là cái giá hợp lý của một luật cơ học.

---

### U11 — Lấp lỗ test WidgetApp.vue

**Vì sao.** `liva-ui/src/WidgetApp.vue` dài 1 443 dòng và chỉ đạt **29,13 % statement** — thấp nhất toàn UI. Trớ trêu là đây chính là **cửa sổ người dùng nhìn thấy suốt ngày** (Ghost Mode overlay). Hạng nhì là `MemoryViewer.vue` (1 373 dòng, 60,69 %) và `VisionView.vue` (20 %).

**Việc.** Tách logic ra composable rồi test composable — rẻ hơn nhiều so với test component, và `liva-ui/src/composables/` đã có sẵn khuôn mẫu tốt (`useFaceTracking.ts` đạt 92 %).

**Nghiệm thu.** `WidgetApp.vue` ≥ 60 % statement, **và nâng ngưỡng trong `vitest.config.ts:42` lên mức mới đạt được**. Bánh cóc chỉ đi lên.

---

## 7. Nhóm E — Năng lực mới

> Chỉ vào nhóm này sau khi A–C xong. Đây là các mục đã nằm trong "Roadmap · Near-term" của README, ghi lại ở đây để có nghiệm thu.

### U12 — Tool calling (đang làm dở)

**Trạng thái 26/07/2026 — ĐÃ COMMIT ở `45e2e58`** (`feat(llm): G1 — vòng tool-calling do LLM dẫn (mặc định TẮT), cổng 13/13 trên model thật`). `llm/tool_calling.rs` nay là **1 255 dòng** đã vào cây, cùng `bin/tool_calling_probe.rs` và các sửa đổi kèm theo ở `agent/graph.rs`, `mcp/server.rs`, `integrations/smart_home.rs`.

**Mặc định TẮT** (`LIVA_TOOL_CALLING=1`) — và từ `0b490b9` lý do đã đổi bản chất: **không còn vì thiếu bằng chứng, mà vì một số đo**. Cổng đã xanh **13/13 trên `Qwen3-VL-2B`**, tức chính model router đang cấu hình, bằng đúng điểm của `gemma-4-E4B` to gấp 4 lần. Nhưng probe đo luôn độ trễ: **trung vị 1877 ms** (dải 1128–2555) thêm vào **mỗi câu chat**. Với trợ lý thoại đó là gần hai giây chờ cho cả những câu như "hôm nay thế nào".

Đặc tả, bảng đo và các khoảng trống còn lại nằm ở [04-de-xuat-tich-hop-openspace.md](04-de-xuat-tich-hop-openspace.md) — **đó** là nguồn sự thật cho hạng mục này, không phải mục U12 ở đây.

**Việc còn lại để bật mặc định — đo ba lần, lần thứ ba thì ĐƯỢC.** Đề xuất: chỉ chạy lượt LLM khi truy hồi vượt một **ngưỡng tương đồng**, bỏ hẳn nó cho câu rõ ràng là trò chuyện. Diễn biến trên corpus 20 câu (`CORPUS_NGUONG` trong `bin/tool_calling_probe.rs`):

| | (A) mô tả tool ngắn | (B) nhồi ví dụ vào `description` | (C) tách `embed_extra` |
|---|---|---|---|
| Ngưỡng điểm top-1 | ❌ chồng 3 ca | ❌ chồng 1 ca | ✅ **trống 0,0159** |
| Độ trễ trung vị | 1877 ms | **3939 ms** | **2501 ms** |
| Prompt | ~193 token | ~417 | ~277 |
| Cổng G1 (Qwen3-VL-2B) | 13/13 | 13/13 | 13/13 |

**(B) là một hồi quy tự gây rồi tự đo ra** — nhồi ví dụ cách nói vào `description` sửa được truy hồi nhưng làm **đắt gấp đôi** đúng chỗ đang là nút cổ chai, vì hai mục đích khác nhau bị nhồi chung một trường: ví dụ cách nói giúp *embedding* rất nhiều, giúp *LLM* gần như không. **(C)** tách chúng ra — `CatalogTool::embed_extra` chỉ vào chuỗi embed, không bao giờ vào prompt.

⇒ Ngưỡng tiền lọc **khả thi**, đường bật G1 mặc định đã mở. *(Hai lần đính chính liên tiếp ở mục này trong cùng một ngày: bản đầu ghi "dấu hiệu khả thi, chưa đo"; bản sau ghi "đã đo, hướng bị đóng"; nay là "đo lần ba, được". Giữ nguyên vệt đó thay vì viết lại cho gọn — nó cho thấy kết luận âm tính ở (A) là **đúng với (A)**, và cái đổi là thiết kế chứ không phải phép đo.)*

📌 Nguồn đầy đủ (bảng ba biến thể, số từng câu): [04-de-xuat-tich-hop-openspace.md](04-de-xuat-tich-hop-openspace.md)

**Còn chưa đo:** tool đến từ server MCP ngoài (`LIVA_TOOL_CALLING_SERVERS`); và ngưỡng mới chưa chạy thật trong đường chat.

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

**◐ Làm 26/07/2026 — DỤNG CỤ xong, VIDEO thì chưa (và một phần chưa quay được).**

`liva-ui/src/components/ResourceMeter.vue` (mới): dải `Máy · LIVA · GPU` gắn ở góc widget — chỗ duy nhất còn nhìn thấy khi người dùng đang chạy game/render toàn màn hình. `pointer-events: none` để không phá Ghost Mode.

Phía lõi phải bổ sung một số: `cpuUsage` vốn đã là **tải ngoài LIVA**, nhưng thiếu **phần LIVA tự chiếm** — mà một mình `cpuUsage` chỉ chứng minh máy đang bận, không chứng minh LIVA rẻ. Nay `governor::cpu_sample()` trả cả hai **từ một lần lấy mẫu**, và `get_system_status` thêm `osStats.livaCpuUsage`.

⚠️ **Vì sao phải là MỘT hàm trả hai số:** cả hai đọc chung `LAST_CPU_SAMPLE` và *thay thế* nó. Gọi hai hàm liên tiếp thì hàm sau chỉ còn khoảng thời gian ~0 để chia — ra số vô nghĩa, và tệ hơn là số **trông vẫn hợp lý** nên không ai phát hiện.

**Đã kiểm — hai tầng riêng biệt:**

| Kiểm | Kết quả |
|---|---|
| Dải render thật trên `widget.html` | `MÁY 14% · LIVA -- · GPU 7%` |
| `osStats.livaCpuUsage` qua socket thật (binary mới) | có mặt · `cpuUsage 20` + `livaCpuUsage 0`, tổng ≤ 100 |
| `cargo test` · clippy · vue-tsc · eslint | 474 pass · 0 · 0 · 0 |

Chỗ `LIVA --` ở lần kiểm đầu là **bằng chứng quy ước trung thực chạy đúng**: gateway lúc đó là binary cũ chưa có trường này, và dải hiện `--` thay vì bịa số 0.

**Một lỗi tự bắt được, cùng họ với lỗi ở U18.** Bản đầu gate việc gửi lệnh theo `gateway.isConnected` — cờ đó `false` ở **cả hai** profile (vỏ Tauri: `connect()` cố ý return sớm vì lệnh đi qua `invoke`; widget trình duyệt: `WidgetApp` không gọi `gateway.init()` vì nó tự mở WS thoại riêng). Hệ quả: dải **không bao giờ hiện**, không lỗi, không cảnh báo, chỉ đơn giản là trống.

**⚠️ Phần video: CHƯA quay, và kịch bản trong mục này phải sửa.** [U1](#u1--build-release-và-kiểm-visionask-thật) đo được `vision:ask` tốn **~80 giây/lượt** trên CPU thuần và tự kết luận "đừng đưa vào demo trực tiếp khi chưa có GPU". Bước *"hỏi bằng giọng → LIVA nhìn quanh con trỏ → trả lời"* vì vậy **không thể nằm trong một video liền mạch ≤90 s**. Hai lối đi:

1. **Quay bản không có vision** — vẫn đủ chứng minh luận điểm chính (không alt-tab · đồng hồ đứng yên · cướp lời được). Làm được ngay.

#### Kịch bản quay — bản không vision (đã kiểm điều kiện 27/07/2026)

**Điều kiện, đã xác nhận chạy thật trên vỏ Tauri:** Piper nạp cả hai giọng qua `../..\models/piper` (bản vá `90c38bf`); `get_system_status` trả đủ `cpuUsage` · `livaCpuUsage` · `gpuUsage`; gateway lên; VAD nạp; 0 restart, 0 panic.

| Giây | Việc | Phải thấy trong khung hình |
|---|---|---|
| 0–10 | Mở sẵn ứng dụng nặng **toàn màn hình** (game / render / build). Chưa đụng LIVA | Đồng hồ tài nguyên ở góc: `Máy` cao, `LIVA` gần 0 |
| 10–20 | **Không alt-tab.** Nói câu đánh thức rồi hỏi một câu ngắn | Ứng dụng nặng vẫn chiếm toàn màn hình, không thu nhỏ |
| 20–40 | LIVA trả lời bằng giọng | Ô `LIVA` nhích lên rồi về — đây là "cái giá", và nó phải **thật** |
| 40–55 | **Cướp lời**: nói chen vào giữa lúc LIVA đang nói | LIVA im ngay giữa câu |
| 55–70 | Hỏi câu thứ hai, để trả lời trọn | Đồng hồ `Máy` vẫn cao suốt — máy chưa hề rảnh đi |
| 70–90 | Giữ khung hình vài giây ở đồng hồ | Ba số cùng đọc được một lượt |

**Ba điều làm hỏng cảnh, tránh từ đầu:**

- **Đừng hỏi câu cần nhìn màn hình.** `vision:ask` tốn ~80 s trên CPU ([U1](#u1--build-release-và-kiểm-visionask-thật)) — video sẽ thành 80 giây không có gì xảy ra.
- **Đừng ghép ảnh Task Manager.** Số phải là dải trong widget, đọc từ `get_system_status`. Ghép vào là phá đúng thứ khiến cảnh này đáng tin.
- **Đừng quay khi ô `LIVA` đứng yên ở 0 % suốt lúc đang nói.** TTS có chi phí; một số 0 phẳng lì trong lúc máy đang phát tiếng là dấu hiệu đo hỏng, và người xem tinh ý sẽ thấy. Thà hiện 3–7 % thật còn hơn một số 0 đẹp.

**Nghiệm thu bản quay:** một lần quay liền mạch ≤ 90 giây, không cắt, trong đó **ứng dụng nặng và ba số của đồng hồ nằm cùng khung hình** ít nhất một lần, và có ít nhất một lần cướp lời thành công.
2. **Bật GPU trước** (`--features cuda` + `LIVA_LLM_N_GPU_LAYERS`) rồi đo lại vision. Chỉ khi đó kịch bản đầy đủ mới quay nổi.

---

### U17 — Onboarding 10 giây — LIVA nói bằng giọng người dùng

> **⛔ ĐÍNH CHÍNH 26/07/2026 — bản đầu của mục này dựa trên một giả định SAI.** Nó viết "engine đã xong, phần thiếu chỉ là một luồng onboard" và ước lượng 1–2 ngày. Kiểm lại code cho thấy **clone giọng từ file ghi âm chưa tồn tại**, và bị chặn bởi hai model thiếu chứ không phải bởi công việc UI. Docstring của chính `tts/vieneu/mod.rs` đã nói trước điều đó: *"live cloning from a wav are a follow-up"*. Mục này được tách làm hai phần dưới đây.

**Hợp đồng một "giọng" trong VieNeu** (đọc từ `voices_v3_turbo.json` + `VieNeuVoice::load`): `speaker_emb` — **192 số float**, đi qua `speaker_anchor()` thành anchor; cộng `codes` — **T×16 mã RVQ** làm tham chiếu in-context. Muốn clone thì phải sinh được **cả hai** từ wav của người dùng.

| Cần để clone | Trạng thái 26/07/2026 |
|---|---|
| MOSS audio tokenizer **encode** (wav → mã RVQ) | **THIẾU** — cả trên đĩa lẫn trong `scripts/models.mjs` chỉ có `moss_audio_tokenizer_decode_full.onnx` |
| Speaker encoder 192-d (wav → `speaker_emb`) | **THIẾU** — không có model nào loại này trong repo |

⚠️ Speaker encoder **phải đúng loại đã dùng lúc train VieNeu**, vì `xvec_w` trong `vieneu_v3_heads.npz` là ma trận học cùng nó. Lấy một ECAPA/CAM++ bất kỳ rồi nhét 192 số vào sẽ cho anchor vô nghĩa — giọng sẽ sai chứ không phải "hơi khác". Đây là rủi ro chính của U17b, và phải kiểm bằng tai trước khi tin.

---

#### ~~U17a — Bộ chọn giọng~~ ✅ XONG 26/07/2026

**Vì sao.** `voices_v3_turbo.json` chứa **10 giọng Việt có tên** (nam/nữ · Bắc/Trung/Nam · kèm phong cách), và `VieNeuVoice::load(dir, voice)` **đã** chọn được theo tên. Nhưng chỉ `vieneu_probe` dùng tham số đó qua `LIVA_VIENEU_VOICE`; đường sản phẩm luôn nạp giọng mặc định `Phạm Tuyên`. Tức là **10 giọng đã tải về, đã chạy được, và vô hình với người dùng** — đúng loại "vàng đang tắt điện" mà dự án hay mắc.

**Việc.** Đưa danh sách preset ra IPC, thêm màn chọn giọng trong dashboard, lưu lựa chọn vào `data/liva-config.json`. Bỏ cổng env cho riêng luồng chọn giọng.

**Nghiệm thu.** Người dùng đổi giọng bằng chuột, nghe thử ngay, và lựa chọn sống sót qua một lần khởi động lại. **Không được gọi đây là "giọng của bạn"** — nó là chọn giọng có sẵn.

**✅ Đã làm 26/07/2026 — output thật đã đo.**

Hai lệnh IPC mới: `voice:list_vieneu_voices` (chỉ đọc JSON, **không nạp ONNX**, nên trả lời được cả khi VieNeu đang tắt) và `voice:set_vieneu_voice`. Lõi thêm `VieNeuVoice::set_voice()` + `list_voices()`; cấu hình đọc theo thứ tự **env → `data/liva-config.json` → mặc định**. Giao diện: khối chọn giọng trong `VoiceManagementView.vue`, nối cả đường Tauri lẫn đường WebSocket.

Chạy `node scripts/e2e-vieneu-voice.mjs` với gateway thật ở `:8099` — **14/14 đạt**:

| Điều kiện | Kết quả đo |
|---|---|
| Liệt kê giọng | 10 giọng, đúng một giọng mang cờ mặc định |
| Tên giọng sai | bị từ chối **trước khi** ghi cấu hình (config không đổi một byte) |
| Bật + chọn giọng | `đã bật và nạp VieNeu`, `current` khớp giọng đã chọn |
| Lựa chọn xuống cấu hình | `{"vieneuEnabled":true,"vieneuVoice":"Mai Anh"}` |
| **Đổi sang giọng khác** | **6 ms** — xác nhận đổi anchor tại chỗ, KHÔNG nạp lại ~500 MB |
| Tắt | `current: null`, `enabled: false` |

Cổng khác sau thay đổi: `cargo test` **386 pass · 0 fail · 1 ignored** (đường cơ sở cũ 348) · clippy **0 warning** · `vue-tsc` **0 lỗi** · ESLint **0 warning** · `cargo check -p liva-desktop` **xanh**.

**Đã kiểm bằng mắt trên Dashboard thật** (`liva-ui` cổng 5175 + gateway :8002, chế độ trình duyệt — tức đi đường **WebSocket**, không phải Tauri IPC): 10 thẻ giọng hiện đủ tên · giới tính · vùng · phong cách; bấm "Trúc Ly" → lõi log `VieNeu-TTS loaded (voice='Trúc Ly', style_id=16, 62 ref frames)`, thẻ hiện huy hiệu **✓ Đang dùng**, dòng trạng thái báo *"đã bật và nạp VieNeu"*, và `data/liva-config.json` nhận `{"vieneuEnabled":true,"vieneuVoice":"Trúc Ly"}`.

Việc nó chạy ở chế độ trình duyệt là bằng chứng **cả hai đường truyền đều sống** — đây chính là điều U8 đòi hỏi, nên U17a không thêm một tính năng chỉ-sống-ở-một-profile nào.

#### U17b — Clone giọng thật (bị chặn, chưa ước lượng được)

**Việc đầu tiên không phải viết code, mà là trả lời hai câu:** upstream `OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano-ONNX` có công bố nhánh **encode** không, và VieNeu dùng speaker encoder nào lúc train? Chưa trả lời được hai câu đó thì mọi ước lượng thời gian đều là bịa.

**Nghiệm thu.** Một người **chưa từng thấy LIVA**, từ lúc mở app tới lúc nghe **giọng mình** phát ra: **dưới 2 phút**, không sửa cấu hình, không đặt env.

**⚠️ Rủi ro còn nguyên.** Chất lượng giọng VieNeu **chưa được duyệt bằng tai** trên máy này. Giọng clone méo hoặc robot thì mục này gây ấn tượng **ngược**, và ấn tượng ngược về giọng nói không gỡ lại được.

**File (cả hai phần).** `liva-native-core/src/tts/vieneu/`, `liva-native-core/src/tts/mod.rs`, `liva-native-core/src/lib.rs` (lệnh IPC), một màn hình trong `liva-ui/src/components/dashboard/`.

---

### U18 — Trí nhớ nhìn thấy được, ngay trên UI

**Vì sao.** `e2e-memory.mjs` đã chứng minh 6/6 rằng LIVA nhớ **xuyên qua một lần khởi động lại tiến trình** — nhưng bằng chứng đó nằm trong terminal, nơi không người xem nào nhìn. Đồng thời Memory Dashboard là màn hình **đã có sẵn**. Đây là mục rẻ nhất nhóm F: không viết năng lực mới, chỉ đưa bằng chứng sẵn có lên chỗ nhìn thấy được. Ranh giới giữa "chatbot" và "trợ lý" trong đầu người xem nằm đúng ở khoảnh khắc này.

**Việc.**
1. Một toast ngắn "LIVA vừa nhớ: …" khi một lượt được persist thành công.
2. Nút khởi động lại lõi ngay trên dashboard, để diễn được trước mặt người xem mà không cần mở terminal.

**File.** `liva-ui/src/components/dashboard/`, `liva-native-core/src/lib.rs` (sự kiện persist).

**Nghiệm thu.** Toàn bộ thao tác "nói một sự thật → khởi động lại → hỏi lại → trả lời đúng" làm được **bằng chuột**, không chạm terminal.

**⚠️ Ràng buộc.** Chỉ hiện các tầng **có dữ liệu thật** (L2). Tầng `l0_5`/L3 hiện chưa có writer ([U13](#u13--consolidation-ngữ-nghĩa-l2--l3)); vẽ ô rỗng cho chúng là đi ngược đúng nguyên tắc mà [U3](#u3--lệnh-preflight-báo-trạng-thái-tài-nguyên--một-phần-cli-xong-26072026-ui-còn-nợ) và `sysinfo.rs` vừa dựng lên.

**✅ XONG 26/07/2026 — ba phần dựng xong, chuỗi đầu-cuối đã diễn được.**

Toàn bộ nằm trong `MemoryViewer.vue`, **không đụng Rust** (`AppState` chưa có kênh phát sự kiện; dựng một cái sẽ phải sửa cả hai điểm vào lẫn tầng WebSocket, mà câu hỏi ở đây chỉ là *"sổ sự kiện có dài thêm không"* — đọc thẳng `get_memory_data` trả lời được).

| Phần | Trạng thái |
|---|---|
| L0.5 thôi nói dối: `--` + nhãn **CHƯA CÓ** + ghi chú giải thích, thay cho `# SESSION STATE (Empty)` | ✅ đã xem tận mắt |
| Nút **Khởi động lại LIVA** hai bước (bấm 1 → cảnh báo, bấm 2 → thực thi) | ✅ đã xem tận mắt |
| Trong trình duyệt, nút báo thẳng *"chỉ khởi động lại được trong ứng dụng desktop"* thay vì im lặng | ✅ đã xem tận mắt |
| Băng "LIVA vừa nhớ thêm N điều" | ⚠️ **chưa xem với dữ liệu thật** |
| Chuỗi *nói → khởi động lại → hỏi lại* bằng chuột | ⚠️ **chưa diễn được** |

**✅ Hai dòng cuối đã nghiệm thu 26/07/2026 — do NGƯỜI DÙNG chạy, không phải agent đo.** Chuỗi *nói một sự thật → băng "vừa nhớ" hiện → bấm Khởi động lại → hỏi lại → trả lời đúng* chạy trên vỏ Tauri thật. Ghi rõ nguồn vì agent không nhìn được cửa sổ native: đây là xác nhận của người dùng, không phải một phép đo có log kèm — muốn có bằng chứng máy đọc được thì phải bổ sung một e2e riêng.

**⚠️ Nhưng để chạy được `npm run dev` thì phải vá HAI bug có sẵn trên `main` trước.** Cả hai đều làm profile Tauri không dùng được, và bug thứ hai bị bug thứ nhất che:

| Bug | Triệu chứng | Vá |
|---|---|---|
| Watcher của `tauri dev` canh cả `src-tauri/data/`, đúng nơi lõi ghi SQLite WAL | App tự kích hoạt rebuild của chính nó — **108 lần khởi động lại** trong vài phút, chưa một lần tới `listening on ws` | `liva-desktop/src-tauri/.taurignore` loại trừ `data/`, `logs/` |
| `boot::spawn_background_services` gọi `tokio::spawn` trong closure `.setup()` của Tauri — **ngoài** runtime | Panic ngay khởi động: *"there is no reactor running"*. `main.rs` không dính vì nằm trong `#[tokio::main]` | Vào ngữ cảnh runtime Tauri trước khi gọi |

**Bài học chung của cả hai:** đây đúng là loại khác biệt giữa hai vỏ mà `boot.rs` sinh ra để xoá — gom danh sách dịch vụ về một chỗ vẫn chưa đủ, vì **ngữ cảnh chạy** của hai vỏ khác nhau. Thêm dịch vụ nền mới thì phải thử **cả** `cargo run` lẫn `npm run dev`, không suy ra từ nhau.

**Bug thứ ba cùng gốc — ✅ ĐÃ VÁ 27/07/2026 (`90c38bf`):** cwd của `tauri dev` là `src-tauri/` nên mọi đường dẫn tương đối trượt; log từng báo `Piper voice dir "models/piper" not found`, tức **profile Tauri không có giọng Piper nào** và TTS rơi xuống Kokoro vốn cũng thiếu file — LIVA im tiếng. Nay Piper dò lên hai cấp giống `vieneu_model_dir()`, và LIVA nói lại được qua vỏ Tauri.

Ba bug này chung một gốc và đáng rút thành luật: **đường dẫn tương đối là một bất biến giữa hai vỏ, không phải chi tiết cục bộ của một module.** Mỗi lần thêm một tài nguyên đọc từ đĩa, phải thử **cả** `cargo run` lẫn `npm run dev` — chỉ một trong hai sẽ lộ lỗi.

**⚠️ ĐÍNH CHÍNH 27/07/2026 — luật trên từng được viết kèm một lời khuyên SAI.** Bản đầu nói "hoặc dùng bộ giải dò-lên-hai-cấp có sẵn". Cách đó đúng cho **tài nguyên chỉ-đọc** (model: mọi bản giống hệt nhau nên tìm thấy bản nào cũng như nhau) nhưng **sai cho trạng thái ghi được**: dò sẽ tìm ra bản *gần nhất*, tức mỗi cwd vẫn cho một database khác nhau.

Hậu quả đo được ngày 27/07 — **ba database `liva_core` cùng tồn tại trên một máy**, kích thước khác nhau (32 KB · 32 KB · 118 KB). Triệu chứng: thêm một liên hệ vào sổ danh bạ, khởi động LIVA bằng cách khác, danh bạ **trống** — LIVA chỉ nói "chưa có ai tên đó". Không lỗi, không log. Đã cắn ba lần trong một buổi.

Luật đúng, tách theo bản chất tài nguyên:

| Loại | Cách giải đường dẫn | Vì sao |
|---|---|---|
| **Chỉ-đọc** (model, voice, wasm) | Dò lên hai cấp — `vieneu_model_dir`, Piper | Mọi bản giống hệt nhau; tìm thấy bản nào cũng đúng |
| **Ghi được** (DB, vault, cấu hình người dùng) | **Một neo cố định** — `crate::data_dir()`: thư mục chứa `data/liva-config.json`, hoặc `%LOCALAPPDATA%\LIVA\data` khi không có cây mã nguồn | Mỗi bản là một **trạng thái riêng**; dò sẽ chia trạng thái thành nhiều bản song song |

Đã sửa ở `boot.rs`: DB mặc định neo vào `data_dir()`, và khi phát hiện database ở chỗ khác thì **báo bằng WARN kèm đường dẫn + kích thước, KHÔNG tự di trú** — gộp hai file SQLite là thao tác mất mát tiềm tàng, người dùng phải là người chọn giữ bản nào. `LIVA_DB_PATH` vẫn thắng tất cả.

**Một lỗi tự bắt được khi rà lại, đáng ghi vì nó chính là thứ U18 sinh ra để chống.** Bản đầu chốt mốc đếm ngay lúc `onActivated`, khi dữ liệu chưa về nên mốc luôn bằng 0 — nghĩa là **lần mở đầu tiên sẽ khoe "LIVA vừa nhớ thêm N điều" cho toàn bộ sổ ký ức cũ**. Nay mốc để `null` khi chưa có dữ liệu và nhận giá trị đầu tiên về làm mốc.

**⚠️ Bẫy kiểm thử, ghi lại để phiên sau không chẩn đoán nhầm.** Khi Browser pane bị ẩn, trang **không dựng khung hình**, nên `<Transition mode="out-in">` trong `DashboardApp.vue` kẹt ở pha leave và **view mới không bao giờ mount**. Triệu chứng đọc y hệt một component hỏng: nút điều hướng đã `active`, không một cảnh báo Vue nào, mà `<main>` vẫn giữ view cũ. Cách phân biệt: tiêm `*{transition:none!important}` rồi bấm lại — nếu view hiện ra thì đó là pane, không phải code.

---

### U19 — Ba tool OS thật

**Vì sao.** Cơ chế tool-calling đã xong ([U12](#u12--tool-calling-đang-làm-dở)) nhưng **catalog gần như rỗng**: chỉ có bốn tool nội bộ, trong đó hai là thao tác vault và một là smart-home chưa có phần cứng. Cơ chế không có nội dung thì không ai thấy nó tồn tại. "Đang bận tay, nói *nhỏ nhạc lại* → nó làm ngay" gây ấn tượng mạnh hơn mọi câu chat, vì nó chứng minh trợ lý **chạm được vào máy**, không chỉ nói.

**Việc.** Ba tool Win32, không thêm dependency nặng: âm lượng hệ thống · độ sáng màn hình · play/pause media. Cả ba đều **hoàn tác được**, nên đủ điều kiện vào `NATIVE_AUTOEXEC`.

**File.** `liva-native-core/src/llm/tool_calling.rs`, `liva-native-core/src/mcp/client.rs`, một module mới dưới `liva-native-core/src/integrations/`.

**Nghiệm thu.** `tool_calling_probe` chọn **đúng** tool cho 10 câu tiếng Việt tự nhiên với model 2B thật, và mọi tool đều hoàn tác được. Đây **chính là** corpus mà U12 đang chờ để bật `LIVA_TOOL_CALLING` mặc định — làm U19 là làm luôn cổng nghiệm thu của U12.

**⚠️ Đừng thêm tool có hậu quả không đảo ngược** (xoá file, gửi tin, mua bán) vào `NATIVE_AUTOEXEC`. Ranh giới chọn/chạy trong `ExecPolicy` tồn tại đúng để chặn chuyện đó — giữ nó.

**✅ XONG 26/07/2026 — hai trên ba tool, nghiệm thu 10/10.**

`integrations/os_control.rs` (mới): `control_volume` (to/nhỏ/tắt-bật tiếng) và `control_media` (phát-dừng/bài kế/bài trước) qua `SendInput` với phím đa phương tiện — **không thêm một dependency nào**, vì `windows-sys` đã bật sẵn `Win32_UI_Input_KeyboardAndMouse`. Cả hai vào `NATIVE_AUTOEXEC` theo đúng ranh giới đã ghi: **đảo ngược được thì cho tự chạy**. Cổng nghiệm thu riêng: `src/bin/os_control_probe.rs`.

**Độ sáng màn hình: CỐ TÌNH chưa làm.** Không có phím ảo chuẩn; `SetMonitorBrightness` (Dxva2) cần DDC/CI nên trượt trên phần lớn màn laptop, còn WMI kéo theo cả tầng COM. Một tool "chỉnh độ sáng" trượt im lặng trên máy beta tester **tệ hơn không có tool** — đúng thứ vừa gỡ khỏi `smart_home`.

| Phép đo (Qwen3-VL-2B thật) | Kết quả |
|---|---|
| Tầng 1 — tool mong đợi lọt vào prompt | **12/12**, và luôn ở top-1 |
| Tầng 2 — **toàn tuyến** (keyword → LLM) | **14/14** · riêng 10 câu OS: **10/10** |
| Trong đó do đường nhanh xử | **9/14** — 0 token, 0 độ trễ, tất định |
| Trong đó đi tới LLM | 5/14, **5/5 đạt** |
| Hồi quy: cổng G1 smart-home | **13/13 — không hỏng** |
| Độ trễ thêm, chỉ cho câu đi tới LLM | ~3 200 ms |
| Unit test mới | 12 · `cargo test` 405 pass · clippy 0 |

**Cách đạt 10/10 — và vì sao nó KHÔNG phải là "model khá lên".** Vòng đo đầu chỉ dùng đường LLM và được 9/10: *"bật nhạc lên"* (mở nhạc hay vặn to nhạc?) rơi sang `control_volume`, *"chuyển bài khác"* đúng tool nhưng sai hướng. Viết lại prompt không sửa được — đó là câu đa nghĩa thật, và trần của model 2B.

Cách sửa là **cho `route_intent` biết từ vựng âm lượng/nhạc**, đúng như thiết kế module tự khai: *"đường nhanh đứng trước, LLM chỉ chạy khi nó nói không biết"*. Câu đa nghĩa không còn tới tay model. Hệ quả đo được: 9/14 câu nay tốn **0 token** — tức đây cũng là bước đầu tiên gỡ rào cho [U12](#u12--tool-calling-đang-làm-dở), vì chi phí ~3 s/lượt là lý do `LIVA_TOOL_CALLING` còn tắt.

⚠️ **Đừng đọc "LLM 5/5" thành "model đã tốt".** Nó 5/5 vì không còn bị hỏi câu khó, không phải vì khá lên. Số cũ 9/10 vẫn là sự thật về model 2B và probe vẫn in nó ra mỗi lần chạy.

**Ba ràng buộc của bảng từ khoá, đều có test canh:**
- **Chỉ tiếng Việt.** Thêm danh từ tiếng Anh là rước lại bẫy `"let's get back on track"` (`track` + `back` = quay lại bài trước). Tiếng Anh để LLM lo — nó xử lý tốt.
- **Đòi danh từ âm thanh/nhạc**, nên không thể cướp `"bật đèn"` / `"tắt quạt"`; và `"làm bài tập xong chưa"` (chữ "bài" rất thông dụng) vẫn rơi về Chat.
- **ĐỘ TO thắng ĐANG-PHÁT-GÌ**: `"nhỏ nhạc lại"` có cả hai loại từ → âm lượng. Nhưng `"tắt nhạc"` là dừng phát, không phải mute — nhánh mute đòi đúng danh từ âm thanh.

**Ba bài học đã đo, đáng giữ hơn con số.**
1. Model điền chuỗi giữ chỗ vào trường tuỳ chọn: `{"action":"up","steps":"any"}`. Với `Option<u8>` thẳng thì **cả lệnh hỏng** dù ý định rõ ràng. Nay `steps` khoan dung (rác → mặc định), còn `action` vẫn nghiêm — sai ở đó là sai ý định.
2. Model nói từ **tự nhiên**, không nói theo tên trường: `pause`, `louder`, `skip`. Đã nhận làm alias; schema vẫn chỉ quảng cáo tên chuẩn nên prompt không phồng thêm token nào.
3. Mô tả tool phải nêu **ranh giới**, không chỉ chức năng. *"nhỏ nhạc lại"* bị `control_media` hút mất cho tới khi mô tả nói rõ ĐỘ TO (volume) ≠ ĐANG PHÁT GÌ (media).

**⚠️ Thay đổi hành vi mà U19 mang lại:** catalog từ 4 → **6 tool**, trong khi `DEFAULT_TOP_K` vẫn là 4. Từ nay **truy hồi loại bớt tool khỏi prompt mỗi lượt** — trước đây 4 tool ≤ 4 chỗ nên thứ hạng không ảnh hưởng gì. Thêm tool thứ 7 trở đi phải đo lại tầng 1, không được cho là hiển nhiên.

---

### U20 — Bộ nhớ thị giác offline *(tuỳ chọn, đắt, có mìn)*

**Vì sao.** Trần ấn tượng cao nhất trong toàn bộ tài liệu này: hỏi "hôm qua mình build lỗi gì ấy nhỉ" và LIVA nhớ, **vì nó đã thấy**. Đây đúng là ý tưởng đã bị công chúng ném đá khi một hãng lớn làm — và lý do bị ném đá là **dữ liệu rời khỏi máy**. LIVA offline là câu trả lời trực tiếp cho lời chê đó.

**⚠️ Đây là mục nguy hiểm nhất tài liệu này.** Nó dẫm vào `passive/hook.rs` — một keylogger toàn hệ thống hiện đã bị đưa ra ngoài build mặc định, và theo ghi chú của chính dự án thì hook bàn phím cấp OS sẽ khiến anti-cheat **ban phần cứng** máy người chơi game. **Không được** bật lại module đó để làm mục này.

**Việc (nếu làm).** Thu thập qua OS Accessibility / UIAutomation — tên cửa sổ, tiến trình, cấu trúc text UI — **không hook bàn phím**. Cộng với một cổng đồng ý tường minh và một chỉ báo "đang ghi" luôn hiển thị.

**Nghiệm thu — theo thứ tự bắt buộc.** Cổng đồng ý và công tắc tắt phải **tồn tại và hoạt động trước khi viết dòng code thu thập đầu tiên**. Ngược thứ tự là tự tạo ra thứ không thể phát hành.

**◐ BƯỚC 1 XONG 26/07/2026 — cổng đồng ý. KHÔNG có một dòng thu thập nào.**

> ⚠️ **Code đã viết và đã kiểm, nhưng CHƯA nằm trong repo tại commit này.** Nó bị giữ lại vì `lib.rs` và `commands/mod.rs` — hai file bắt buộc phải sửa để nối cổng — đang khai `pub mod messaging;` cho một module **chưa được commit** của một phiên làm việc song song, và module đó có một test hỏng do dùng chung trạng thái (`messaging::outbox`, đạt khi chạy một mình, hỏng khi chạy cùng các test anh em). Stage chúng sẽ cho một commit **không biên dịch được** hoặc một commit **đỏ test**. Mục này sẽ vào repo ngay khi phiên kia commit `messaging` và test xanh trở lại. Ghi ra đây để không ai đọc mục này rồi đi tìm code không thấy.

Đây là bước đầu **bắt buộc** ở trên, làm đúng thứ tự. Phần thu thập (UIAutomation) vẫn chưa bắt đầu, và mìn `passive/hook.rs` vẫn nguyên sau `--features experimental` — không đụng tới.

| Thành phần | Ghi chú |
|---|---|
| `consent.rs` (mới) | Nguồn sự thật, **fail-closed**: thiếu file / JSON hỏng / sai kiểu → CHƯA đồng ý |
| `commands/consent.rs` (mới) | Miền IPC `consent:get` · `grant` · `revoke` |
| `ObservationConsentPanel.vue` (mới) | Công tắc người dùng bấm được, trong Cài đặt |
| `data/consent.json` | **gitignore** — quyết định riêng tư per-máy, không commit |

**Cổng nằm trong build MẶC ĐỊNH**, không sau `experimental` như `passive/`: một cổng chỉ tồn tại ở build thử nghiệm thì không chặn được gì trong bản giao cho người dùng.

**Đã kiểm — hai tầng:**

- **Lõi, qua WebSocket thật: 9/9.** Mặc định tắt · bật được · đọc lại vẫn bật (bền vững qua file) · tắt có hiệu lực ngay lần đọc kế tiếp · lệnh sai trả lỗi thay vì im lặng.
- **Giao diện, vitest: 7/7.** Hỏi trạng thái thật khi mở · mặc định hiện ĐANG TẮT · nút gửi đúng `grant`/`revoke` · và ranh giới quan trọng nhất: **bật cổng KHÔNG hiện "đang ghi"**.

Cổng khác: `cargo test` 510 pass · clippy 0 · vitest 265 pass · vue-tsc 0 · eslint 0.

**Ranh giới "đã cho phép ≠ đang ghi" được ghim ngay từ hợp đồng IPC.** `consent:*` trả cả `granted` lẫn `active`, và `is_capture_active()` **luôn `false`** vì chưa có collector. Làm vậy để chỉ báo "đang ghi" đã có chỗ nối sẵn — khi collector ra đời không ai phải nhớ thêm nó vào, và người dùng bật cổng hôm nay không hiểu nhầm rằng máy mình đang bị ghi.

**⚠️ Hợp đồng cho collector tương lai, đọc trước khi viết dòng đầu tiên:**
1. Hỏi `consent::ObservationConsent::is_capture_allowed()` **trước mỗi lần ghi**, không cache qua ranh giới thu hồi. Nếu cần cache ở hot path thì **vô hiệu hoá cache khi thu hồi** là bắt buộc.
2. Thu thập qua **UIAutomation**, tuyệt đối không bật lại `passive/hook.rs`.
3. `is_capture_active()` phải trả `true` **thật** khi đang ghi — để nó trả `false` trong lúc vẫn ghi là nói dối người dùng ở đúng chỗ nhạy cảm nhất.

**Chưa làm:** xoá dữ liệu khi thu hồi (chưa có dữ liệu để xoá, nhưng phải là một phần của công tắc khi có), và chỉ báo "đang ghi" thường trực trên widget.

**⚠️ Bẫy kiểm thử lặp lại lần thứ hai.** Không xem được panel trong Browser pane: nút điều hướng `active: true` nhưng `<Transition mode="out-in">` kẹt vì pane không dựng khung hình — y hệt U18, và lần này thủ thuật `transition:none` **không** gỡ được. Đã chuyển sang vitest, vốn tốt hơn: chạy trong CI, không phụ thuộc pane. **Với component dashboard, viết test vitest thay vì cố xem bằng mắt.**

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
