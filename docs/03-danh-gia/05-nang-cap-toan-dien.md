---
title: "Nâng cấp toàn diện — việc cần làm, theo thứ tự"
updated: 2026-08-07
commit: dec1c14
stale-ok: dec1c14
status: living
owns:
  - duong-co-so-do-luong
  - backlog-nang-cap-U1-U15
  - goi-trinh-dien-U16-U20
  - ra-soat-proj-airi-U24-U29
  - locomotion-avatar-U30-U33
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
  - liva-native-core/src/openai_api.rs
  - liva-native-core/src/preflight.rs
  - liva-native-core/src/sysinfo.rs
  - liva-native-core/src/tts/avatar_control.rs
  - liva-native-core/src/tts/normalizer.rs
  - liva-native-core/src/tts/vieneu/g2p.rs
  - liva-native-core/src/vision/mod.rs
  - liva-ui/src/WidgetApp.vue
  - liva-ui/src/components/VRMEngine.vue
  - liva-ui/src/composables/footPlantIK.ts
  - liva-ui/src/composables/mixamoRetarget.ts
  - liva-ui/src/composables/use3DModel.ts
  - liva-ui/src/composables/useAvatarAnimation.ts
  - liva-ui/src/composables/useSpeakerPlayback.ts
  - liva-ui/src/utils/avatarControlTags.ts
  - liva-ui/src/utils/avatarMath.ts
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

**Bốn luật cứng khi thi hành backlog này:**

- **Không commit tự động.** `git commit`/`push`/`pull` là hành động của người dùng (`AGENTS.md`).
- **Chạy `impact()` trước khi sửa symbol.** Bắt buộc theo `CLAUDE.md`; các mục U10/U11 chạm vào symbol có nhiều người gọi.
- **Không hạ ngưỡng để cổng xanh.** Ngưỡng coverage trong `vitest.config.ts:42` là bánh cóc — chỉ đi lên.
- **Tách commit mã nguồn khỏi commit tài liệu.** Xem bẫy ở §0.2 — đây là cách làm CI đỏ mà `docs-check` *không* cảnh báo được trước.

### 0.1 Việc tiếp theo — chọn từ trên xuống

> 🔴 **Chen ngang, chốt 07/08/2026: [U30](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước) đứng trước mọi mục trong bảng dưới.** Đây là triệu chứng **người dùng trực tiếp báo** (avatar khựng theo từng bước chân), tức hạng khác với backlog — cùng hạng với hồi quy ở bước 1 của giao thức. Và **bước 0 của nó là một dòng**: tắt `FootPlantIK`, đi lại, nhìn. Làm cái đó trước khi nhận bất kỳ mục nào khác; kết quả quyết định U30 hay [U31](#u31--ba-khoản-phí-mỗi-frame-trên-đường-avatar) mới là chỗ đáng đào.

Chốt ngày **29/07/2026**. Thứ tự đã áp quy tắc chặn ở §2: xong nhóm A trước, **không đụng nhóm D** (U10/U11) khi A/B/C còn dở.

| Thứ tự | Việc | Vì sao ở vị trí này | Bắt đầu từ đâu |
|---|---|---|---|
| ~~**0**~~ | ~~Đo lại đủ 9 dòng đường cơ sở §1 tại HEAD~~ | ✅ **XONG 29/07/2026** — đo lại **11 cổng** tại `c6ec120` trong một phiên. Không có hồi quy nào ở mã nguồn (test 405 → **554**, coverage nhích lên cả bốn chỉ số); **một hồi quy ở tài liệu**: `docs-check` đỏ 6 lỗi, đã vá cùng phiên | Bảng mới ở [§1](#1-đường-cơ-sở-đã-đo--02082026-tại-260c643-thay-bảng-2907) |
| **1** | **U2 — installer + thử trên máy sạch** | Mục cao nhất chưa gạch của nhóm A, và A chặn beta | [§U2](#u2--installer-hiện-hành-và-thử-trên-máy-sạch). Nhớ: **U1c cho kết quả ÂM TÍNH**, cuBLAS là phụ thuộc cứng ⇒ tính gói **~830 MB**, đừng lên kế hoạch cho một con số nhỏ hơn |
| **2** | **U16 — quay video** | Mã và dụng cụ đo xong rồi; **chỉ bạn quay được**, không ai làm hộ | Kịch bản quay đã viết sẵn trong [§U16](#u16--gói-demo-không-alt-tab-có-hiện-chi-phí). Cần **build release + CUDA** (vision 1,2 s); trên debug `vision:ask` hỏng có chủ đích |
| **3** | **U3 (màn hình UI) + U8 (bảng năng lực theo profile)** | Hai mục ◐ còn sót đuôi nhỏ, gỡ nốt cho nhóm A/C sạch | `preflight` CLI đã xong; còn màn hình. U8: `boot.rs` đã xong, còn bảng ở `01-ban-ve/01` + `02-van-hanh/03` |
| ~~**4**~~ | ~~U7 — dọn `unwrap()` trên đường thoại~~ | ✅ **XONG 29/07/2026 — nhưng tiền đề của mục SAI.** Không `unwrap()` nào trong `tts/` là điểm panic do đầu vào. Đã vá lớp thật sự đáng vá (nhiễm độc khoá), thêm 6 test đầu vào rác, và sửa **hai lỗi thật** trong `voice_stress.exe` vốn đỏ sẵn ở HEAD | [§U7](#u7--dọn-unwrap-trên-đường-thoại) |
| ~~**5**~~ | ~~U9 — một con số TTFT đo được~~ | ✅ **XONG 29/07/2026** — `ttft_bench.exe`, p50 **667 ms CPU · 18 ms CUDA** | [§U9](#u9--một-con-số-ttft-đo-được) |

**Đang bị chặn, đừng nhận:** **U17b** (thiếu 2 model: MOSS *encode* và bộ mã hoá giọng 192 chiều) · **U20 bước 2+** (có mìn — nếu làm thì **bắt buộc** dùng UIAutomation, **tuyệt đối không** `passive/hook.rs`).

**Ứng viên ngoại lệ, chưa duyệt:** [**U21**](#u21--sổ-đo-mỗi-lượt--turn_telemetry) (sổ đo mỗi lượt) thuộc nhóm E nên theo quy tắc chặn thì chưa tới lượt — nhưng nó là *hạ tầng đo* cho U14 chứ không phải năng lực mới, và mỗi ngày hoãn là một ngày mất dữ liệu. Điều kiện để chen ngang: nghiệm thu 3 (độ trễ thêm < 5 ms ở p50) phải đạt. Không đạt thì xếp lại hàng.

**Việc vặt còn treo:** `.gitnexusrc` đang ở trạng thái `M` (thêm `walCheckpointThreshold`) — chưa commit.

⚠️ **Bẫy đo khi cây làm việc đang bị sửa song song — trả giá 06/08/2026.** Trong phiên [U24](#u24--hai-lỗi-nối-dây-trên-đường-lip-sync-của-widget)/[U26](#u26--control-tag-đọc-được-giữa-câu-không-chỉ-ở-đầu-lượt), `npm run test:coverage -w liva-ui` đỏ ở `useAvatarAnimation.test.ts` (foot-plant IK, lệch 0,037 so với ngưỡng 0,00005). Chạy lại riêng file đó: **đỏ y hệt, cùng con số tới 17 chữ số thập phân**. Hai lần trùng khít như vậy đọc rất giống "đỏ ổn định, không flaky" — và kết luận đó **sai**.

Sự thật: `footPlantIK.ts` và test của nó được sửa lúc **15:33 và 15:34**, tức *trong lúc* phiên đang chạy, bởi việc song song. Bốn lần chạy sau đó — hai lần riêng file, một lần toàn bộ, một lần có coverage — đều **394/394 xanh**.

⇒ Hai phép đo trùng khít nhau **không** chứng minh tính ổn định nếu chúng cùng nằm trước một lần sửa. Khi các file liên quan đang ở trạng thái `??`/`M` và có người khác đang gõ, hãy **kiểm `mtime` trước khi kết luận** — `ls -l --time-style=+%H:%M:%S <file>` mất một giây và phân biệt được "lỗi thật" với "ảnh chụp cũ".

### 0.2 Ba cái bẫy đã cắn trong phiên 27/07 — đọc để khỏi mất buổi

**1. Commit chứa CẢ mã nguồn lẫn tài liệu thì `docs-check` không thể xanh.** Sha trong front-matter phải trỏ vào *chính commit đang được tạo* — thứ chưa tồn tại lúc viết. Hậu quả thật: `46afef4` sửa `lib.rs` + `boot.rs` (nằm trong `covers` của 4 tài liệu), gate xanh **trước** khi commit nên đã push, và đỏ **ngay sau** khi push. Cách đúng: **commit mã nguồn trước, rồi một commit chỉ-tài-liệu** trỏ vào sha vừa tạo (`2b12125` là vế thứ hai đó). Commit tài liệu không chạm file mã nguồn nào nên sha đứng yên.

**2. `commit:` và `stale-ok:` không thay thế nhau được.** `commit: <sha>` = "tôi đã đối chiếu *nội dung* tài liệu với commit đó". `stale-ok: <sha>` = "tôi đã *đọc diff* và không có gì cần đổi". Bump `commit:` khi bạn không sửa gì là dập một cảnh báo thật bằng một lời khai không có thật.

**3. GitNexus index — lỗi báo ra không phải lỗi thật.** Reindex 27/07 hỏng hai lần liên tiếp vì hai nguyên nhân khác nhau, **cả hai đều là lỗi công cụ, không phải mã nguồn**:
- *Trùng khoá chính* `Function:…run_stress_test:0` — nghe như mã nguồn có hàm trùng tên, nhưng file chỉ có **một** `def run_stress_test`. Thủ phạm: schema đổi **v4 → v5** buộc rebuild toàn bộ, rồi nó `CREATE` chồng lên 5 499 embedding cũ thay vì `MERGE`. **Luôn kiểm mã nguồn trước khi tin thông báo lỗi của công cụ.**
- *Xoay vòng WAL thất bại* ở ngưỡng mặc định ~16 MB. Đã ghim `walCheckpointThreshold: 67108864` vào `.gitnexusrc`.

Sau khi ghim, chạy `npx gitnexus analyze` là **incremental 52 s** thay vì rebuild 258 s. Lưu ý `embeddingDims` **cố tình không** đọc từ `.gitnexusrc` (schema đọc biến môi trường lúc nạp module, *trước* khi rc được đọc — đặt vào rc sẽ lệch âm thầm với cột vector); phải dùng `--embedding-dims` hoặc `GITNEXUS_EMBEDDING_DIMS`.

---

## 1. Đường cơ sở đã đo — **02/08/2026 tại `260c643`** (thay bảng 29/07)

Tất cả các số dưới đây do **chạy thật**, không trích từ tài liệu. Lệnh kèm theo để tái lập.

Bảng này đo sau khi hạ **253 file** đang treo trong cây làm việc thành ba lát commit (`98efc55` mã nguồn · `e6391eb` tài liệu · `260c643` vá cuối), và **CI đã xác nhận 25/25 xanh trên `260c643`** — nên đây là lần đầu đường cơ sở được đo ở một trạng thái mà *cả* máy dev *và* runner sạch cùng đồng ý. Bảng 29/07 đo với 3 file `.rs` chưa commit; bảng này không có ngoại lệ nào.

| Cổng | Lệnh | Kết quả 02/08/2026 | So 29/07 |
|---|---|---|---|
| Test Rust | `cargo test --no-fail-fast` (gốc workspace) | **625 pass · 0 fail · 2 ignored**, 31 binary test | ↑ từ 564 / 20 binary |
| Clippy (gate cứng) | `cargo clippy --all-targets --message-format=short` rồi đếm `": warning:"` | **0 warning** | = |
| **Format** (gate MỚI) | `cargo fmt --all -- --check` | **0** | mới có — xem [§gate mới](../02-van-hanh/04-kiem-thu-va-ci.md) |
| Typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` (trong `liva-ui/`) | **0 lỗi** | = |
| ESLint | `npx eslint . --max-warnings 0` | **0 warning** | = |
| Test + Coverage UI | `npm run test:coverage -w liva-ui` | **287 pass / 29 file** — **68,15 % stmt · 50,01 % branch · 54,33 % func · 70,37 % line** | ↑ cả bốn |
| Lỗ hổng npm | `npm audit --audit-level=high` (**toàn workspace**, không còn `--omit=dev`) | **0 vulnerabilities** | phạm vi rộng hơn trước |
| **Lỗ hổng Rust** (gate MỚI) | `cargo audit` (cargo-audit ghim `0.22.2`) | **exit 0** — 0 vulnerability trên 857 crate; **22 warning** `unmaintained`/`unsound` | mới có |
| Vỏ Tauri | `cargo check -p liva-desktop` | **0** | = |
| **Test vỏ Tauri** (gate MỚI) | `cargo test -p liva-desktop` | **xanh** (qua CI bước 22) | mới có |
| Module thử nghiệm | `cargo check --all-targets --features experimental` | **0 lỗi** | = |
| Sức khoẻ tài liệu | `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` | **exit 0** — và nay kiểm cả **78 neo `#anchor`** | cổng mạnh hơn |
| Trích dẫn tài liệu | `node scripts/docs-citations.mjs --max-unchecked=508` | pass — 56 tài liệu · 1 086 trích dẫn, **207 không kiểm được**, **0 neo hỏng** | phạm vi đổi, xem ghi chú |
| E2E WebSocket | `node scripts/e2e-gateway-ci.mjs` (tự dựng + tự chạy binary debug) | **8/8 đạt** | = |
| E2E bộ nhớ | gateway :8099 + `node scripts/e2e-memory.mjs` | *chưa đo lại* — số gần nhất **6/6** (26/07) | không đo |
| **TTFT** ([U9](#u9--một-con-số-ttft-đo-được)) | `.\target\release\ttft_bench.exe 20` | *chưa đo lại* — số gần nhất **p50 667 ms CPU · 18 ms CUDA** (29/07) | không đo |

#### Đo lại 04/08/2026 tại `596e8b6` — chỉ những dòng đã đổi

Bảng trên giữ nguyên vì nó là **ảnh chụp có ngày** của `260c643`. Dưới đây là các cổng cho kết quả khác khi chạy lại ở `596e8b6`; cổng nào không có tên ở đây thì kết quả không đổi.

| Cổng | Kết quả 04/08/2026 | So 02/08 |
|---|---|---|
| Test Rust | **635 pass · 0 fail · 3 ignored**, 31 binary test | ↑ từ 625 |
| Test + Coverage UI | **299 pass / 30 file** — **73,85 % stmt · 55,84 % branch · 63,86 % func · 75,88 % line** | ↑ cả bốn |
| Lỗ hổng npm | **exit 1 — 5 lỗ hổng (3 high)**: fast-uri, ip-address, undici, hono, postcss | ↓ **không phải hồi quy của repo** |
| Test vỏ Tauri | **12 pass · 0 fail** (chạy trực tiếp, không qua CI) | số cụ thể thay cho "xanh" |

**Về dòng npm:** `package-lock.json` không đổi giữa hai lần đo. Kiểm bằng cách stash toàn bộ cây rồi chạy lại trên HEAD sạch — **cũng đỏ**. Đây là advisory mới công bố trong hai ngày, cùng hiện tượng trôi số đã ghi cho `cargo audit`, và phải vá bằng một commit nâng phụ thuộc riêng chứ không phải bằng cách sửa mã nguồn.

**Hai bẫy đo đạc mới, cùng họ với các bẫy ở [§0.2](#02-ba-cái-bẫy-đã-cắn-trong-phiên-2707--đọc-để-khỏi-mất-buổi):**

1. **`docs-citations.mjs` chết vì hết heap, không phải vì tài liệu sai.** Nó duyệt cây bằng `readdirSync` với `IGNORE_DIRS` cố định (`node_modules`, `target`, `models`…) **không có `venv`**. Từ khi `tools/wakeword/venv` tồn tại (240 gói site-packages), lệnh này nổ `JavaScript heap out of memory` ở mặc định 4 GB. Với `node --max-old-space-size=12288` thì **exit 0, 0 neo hỏng**. CI clone sạch nên không có `venv` ⇒ **CI không dính**; đây là lỗi chỉ xuất hiện trên máy dev.
2. **`liva-native-core/Cargo.lock` là rác thời tiền-workspace.** Chạy `cargo audit` từ thư mục đó quét **319 crate** và báo **"1 vulnerability"** (crossbeam-epoch, RUSTSEC-2026-0204). Chạy từ gốc — như CI làm — quét **857 crate** và cho **0 vulnerability**. Cùng loại rác với `liva-native-core/target/`; luôn chạy `cargo audit` từ gốc workspace.

#### Đo lại 05/08/2026 tại `2dc8e2e` — 14 cổng, chỉ những dòng đã đổi

Đo trên cây làm việc **trước khi** hạ nó thành hai lát commit (`2dc8e2e` mã nguồn · lát tài liệu ngay sau). Cổng nào không có tên ở đây thì kết quả không đổi so với 04/08.

| Cổng | Kết quả 05/08/2026 | So 04/08 |
|---|---|---|
| Test Rust | **657 pass · 0 fail · 3 ignored**, 35 binary test | ↑ từ 635 / 31 |
| Lỗ hổng npm | **0 vulnerabilities** | ↑ từ 5 (3 high) — vá ở `30349c5`, đúng như dự đoán "phải vá bằng commit nâng phụ thuộc" |
| Test + Coverage UI | **299 pass / 30 file** — 73,75 % stmt · 55,82 % branch · 63,74 % func · 75,77 % line | ≈ (−0,1 điểm cả bốn) |

⚠️ **Dòng coverage giảm 0,1 điểm KHÔNG phải hồi quy, và cách xác định điều đó mới là phần đáng ghi.** `git diff 596e8b6..HEAD -- liva-ui/` trả về **rỗng** — nguồn UI không đổi một byte giữa hai lần đo. Cùng nguồn, khác số ⇒ nhiễu của chính phép đo (nhiều khả năng do `30349c5` nâng lockfile, đổi cây phụ thuộc mà istanbul đo qua). **Đừng đuổi theo 0,1 điểm coverage khi `git diff` của thư mục đó rỗng** — thời gian đó dành cho chỉ số khác có ích hơn.

**Cổng mới có mặt trong lần đo này:** `npm run devkit:lint` + `node scripts/actionlint.mjs` — **pass**.

#### 🔴 `docs-citations` chỉ nhìn thấy 40 % bộ trích dẫn — đo 05/08/2026

Dòng "Trích dẫn tài liệu" ở bảng trên **phải đọc kèm phạm vi**, nếu không nó nói quá:

| | Trích dẫn | |
|---|---:|---|
| Cổng thật sự kiểm | **1 085** | 59 tài liệu sống |
| **Ngoài phạm vi kiểm** | **1 636** | 40 tài liệu đông lạnh (11 FREEZE + toàn bộ `docs/99-luu-tru/`) |
| | | ⇒ cổng đọc **40 %**, mù **60 %** |

`listDocs` trong `scripts/docs-citations.mjs` bỏ cả thư mục `99-luu-tru/` và mọi tài liệu `disposition: FREEZE`. **Đó là quyết định đúng** — snapshot có ngày mô tả mã nguồn của quá khứ, bắt nó khớp HEAD là bắt nó nói dối. Nhưng "không chặn" đã bị làm thành "không nhìn", và hai thứ đó khác nhau: ai đọc `✅ Không có neo hỏng` mà không biết phạm vi sẽ hiểu thành *"mọi trích dẫn đều đúng"*.

Đã vá bằng cách **đo mà không chặn**: mỗi lần chạy, cổng in thẳng phạm vi ngoài tầm, kèm số trích dẫn trỏ quá độ dài file (**100** khi đo). Biến đếm riêng, không cộng vào `total`/`khongKiem`/`deXuat`, **không đụng exit code**. Danh sách chi tiết: `node scripts/docs-citations.mjs --dong-bang`.

⚠️ **Con số 100 KHÔNG phải hạn ngạch phải hạ, và đây là chỗ dễ hiểu sai nhất.** Nó đo **khoảng cách giữa HEAD và các mốc lịch sử**, không đo chất lượng tài liệu — mã nguồn tiến lên thì toạ độ trong snapshot *phải* lệch đi, đó là hành vi đúng. Sửa nội dung tài liệu FREEZE để nó nhỏ lại là làm sai lệch sử liệu. Cùng loại với [22 warning `cargo audit`](#1-đường-cơ-sở-đã-đo--02082026-tại-260c643-thay-bảng-2907): một con số để **theo dõi**, không phải để **hạ**.

📌 Vì sao con số này lộ ra muộn: đợt sửa 42 toạ độ `lib.rs` ngày 05/08 hạ chốt `--max-unchecked` 508 → 207, nhưng `khongKiem` **không giảm một đơn vị nào** (đúng 207 trước và sau). Truy ra thì 270 trong 422 trích dẫn `lib.rs` của bộ tài liệu nằm trong vùng mù — tức phần lớn công việc đó diễn ra ở chỗ cổng không đếm. **Một chốt không nhúc nhích sau khi bạn vừa sửa đúng thứ nó đo là tín hiệu phải truy, không phải chuyện thường.**

**⚠️ Bẫy đo thứ ba, cùng họ với hai bẫy ở [§0.2 mục 3](#02-ba-cái-bẫy-đã-cắn-trong-phiên-2707--đọc-để-khỏi-mất-buổi) và bẫy `cargo test` ‖ `cargo clippy`.** Lần chạy `cargo test` đầu tiên đỏ với `CL.exe exited with code 1` giữa lúc biên dịch llama.cpp — trông y hệt build gãy ở HEAD. **Không phải:** lúc đó vitest + `vue-tsc` + `node --max-old-space-size=12288` + MSVC 20 luồng chạy song song, và MSVC chết vì cạn bộ nhớ. Chạy `cargo test` một mình: sạch, 657 pass. **Nhận dạng:** lỗi nằm trong C++ của *phụ thuộc* chứ không trong mã của bạn, và nó không tái hiện khi chạy đơn lẻ ⇒ nghi tài nguyên máy trước, đừng nghi HEAD. Quy tắc rút ra: **các cổng nặng chạy tuần tự, không song song** — đúng cùng kết luận với bẫy `cargo test` ‖ `cargo clippy`, chỉ khác nguyên nhân (bộ nhớ thay vì fingerprint).

**Ba dòng vẫn chưa đo lại, nói rõ thay vì bỏ lửng:** E2E bộ nhớ (cần model embedding + DB trên đĩa), TTFT và độ trễ vision (cần build release + CUDA).

---

### 🔄 ĐỔI ROUTER 02/08/2026 — một phần bảng trên đã hết hiệu lực

Router đổi từ **Qwen3-VL-2B** sang **gemma-4-E4B-it-qat-UD-Q4_K_XL** (`6723114`). Mọi con số phụ thuộc model trong bảng trên **đo trên Qwen**, nên phải đọc kèm mốc này:

| Dòng | Còn dùng được? |
|---|---|
| Test Rust · Clippy · Format · Typecheck · ESLint · Coverage · npm/cargo audit · docs | ✅ không phụ thuộc model |
| **TTFT** | ✅ **đã đo lại trên gemma 02/08**: p50 **30 ms** · p95 32 ms · min 29 · max 33 (CUDA, 20 lượt). Qwen 29/07 là 18 ms ⇒ chậm hơn **12 ms**, cả hai đều dưới ngưỡng cảm nhận |
| **E2E WebSocket** | ✅ vẫn 8/8 — bộ này chỉ kiểm giao thức và phân quyền, không chạm model |

**Số mới đo được ở lần đổi này** (`e2e-vision-ipc.mjs --release`, CUDA, 3 lượt):

| | Qwen3-VL-2B | gemma-4-E4B |
|---|---|---|
| `vision:ask` p50 | 1 539 ms | **877 ms** |
| Token ảnh mỗi màn hình | 2 279 | **513** |
| Lớp trên CUDA0 | 29 | 43 |
| Thông lượng text | 167,8 tok/s | 75,6 tok/s |

⚠️ **Đừng đọc "gemma chậm hơn 2,2× mỗi token" thành "gemma chậm hơn".** Ở vision nó **nhanh hơn 1,75×** vì cần ít hơn 4,4× token ảnh; ở câu suy luận nó xong nhanh hơn theo đồng hồ dù tok/s thấp hơn. **tok/s không phải độ trễ** — đây là chỗ dễ kết luận ngược nhất trong cả bảng.

⚠️ **Hai con số thông lượng trong tài liệu này KHÁC NHAU và cả hai đều đúng:** `model_compare` cho **75,6 tok/s**, `ttft_bench` cho **92,9 tok/s**. Chúng đo hai thứ khác nhau — độ dài prompt, trần token mỗi lượt và nội dung câu hỏi đều khác. Đừng ghép chúng thành một con số "thông lượng của gemma"; luôn dẫn kèm tên công cụ đã đo.

📌 Lý do đổi và bảng so chất lượng 5 mục: `bin/model_compare.rs` (`b9fd7a5`).

**Ba dòng KHÔNG đo lại phiên này, nói rõ thay vì bỏ lửng:** E2E bộ nhớ (cần model embedding + DB trên đĩa), TTFT (cần build release + CUDA), và **mật độ `.unwrap()`** — dòng này bị bỏ có chủ đích, xem ghi chú dưới bảng quy mô.

⚠️ **Đừng so thẳng 1 086 trích dẫn với 2 149 của bảng 29/07.** Cây tài liệu v2 vào ở `e6391eb` thay nhiều tài liệu cũ bằng tài liệu mới có ít toạ độ `file:dòng` hơn (dùng neo ký hiệu nhiều hơn), và 11 snapshot FREEZE bị bỏ qua theo `document-inventory.json`. Con số nhỏ đi **không** có nghĩa là mất trích dẫn; điều đáng theo dõi là **0 neo hỏng**, không phải tổng.

⚠️ **`cargo audit` chỉ đỏ vì *vulnerability*.** 22 warning `unmaintained`/`unsound` đến từ cây Tauri/GTK và **trôi theo RustSec chứ không theo mã LIVA** — con số này đổi *không* tự động là hồi quy. [A31-01](02-no-ky-thuat-va-rui-ro.md) ghi 21 khi đo ngày 31/07; chênh 1 là advisory mới công bố. Đừng đọc nó như hạn ngạch phải hạ.

**Hồi quy đã tìm ra và nguyên nhân.** `docs-check` đỏ ở 6 tài liệu tầng `03-danh-gia`, do `241e8f9` — **67 file, +5 280 dòng, gộp CẢ mã nguồn lẫn tài liệu trong một commit**. Đúng cái bẫy §0.2 mục 1 viết ngày 27/07, vi phạm hai commit sau đó. `docs-check` đọc `git log base..HEAD` nên đây là **đỏ ở commit**, không phải đỏ ở cây làm việc: CI trên `main` fail ở bước 3/19.

**🔴 Một cổng CI ĐỎ NGẪU NHIÊN ~20%, tìm ra 29/07/2026 — và nó nguy hiểm hơn một cổng đỏ hẳn.** `webrtc::pipeline::outbound_tests::speaker_queue_day_fail_fast_khong_giu_blocking_thread` **nhấp nháy: đỏ 1/5 lần** khi chạy cả suite. Nguyên nhân không phải hành vi mà là **đua lịch trình**: test bọc `spawn_blocking` trong `timeout(20ms)`, nên 20 ms đó phải đủ cho tokio *lên lịch* tác vụ, chạy nó, *và* trả kết quả — trong khi 491 test đang chạy song song. Lần đỏ bắt được có tổng thời gian 13,2 s so với ~8 s của các lần xanh, tức máy đang tải.

Đã vá bằng cách tách hai thứ bản cũ trộn làm một: **phép đo** chuyển vào *bên trong* closure (loại hẳn độ trễ lên lịch khỏi con số), còn **hàng rào treo** bên ngoài nới lên 10 s — nó không phải phép đo, vì chế độ hỏng cần bắt là chặn **vô hạn** nên hạn nào cũng bắt được. Kiểm chứng: 6 lần chạy cả suite liên tiếp đều xanh.

**Và một test nhấp nháy THỨ HAI, chỉ lộ ra trên CI** — `system_status_tests::khong_do_duoc_thi_null_chu_khong_phai_khong` (`lib.rs`). Nó đòi `cpuUsage` phải `null` **hoặc > 0**, rồi đỏ với `được: Number(0)`. Nhưng `cpuUsage` là tải CPU **ngoài** LIVA (trừ phần LIVA tự dùng qua `GetProcessTimes`), nên trên một runner rảnh **0 là số đo THẬT**. Test đã gộp *"0 vì không đo được"* với *"0 vì đúng bằng 0"* ⇒ xanh trên máy dev (luôn có gì chạy nền), đỏ trên máy rảnh. Vá bằng cách tách theo **đơn vị**: phần trăm thì `null` hoặc `0..=100` (cận **trên** là thứ mới, bắt được lớp lỗi đảo cặp ở bẫy 1 của U3); `totalMemory` giữ nguyên `> 0` vì ở đó 0 đúng là số giả; `freeMemory` đổi sang bất biến mạnh hơn — phải **≤ `totalMemory`**.

**🔴 Và một test nhấp nháy THỨ BA, 01/08/2026 — cùng họ, khác cơ chế.** `db::db_tests::tests::tu_choi_db_tu_tuong_lai` đỏ trên CI tại `bc20eb1` với `os error 32` ("file being used by another process") ở khâu **dọn file tạm**, không phải ở khâu khẳng định. **Bằng chứng nó là nhấp nháy chứ không phải hồi quy, và bằng chứng này là loại tốt nhất có thể có:** `bc20eb1` chỉ đổi `CLAUDE.md` + một file `docs/`, mã Rust **byte-identical** với `e6391eb` vừa xanh. Cùng mã, một lần xanh một lần đỏ.

Cơ chế: `DatabasePool::new` trả `Err`, nhưng r2d2 giữ pool sau một `Arc` dùng chung với thread bảo trì của nó ⇒ kết nối SQLite **đóng trễ**, không đồng bộ với lúc hàm trả về. Trên Windows, xoá file còn handle mở là lỗi cứng. Bản cũ gọi `remove_file` một phát nên ăn may theo tải máy.

Vá ở `f5cbd26` bằng đúng nguyên tắc của hai ca trước — **tách phép khẳng định khỏi phần không phải phép đo**: `assert!` giữ nguyên, phần dọn dẹp chuyển sang `xoa_file_test()` retry giới hạn 40 × 50 ms, và **vẫn panic khi hết hạn** vì một handle rò *vĩnh viễn* là lỗi thật. Retry nuốt độ trễ đóng, không nuốt rò rỉ.

⚠️ **Bản vá này mới có MỘT điểm dữ liệu** (`260c643` xanh). Một cuộc đua thì thắng phần lớn thời gian — `e6391eb` cũng đã thắng trước khi `bc20eb1` thua. Nếu nó đỏ lại ở đúng test đó thì giả thuyết "handle đóng trễ" **SAI** và phải đào lại; **đừng nới thêm thời gian chờ**, vì nới ngưỡng cho một cổng nhấp nháy đúng là cách biến nó thành cổng vô dụng.

**Vì sao ghi vào đây thay vì lặng lẽ sửa:** một cổng đỏ ngẫu nhiên **tệ hơn không có cổng**, vì nó dạy người ta bấm "chạy lại" — và thói quen đó sẽ nuốt luôn lần đỏ thật đầu tiên. Đây cũng là lý do phải phân biệt "test đỏ" với "test nhấp nháy" ngay khi thấy, chứ đừng chạy lại rồi đi tiếp.

**🟢 Cách kiểm cổng tài liệu ĐÚNG NHƯ CI, trước khi push — dùng worktree sạch.** Máy dev có những file mà bản checkout sạch **không bao giờ có**, và cổng `docs-citations` đi hỏi đĩa nên nó xanh ở đây, đỏ ở kia. Đã cắn thật ngày 29/07: `09-tich-hop-ngoai.md` trích một toạ độ dòng bên trong file `tokenizer.json` của model Nemotron ASR, mà thư mục chứa nó là **gitlink (mode `160000`) không có `.gitmodules`** ⇒ checkout sạch chỉ tạo thư mục **rỗng**. Chạy cục bộ: exit 0. Chạy trên CI: đỏ.

> **Và bản nháp đầu của chính ghi chú này đã tái tạo đúng lỗi nó đang mô tả** — nó chép lại nguyên văn đường dẫn kèm số dòng để minh hoạ, thế là `docs-citations` bắt được một neo hỏng **mới**, lần này nằm trong tài liệu bạn đang đọc. Bài học kép: (1) **viết về một trích dẫn hỏng cũng là tạo ra một trích dẫn hỏng** — mô tả nó, đừng chép nó; (2) phép kiểm worktree ở trên **đã bắt được lỗi đó trước khi push**, tức nó tự chứng minh giá trị ngay trong lần dùng đầu tiên.

```bash
git worktree add "$TEMP/liva-ci-check" HEAD
cd "$TEMP/liva-ci-check"
node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia
node scripts/docs-citations.mjs --max-unchecked=508
git worktree remove "$TEMP/liva-ci-check" --force
```

Worktree tái hiện **chính xác** thứ CI thấy — kể cả gitlink rỗng — mà **không đụng vào cây làm việc**, nên dùng được cả khi đang có việc dở dang. Rẻ (chỉ chạy script Node, không build). Cùng cách này cũng kiểm được `cargo check -p liva-native-core` ở trạng thái đã commit; riêng `cargo check -p liva-desktop` thì **không** chạy được trong worktree vì `build.rs` của nó cần `node_modules/sqlite-vec-windows-x64/vec0.dll` — CI có nhờ `npm ci`, worktree mới thì không.

**⚠️ Một bẫy đo mới, chưa có trong §0.2 — chạy `cargo test` và `cargo clippy` SONG SONG trên cùng `target/`.** Lần chạy đầu ra một loạt `error: crate 'moxcms' required to be available in rlib format, but was not found in this form` cộng `can't find crate for 'liva_native_core'` — trông y hệt build gãy ở HEAD. Không phải: `clippy` sinh `.rmeta` thay cho `.rlib`, hai tiến trình giẫm lên fingerprint của nhau. Chạy tuần tự thì sạch tuyệt đối. **Nhận dạng:** lỗi nói về *định dạng* crate phụ thuộc chứ không về mã nguồn của bạn ⇒ nghi công cụ trước, đừng nghi code. Cùng họ với bẫy §0.2 mục 3.

**Quy mô mã nguồn** (đếm 02/08/2026 tại `260c643`): Rust `liva-native-core/src` **123 file · 46 733 dòng**; `liva-ui/src` **50 file · 17 517 dòng**; vỏ Tauri `liva-desktop/src-tauri/src` **1 033 dòng**; test Rust `tests/` **19 file · 5 196 dòng**; tài liệu `docs/` **96 file · 28 551 dòng**. **857 crate** trong `Cargo.lock`.

**[U10](#u10--tách-handle_command) đã đi xa hơn bảng cũ ghi.** `handle_command` nay **140 dòng** trong `lib.rs`, định tuyến **8 miền qua `::owns()`** (`config`, `integrations`, `llm`, `memory`, `messaging`, `setup`, `skill_store`, `task`) và chỉ còn **5 nhánh chuỗi inline**. Thư mục `commands/` là **12 module · 2 890 dòng**. `lib.rs` **1 550** · `db.rs` **1 641** · `main.rs` **266** dòng.

**Hotspot còn lại** — **10** file > 1 000 dòng trong `liva-native-core/src` + `liva-ui/src` (phạm vi đếm này hẹp hơn phạm vi của [A31-04](02-no-ky-thuat-va-rui-ro.md), nên **hai con số không so thẳng được**). Đo lại 05/08/2026 tại `2dc8e2e`: `WidgetApp.vue` **1 811** · `MemoryViewer.vue` **1 667** · `db.rs` 1 641 · `websocket.rs` 1 630 · `llm/tool_calling.rs` 1 537 · `tts/vieneu/mod.rs` 1 167 · `use3DModel.ts` 1 160 · `mcp/client.rs` 1 139 · `tts/normalizer.rs` 1 059 · `vision/diff.rs` 1 026.

Bảng 04/08 tại `596e8b6` ghi **12 file** và mở đầu bằng `agent/graph.rs` **1 947** · `websocket.rs` **1 912**, với nhận xét rằng **hai trong ba dịch chuyển là đi lùi** — tính năng nhắn tin bằng giọng đắp thẳng vào đúng hai file đã đứng đầu bảng. **Cả hai nay đã đảo chiều:** `websocket.rs` → 1 630 (`147f55c`), `agent/graph.rs` → **806** và `lib.rs` 1 550 → **740** (`2dc8e2e`). Không file `.rs` nào còn trên 1 000 dòng ở hai vị trí đầu bảng nữa.

**Điều đáng chú ý hơn con số: hotspot đã đổi phía.** Bốn vị trí đầu từng là Rust; nay hai vị trí đầu là Vue (`WidgetApp.vue`, `MemoryViewer.vue`) và chúng **không giảm một dòng nào** qua cả hai milestone. Đó là phía chưa có bước tách nào, và cũng đúng là phía [U11](#u11--lấp-lỗ-test-widgetappvue) nhắm tới. Việc tách MemoryViewer trước đó **không phải là thứ đã cứu coverage** — bốn component con chỉ gánh 164 dòng; phần có tác dụng là **8 test mới**, đưa `functions` từ 23,8 % lên 60 %. Bài học đó áp thẳng cho U11: **tách file không tự sinh coverage, test mới mới sinh.**

**Quy mô mã nguồn đo lại 05/08/2026 tại `2dc8e2e`:** Rust `liva-native-core/src` **134 file · 48 770 dòng** (+11 file, +2 037 dòng so 02/08 — refactor tách file nên số file tăng nhiều hơn số dòng); `liva-ui/src` **54 file · 17 608 dòng**; test Rust `tests/` **20 file · 5 436 dòng**; **24 binary** trong `src/bin`; `commands/` **12 module · 2 890 dòng**. Vẫn **0 `TODO`/`FIXME`/`HACK`/`XXX`**.

**Mật độ panic — số CŨ, đo 29/07 tại `c6ec120`, KHÔNG đo lại ở `260c643`:** `.unwrap()` xuất hiện **96 lần trong code production** (106 lúc vào phiên 29/07, −10 sau [U7](#u7--dọn-unwrap-trên-đường-thoại)) và ~436 lần trong khối `#[cfg(test)]`.

**Vì sao cố ý không đo lại.** `98efc55` thêm 12 file `.rs` vào `src/` (46 733 dòng, +4 821), nên con số 96 gần như chắc chắn đã đổi. Nhưng chính đoạn dưới đây kết luận mật độ `unwrap()` là **chỉ số TỆ** cho độ bền, và đo lại một chỉ số tệ chỉ tạo ra một con số phải bảo trì. Nếu phiên sau cần nó, hãy đo **kèm phân loại** (bao nhiêu là `Regex::new(<hằng>)`, bao nhiêu chạm đầu vào người dùng) chứ đừng chỉ đếm — đếm trần thì con số mới cũng vô dụng như con số cũ.

⚠️ **Con số này chỉ có nghĩa khi đếm bằng bộ đếm phân biệt được `#[cfg(test)]` *và* bỏ comment.** Grep phẳng cho **524** — trộn lẫn code production với test, mà §9 lại cấm dọn unwrap trong test, nên số đó không dùng để nghiệm thu được. Bộ đếm cũng phải **cắt comment cuối dòng trước khi đếm**: một doc-comment *nhắc tới* `.unwrap()` sẽ bị tính thành một điểm panic — đúng lỗi đã xảy ra khi thêm doc-comment cho `doc_cache` trong `vieneu/g2p.rs`, làm số đếm cao hơn thực tế 1 đơn vị và suýt biến thành "còn sót một chỗ chưa sửa".

**Và quan trọng hơn con số: [U7](#u7--dọn-unwrap-trên-đường-thoại) đã chứng minh mật độ `unwrap()` là một chỉ số TỆ cho "độ bền".** 30 trong số đó là `Regex::new(<hằng chuỗi>)` — không đầu vào nào kích hoạt được. Đọc dòng này như một chỉ báo cần *phân loại*, đừng đọc nó như một hạn ngạch cần *hạ*.

**0 `TODO`/`FIXME`/`HACK`/`XXX`** trong toàn bộ `liva-native-core/src` + `liva-ui/src`.

**Ba điều kiện đo cần biết để không hiểu nhầm số trên:**

1. Đo trên **build debug**. Bản release có sẵn từ 26/07/2026 ([U1](#u1--build-release-và-kiểm-visionask-thật)): `target/release/liva-native-core.exe`, và `e2e-gateway.mjs` trên đó cũng **8/8** — khác biệt duy nhất là `vision:ask` trả mô tả thật thay vì lỗi "requires a release build". Trên debug, `vision:ask` trả **lỗi trong 969 ms**; đó là hành vi đúng và có chủ đích, không phải hỏng.
2. **Cây làm việc SẠCH lúc đo** — khác bảng 29/07, vốn đo kèm 3 file `.rs` chưa commit. Ở `260c643` chỉ còn `models/nemotron-asr` (nested repo có LFS, luôn hiện "modified content"). Mọi số ở trên vì thế **tái lập được từ một checkout sạch**, và CI đã tái lập chúng: 25/25 xanh.
3. **Phân biệt "đỏ ở cây làm việc" với "đỏ ở commit".** Trong phiên 26/07 có lúc `cargo check -p liva-desktop` hỏng vì `tts/vieneu/mod.rs` thiếu field — nhưng file đó **nguyên vẹn ở HEAD**, chỉ là một phiên song song đang sửa dở. Đây đúng loại nhầm lẫn khiến người ta tưởng có hồi quy trong khi không có; luôn kiểm ở commit trước khi kết luận. Bẫy `cargo test` ‖ `cargo clippy` ghi ở trên là biến thể thứ hai của cùng lớp lỗi này: **cái báo lỗi chưa chắc là cái hỏng**.

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
| ~~**U3**~~ ✅ **XONG 07/08/2026** | [Lệnh `preflight` báo trạng thái tài nguyên](#u3--lệnh-preflight-báo-trạng-thái-tài-nguyên--một-phần-cli-xong-26072026-ui-còn-nợ) | A | ~~Beta~~ | CLI 26/07; **màn hình UI xong 06/08** (`SystemView.vue` + `useGateway.ts`) |
| ~~**U4**~~ ✅ **XONG 26/07/2026** | [Đồng bộ `03-danh-gia/` với code](#u4--đồng-bộ-03-danh-gia-với-code) | B | ~~Hồ sơ~~ | đã xong |
| ~~**U5**~~ ✅ **XONG 26/07/2026** | [Biến drift tài liệu thành gate thật](#u5--biến-drift-tài-liệu-thành-gate-thật) | B | — | đã xong |
| ~~**U6**~~ ✅ **XONG 26/07/2026** | [Sửa con trỏ chết trong AGENTS.md](#u6--sửa-con-trỏ-chết-trong-agentsmd) | B | — | đã xong |
| ~~**U7**~~ ✅ **XONG 29/07/2026** | [Dọn `unwrap()` trên đường thoại](#u7--dọn-unwrap-trên-đường-thoại) | C | ~~Beta~~ | **tiền đề SAI** — xem phân loại; đã vá nhiễm độc khoá + 6 test rác + 2 lỗi trong `voice_stress` |
| ~~**U8**~~ ✅ **XONG 07/08/2026** | [Thu hẹp khoảng cách hai profile chạy](#u8--thu-hẹp-khoảng-cách-hai-profile-chạy) | C | ~~Beta · Hồ sơ~~ | `boot.rs` xong 26/07; **hai bảng "năng lực theo profile" đã cập nhật 06/08** |
| ~~**U9**~~ ✅ **XONG 29/07/2026** | [Một con số TTFT đo được](#u9--một-con-số-ttft-đo-được) | C | ~~Hồ sơ~~ | đã đo — **p50 667 ms CPU · 18 ms CUDA** |
| **U10** ◐ | [Tách `handle_command`](#u10--tách-handle_command) | D | — | **đang làm** — 6 miền tách xong, `lib.rs` 2 773 → 1 788 dòng |
| ~~**U11**~~ ✅ **XONG 07/08/2026** | [Lấp lỗ test WidgetApp.vue](#u11--lấp-lỗ-test-widgetappvue) | D | — | line **73,89% → 80,70%**, chốt per-file **50 → 80** |
| **U12** | [Tool calling (đang làm dở)](#u12--tool-calling-đang-làm-dở) | E | — | đang chạy |
| **U13** | [Consolidation ngữ nghĩa L2 → L3](#u13--consolidation-ngữ-nghĩa-l2--l3) | E | — | 1–2 tuần |
| **U14** | [Tự động chuyển router ↔ expert](#u14--tự-động-chuyển-router--expert) | E | — | 3–5 ngày |
| **U15** | [Nối `CodeAgent` vào LLM thật](#u15--nối-codeagent-vào-llm-thật) | E | — | 1 tuần |
| ◐ **U16** | [Gói demo "không alt-tab", có hiện chi phí](#u16--gói-demo-không-alt-tab-có-hiện-chi-phí) — dụng cụ đo xong 26/07; video chưa quay (vision 80 s chặn kịch bản đầy đủ) | F | Hồ sơ | còn quay |
| ~~**U17a**~~ | [Bộ chọn giọng VieNeu](#u17a--bộ-chọn-giọng--xong-26072026) — ✅ **XONG 26/07/2026** | F | — | xong |
| **U17b** | [Clone giọng thật](#u17b--clone-giọng-thật-bị-chặn-chưa-ước-lượng-được) — **BỊ CHẶN**: thiếu 2 model | F | Hồ sơ | chưa ước lượng được |
| ~~**U18**~~ | [Trí nhớ nhìn thấy được, ngay trên UI](#u18--trí-nhớ-nhìn-thấy-được-ngay-trên-ui) — ✅ **nghiệm thu 26/07** (người dùng chạy trên vỏ Tauri) | F | — | xong |
| ~~**U19**~~ | [Ba tool OS thật](#u19--ba-tool-os-thật) — ✅ **nghiệm thu 10/10 ngày 26/07**; độ sáng cố tình bỏ | F | — | xong (2/3 tool) |
| ◐ **U20** | [Bộ nhớ thị giác offline *(tuỳ chọn, đắt, có mìn)*](#u20--bộ-nhớ-thị-giác-offline-tuỳ-chọn-đắt-có-mìn) — **bước 1 (cổng đồng ý) xong 26/07**; chưa có dòng thu thập nào | F | — | còn thu thập |
| **U21** | [Sổ đo mỗi lượt — `turn_telemetry`](#u21--sổ-đo-mỗi-lượt--turn_telemetry) | E | **U14** (và có ích cho U13) | 0,5–1 ngày |
| **U22** | [Hỏi trước khi trả — nhịp truy xuất](#u22--hỏi-trước-khi-trả--nhịp-truy-xuất) | E | — | 1–2 ngày · **cuối hàng**, không chen ngang |
| ~~**U23**~~ ✅ **XONG 07/08/2026** | [Màn Kỹ năng đang báo 1 trong khi lõi có 7](#u23--màn-kỹ-năng-đang-báo-1-trong-khi-lõi-có-7) — màn hình nói dối, không phải thiếu năng lực | F | ~~Hồ sơ · Beta~~ | `list_skills()` dùng chung cho cả hai lệnh; **1 → 7**, +3 test |
| ~~**U24**~~ ✅ **XONG 06/08/2026** | [Hai lỗi nối dây trên đường lip-sync widget](#u24--hai-lỗi-nối-dây-trên-đường-lip-sync-của-widget) — nhân đôi tiếng + analyser bám chunk chưa phát | C | Beta · Hồ sơ | đã sửa, 5 test mới |
| ~~**U25**~~ ✅ **XONG 06/08/2026** | [`useVRM.ts` là code mồ côi](#u25--usevrmts-là-code-mồ-côi-và-nó-đã-làm-người-rà-kết-luận-sai) — ≈420 dòng mô tả sai hệ thống | C | — | đã xoá; hàm thuần tách sang `utils/avatarMath.ts` |
| ~~**U26**~~ ✅ **XONG 06/08/2026** | [Control tag đọc được giữa câu](#u26--control-tag-đọc-được-giữa-câu-không-chỉ-ở-đầu-lượt) — danh sách trắng hai phía TS/Rust | E | — | đã sửa, 12 + 14 test |
| **U27** ◐ | [Hợp đồng giao thức + SDK công khai cho cổng 8002](#u27--hợp-đồng-giao-thức--sdk-công-khai-cho-cổng-8002) — **tiền đề mục này SAI một phần**: hợp đồng đã có 974 dòng, client 0-dep đã có | E | Hồ sơ | ví dụ chạy được **xong 06/08** (3/3 trên gateway thật); còn phép đo trên người + gói npm |
| ~~**U28**~~ ✅ **XONG 06/08/2026** | [Endpoint tương thích OpenAI trên gateway](#u28--endpoint-tương-thích-openai-trên-gateway) — `/v1/models` · `/v1/chat/completions` (+SSE) · `/v1/audio/speech` | E | Hồ sơ | SDK OpenAI v7.4.0 **6/6**, 0 crate mới, mặc định TẮT |
| **U29** | [Vòng lặp chủ động có ngân sách tick](#u29--vòng-lặp-chủ-động-có-ngân-sách-tick) — trụ "chủ động" | E | Ba trụ cột | nhiều tuần |
| 🔴 **U30** | [Bù ngang ở pelvis — sóng răng cưa theo nhịp bước](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước) — **triệu chứng người dùng báo**; bước 0 là một dòng | C | U33 | 0,5 ngày cho bậc 1 |
| ~~**U31**~~ ✅ **XONG 07/08/2026** | [Ba khoản phí mỗi frame trên đường avatar](#u31--ba-khoản-phí-mỗi-frame-trên-đường-avatar) — **đo xong: (a) và (c) không đáng kể**, giá trị thật ở (b) và (d) | C | — | đã làm, +9 test |
| **U32** | [Retarget đang vứt đi phần lớn chuyển động](#u32--retarget-đang-vứt-đi-phần-lớn-chuyển-động) — giữ 11/52 track, bỏ hips position | E | — | 2–4 ngày |
| **U33** | [Locomotion đúng: nhịp cố định, blendspace, distance matching](#u33--locomotion-đúng-nhịp-cố-định-blendspace-distance-matching) | E | — | **nhiều tuần** · cần U30 xong trước |

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

#### ◐ Việc 1 ĐÃ XONG TỪ 31/07 nhưng KHÔNG AI GHI SỐ — đo lại 02/08/2026

`tauri build` đã chạy ngày 31/07/2026. Installer tồn tại, chưa từng được ghi vào đây:

| | |
|---|---|
| `target/release/bundle/nsis/LIVA_25.0.0_x64-setup.exe` | **224,4 MB** |
| ↳ dựng lại 03/08 sau khi hạ version về `1.0.0`: `LIVA_1.0.0_x64-setup.exe` | **229,99 MiB**, SHA-256 `72DF20B5…CB34` — xem [báo cáo phát hành](../02-van-hanh/release-v1.0.0-smoke-test.md) |
| `target/release/liva-desktop.exe` | 68,6 MB |
| `target/release/liva-native-core.exe` | 45,5 MB |
| Thời gian build | **không ai ghi lại** — không truy ngược được |
| DLL NVIDIA trong bundle | **không có cái nào** |

**🔴 Và đây là bản CPU, tức installer hiện có KHÔNG có vision dùng được.** Đối chiếu [U1a](#u1a--vision-trên-cuda-đo-xong): CPU cho `vision:ask` **~80 s/lượt**, CUDA cho **1,2–1,4 s**. Một trong ba trụ cột được quảng bá sẽ không dùng được trên bản mà beta tester nhận.

**Cách xác định, kèm chứng dương tính — đừng tin một kết quả âm tính chưa được kiểm chứng.** Bộ dò chuỗi trong binary chỉ có nghĩa nếu nó chứng minh được là *chạy được*, nên phải dò cả những chuỗi **bắt buộc phải có**:

| Chuỗi | Kết quả | Vai trò |
|---|---|---|
| `ggml`, `llama_`, `liva`, `LIVA_LLM_N_GPU_LAYERS`, `ggml_backend` | **5/5 THẤY** | chứng dương tính — bộ dò hoạt động |
| `cudart` | không | phụ thuộc bắt buộc nếu link CUDA |
| `ggml_cuda`, `CUDA0`, `no CUDA-capable` | không | dấu vết runtime CUDA |
| `cublas` | *thấy* | **bất thường đã ghi nhận** — nhiều khả năng là tên backend trong bảng đăng ký của ggml, vì `cudart` mới là thứ không thể thiếu khi link thật |

Kích thước củng cố kết luận: 45,5 MB nằm ở dải CPU mà [U1b](#u1b--ghim-cudaarchs-và-quyết-định-cách-phát-hành) đo (**43,4 MB**), cách xa CUDA ghim `sm_120` (**74,5 MB**).

⇒ **Quyết định phát hành ở U1b vẫn chưa được thi hành.** U1b chốt "MỘT bản CUDA phục vụ được mọi máy — có GPU thì 1,4 s, không có thì rơi về CPU", giá là ~830 MB. Bản đang có đi ngược lại: nhẹ hơn nhiều nhưng **không có vision**. Phải chọn có ý thức trước khi giao cho ai.

#### ◐ Việc 4: hai artifact di sản ĐÃ XÁC NHẬN, nhưng **đừng xoá vội**

| File | Ngày | Dấu vết |
|---|---|---|
| `release/desktop-client.exe` (11,2 MB) | 25/06 | chứa **Electron + Node**, **không** có `liva-native-core` ⇒ đúng là bản thời Node.js |
| `release/desktop-client-setup.exe` (2,5 MB) | 25/06 | không dấu vết nào (stub NSIS), cùng ngày ⇒ di sản theo liên đới |

⚠️ **Cả hai đều `gitignore`.** Xoá là **mất vĩnh viễn** — git không khôi phục được. U2 viết "xoá hoặc chuyển vào lưu trữ"; với file gitignored thì **chuyển** là lựa chọn duy nhất an toàn.

✅ **ĐÃ CHUYỂN 02/08/2026** sang `release/_luu-tru-nodejs-2026-06/`, kèm `README.md` ghi lý do và cách hoàn tác (chuyển ngược ra `release/`). Không xoá byte nào. `release/` nay chỉ còn `liva-mobile.apk` + `.idsig`.

`release/liva-mobile.apk` (27/06) là thứ khác, **không** thuộc phạm vi U2.

#### ✅ Quyết định U1b ĐÃ THI HÀNH — 05/08/2026, bộ cài CUDA đã dựng và đo

Bảng ở trên ghi *"bản đang có là bản CPU, tức installer hiện có KHÔNG có vision dùng được"*. Nay đã dựng bản CUDA thật, giữ **mặc định 9 kiến trúc**:

| | |
|---|---|
| `LIVA_1.0.0_x64-setup.exe` | **805,4 MB** (844 543 259 byte) |
| SHA-256 | `98F4A72CB1E060124D2170EA7566E2C4412488AA669A9DF05C13FCA826E0FBAB` |
| Thời gian cài (im lặng) | 54 giây |
| Ba DLL NVIDIA **trong bundle** | có — `cublasLt64_12` 643,4 MB · `cublas64_12` 108,4 MB · `cudart64_12` 0,5 MB, đáp **cạnh .exe** |
| `vision:ask` | **5/5 đạt**, p50 **937 ms** · min 844 · max 2031, 43 lớp trên CUDA0 |

Đường đóng gói: `npm run installer:windows:cuda` → `scripts/stage-cuda-redist.mjs` chép DLL từ CUDA Toolkit vào `cuda-redist/`, khai trong `bundle.resources` là `"cuda-redist": "./"`. Chi tiết và cách tái lập: [`02-van-hanh/03` §6.1](../02-van-hanh/03-trien-khai-va-runtime.md).

**🔴 Một lỗ hổng trong chính U1b, tìm ra trước khi build nên chưa kịp gây hại.** U1b chốt *"ghim `120a-real`: −63 % binary, không mất hiệu năng"* — đúng **trên máy dev**, và đó là cái bẫy. Danh sách mặc định phần lớn là `-virtual`, tức **PTX do driver JIT lúc nạp**, và chính PTX đó khiến một binary chạy được trên card nó chưa từng được biên dịch cho. Ghim `120a-real` vứt sạch PTX ⇒ máy RTX 30xx/40xx **không nạp nổi kernel CUDA**. Một phép đo về *kích thước* đã suýt được đọc thành một quyết định về *khả năng tương thích*. Giá của 9 kiến trúc: 937 ms so với 877 ms — **bằng không**, cả hai đều mẫu 3 lượt.

#### ◐ Việc 2 và 3 — đã làm được phần mô phỏng, phần còn lại vẫn cần máy thật

Cài bộ cài trên vào thư mục trắng, `PATH` rút còn `System32`, xoá mọi `LIVA_*`, rồi chạy `--preflight`. Checklist đầy đủ: [`02-van-hanh/03` §6.2](../02-van-hanh/03-trien-khai-va-runtime.md). Kết luận: máy mới cài **chạy được nhưng cụt gần hết**, và thứ chặn nhiều nhất là **model chưa tải**.

**Phép đo này đẻ ra hai lỗi thật, cả hai đều trong chính bộ chẩn đoán:**

1. **`--preflight` doạ sai 85 lần.** In `✗ ~80 s mỗi lượt` trên đúng máy đang chạy vision 937 ms. Nó **chép lại** quyết định `n_gpu_layers` của `boot.rs` thay vì **gọi** nó, và `533f3c6` đã thêm nhánh tự chọn theo VRAM ở dưới. Đã vá: `boot::gpu_layers_mac_dinh()` thành `pub`. Rồi lộ tầng thứ hai — `gpu_layers_theo_vram` trả 0 **ngay khi không đo được kích thước model**, nên trên máy mới cài lý do thật là *"chưa có model"*, không phải VRAM; thông điệp đã tách ba ca. **Đây là lần thứ ba cùng một dòng sai theo cùng một kiểu** (bản đầu quên `n_gpu_layers`; bản hai đọc env var; bản ba giải thích sai lý do) — mỗi lần vá đều đúng lúc đó rồi lệch khi mã dưới nó đi tiếp.
2. **`✓ sqlite-vec` xanh vì lý do sai trên máy dev.** `vec0_candidate_paths` xếp **đầu** một đường dẫn dựng từ `env!("CARGO_MANIFEST_DIR")` — hằng số biên dịch cứng trỏ về cây mã nguồn — nên bản `vec0.dll` đã cài cạnh .exe không bao giờ được thử tới. Sản phẩm vẫn đúng trên máy sạch thật, nhưng dòng ✓ đó **không chứng minh được bản đóng gói dùng được**.

**Vẫn cần máy/VM thật:** VC++ Redistributable và WebView2. Máy đo đã có sẵn cả hai; bộ cài *có nhúng* WebView2 offline, nhưng "có nhúng" không phải "đã chứng minh chạy trên máy chưa có". Đây là phần chặn beta còn lại, và nó **không suy luận từ code được**.

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

#### ⚠️ ĐÃ LÀM 29/07/2026 — nhưng TIỀN ĐỀ CỦA MỤC NÀY SAI, và "đếm về 0" là mục tiêu sai

**Kết luận trước, lý do sau: trong 52 `.unwrap()` của cây `tts/`, KHÔNG CÓ CÁI NÀO là điểm panic do đầu vào kích hoạt.** Câu *"38 điểm panic tiềm tàng nằm đúng trên đường chạy của mọi lượt nói"* không đúng. Phân loại đầy đủ, đọc từng dòng:

| Lớp | Số | Ở đâu | Kích hoạt được bằng đầu vào? |
|---|---|---|---|
| `Regex::new(<hằng chuỗi>).unwrap()` trong `OnceLock`/`LazyLock` | **30** | `normalizer.rs` 18 · `tts/g2p.rs` 9 · `vieneu/g2p.rs` 3 | **Không.** Chỉ hỏng nếu chính regex viết sai — lỗi lập trình nổ ngay lần chạy đầu của bất kỳ ai |
| `data[a..b].try_into().unwrap()` đọc từ điển nhị phân | **6** | `vieneu/g2p.rs` | **Không.** `PhonemeDict::new` đã chứng minh `position + count × record_size ≤ len` và `position ≥ 32` trước khi dựng xong |
| `RwLock::read()/write().unwrap()` | **10** | `vieneu/g2p.rs` | **Không trực tiếp** — chỉ nổ khi khoá đã nhiễm độc, tức panic **thứ cấp** |
| `dp[i].as_ref().unwrap()` (bất biến DP) | **1** | `vieneu/g2p.rs:440` | **Không.** Có guard `if dp[i].is_none() { continue }`, và vòng trong chỉ ghi `dp[j]` với `j > i` |
| Còn lại (`engine.rs` 2 · `piper.rs` 1 · `vieneu/mod.rs` 1 · `punc.rs` 1) | 5 | — | không thuộc đường phiên âm chữ→âm |

⇒ **Chuyển 30 điểm đầu sang `Result` chỉ thêm những nhánh lỗi không bao giờ chạy tới** — code dài hơn, khó đọc hơn, và không giảm được một panic nào. Đó là việc *trông giống* cải thiện. Nên "số đếm ở đường thoại về 0" bị **thay bằng** một nghiệm thu nói đúng điều U7 thật sự quan tâm: *người lạ gõ gì thì LIVA cũng không câm.*

**Đã làm — bốn thứ, mỗi thứ đều đo được:**

1. **Vá lớp DUY NHẤT thật sự có giá trị: 10 điểm nhiễm độc khoá.** `merged_cache`, `common_cache`, `missing_*`, `segmentation_cache` là **memoization thuần tuý** — không có bất biến nào bắc ngang hai lần ghi, nên mọi trạng thái đều là trạng thái hợp lệ của một cache. Nhưng `.unwrap()` ở đó biến **một** panic đơn lẻ ở đâu đó thành **mất TTS vĩnh viễn cả tiến trình**: mọi lượt nói sau đều panic tại cùng dòng, người dùng chỉ thấy LIVA câm hẳn tới khi khởi động lại — đúng chế độ hỏng khó lấy log nhất với beta tester offline. Nay đi qua `doc_cache`/`ghi_cache` (`unwrap_or_else(|e| e.into_inner())`), có doc-comment ghi rõ **vì sao đây là lựa chọn đúng chứ không phải đường tắt**, kèm cảnh báo đừng sao chép sang khoá bảo vệ trạng thái có bất biến. Có test nhiễm độc khoá **thật** (panic trong một luồng đang giữ khoá ghi) rồi khẳng định `phonemize` vẫn chạy — kèm `assert!(is_poisoned())` để test không xanh rỗng.
2. **Bộ test đầu vào rác** đúng như nghiệm thu đòi, cho **cả hai** module: rỗng · chỉ khoảng trắng · chỉ emoji · ký tự điều khiển · đảo chiều bidi · ghép tổ hợp Unicode · 100 KB · số dài bất thường · ngày vô nghĩa · viết tắt dính nhau. `normalizer` chạy chéo **6 mã ngôn ngữ**. `g2p` chạy trên **từ điển tổng hợp trong `%TEMP%`** nên **hermetic** — chạy được trên CI không có model, đúng nơi cần bắt hồi quy.
3. **Thêm ca DƯƠNG cho `PhonemeDict`.** Hai test từ chối đã có từ trước sẽ **xanh rỗng** nếu `new()` lỡ từ chối *mọi* thứ; giờ có một từ điển hợp lệ nhỏ nhất nạp được và tra cứu đúng, đi qua chính 6 `unwrap()` ở lớp 2 của bảng trên.
4. **Sửa hai lỗi thật trong `voice_stress.exe`** — xem khung dưới.

**`voice_stress.exe` ĐỎ SẴN ở HEAD, và cả hai lỗi đều là lỗi của phép kiểm, không phải của tính năng.**

- **Lỗi 1 — assert chỉ xanh khi môi trường HỎNG.** Năm assert ghim chuỗi IPA nguyên văn (`doʊktoʊɹ`, `mɪstɛɹ`…). Đo thật: `phonemize("Hello Dr. Watson.")` → `həlˈoʊ dˈɑːktɚ wˈɑːtsən`, tức mở rộng **chạy đúng** (`dˈɑːktɚ` chính là "doctor"), assert vẫn nổ. Những chuỗi kia là output **nhánh dự phòng** của `g2p.rs` — nhánh chỉ chạy khi `try_espeak_ng` thất bại; thông điệp lỗi của chính bản cũ còn ghi *"in fallback"*. Nghĩa là bộ assert này **chỉ đạt trên máy THIẾU espeak-ng**, trong khi espeak-ng là phụ thuộc bắt buộc. Thay bằng một **đẳng thức**: viết tắt phải phiên âm y hệt dạng viết đầy đủ (`"Dr."` ≡ `"Doctor"`) — độc lập phiên bản espeak, độc lập giọng, đúng cả trên nhánh dự phòng, và vẫn bắt được đủ hồi quy đáng lo (bảng mở rộng bị gỡ, ánh xạ sai từ, regex thôi khớp).
- **Lỗi 2 — thiếu tài nguyên bị xử như lỗi lập trình.** `std::fs::read(&tts_voice).expect(...)` giết cả chương trình bằng `Os { code: 3, kind: NotFound }` khi chưa có file giọng Kokoro — không nói thiếu file nào, không nói lấy ở đâu, và **chôn luôn ba nhóm kiểm phía sau vốn không cần TTS**. Nhánh STT ngay **phía trên trong cùng một hàm** đã hạ cấp mềm (`Err → None`) từ trước; đây là lệch do sót. Nay in tên file + lệnh lấy nó rồi bỏ qua phần TTS.
- **Vì sao không ai bắt được:** `voice_stress` **không nằm trong CI** (CI chạy `cargo test`, không chạy binary probe). Cùng mô thức đã ghi trong doc-comment `boot.rs`: *mọi lệch đều rơi đúng vào phía không ai kiểm.*

**Nghiệm thu — đo 29/07/2026:**

| Tiêu chí | |
|---|---|
| `verify_round2.exe` | **exit 0** |
| `voice_stress.exe` | **exit 0** (trước phiên này: exit 101) — chạy hết mọi mục, ASR trung bình 155 ms/lượt |
| Test đầu vào rác cho `normalizer` + `g2p` | **đạt** — 6 test mới, tất cả xanh |
| `cargo test` | 554 → **564 pass · 0 fail** |
| clippy `-D warnings` | **exit 0**, 0 warning |
| ~~Số đếm unwrap đường thoại về 0~~ | **BỎ — mục tiêu sai**, xem phân loại ở trên. Số thật: `tts/` 52 → **42**, toàn `src/` 106 → **96** |

**Việc còn lại thật sự đáng làm (không phải đếm unwrap):** `tts/g2p.rs` ghim đường dẫn model Kokoro vào `node_modules/kokoro-js/...` — một thư mục cache của npm, không phải `models/` như `LIVA_TTS_MODEL_PATH` mặc định. Trên bản cài không có `node_modules`, nên đường dò này chắc chắn hụt. Chưa sửa vì nó chạm cấu hình đường dẫn model, thuộc phạm vi U2.

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

#### ✅ NGHIỆM THU ĐẠT — 29/07/2026

`liva-native-core/src/bin/ttft_bench.rs`. Một lệnh, in cấu hình máy và số đo cạnh nhau trong cùng một lần chạy:

```powershell
cargo build --release --bin ttft_bench          # thêm --features cuda nếu có GPU
.\target\release\ttft_bench.exe 20              # 20 lượt + 1 lượt làm nóng
```

**Cùng máy, cùng model, cùng n=20, cùng prompt — chỉ đổi thiết bị:**

| | CPU (`--release`) | CUDA (`--release --features cuda`, `LIVA_LLM_N_GPU_LAYERS=99`) | Tỉ lệ |
|---|---|---|---|
| TTFT p50 | 667 ms | **18 ms** | **37×** |
| TTFT p95 | 837 ms | **21 ms** | 40× |
| TTFT min / max | 630 / 838 ms | 17 / 21 ms | — |
| Thông lượng sau token đầu | 18,4 token/s | **193,9 token/s** | **10,5×** |

Máy đo: 20 lõi luận lý · RAM 47,8 GiB · RTX 5060 Ti 16 311 MiB · `n_ctx` 4096 · Qwen3-VL-2B-Instruct Q4_K_M (1,03 GiB) · build **release**. Nạp model 1,6 s.

⇒ **18 ms là dưới ngưỡng cảm nhận được của con người.** Cùng kết luận với [U1a](#u1a--vision-trên-cuda-đo-xong) ở phía thị giác: nút thắt của LIVA không phải model, mà là **chạy đúng thiết bị**. Cả hai trụ đều nhảy một bậc độ lớn khi có GPU.

**Ba bẫy đo đã né, ghi lại vì chúng đều cho ra số đẹp và SAI:**

1. **Cache tiền tố.** `generate_completion` so prompt mới với `last_tokens` và **bỏ qua phần prefill đã trùng**. Gửi đúng một prompt 20 lần thì lượt đầu đo prefill thật, 19 lượt sau đo gần bằng 0 — p50 tụt xuống một con số rất ấn tượng và vô nghĩa. Bench gắn **nhãn phiên khác nhau ngay đầu prompt** để tiền tố chung bằng 0. Có unit test khoá điều này lại (`nhan_phien_nam_dau_prompt_va_khac_nhau_giua_cac_luot`) — nếu ai đó chuyển nhãn xuống cuối prompt, test đỏ chứ không phải benchmark âm thầm đo sai.
2. **"Token đầu tiên" có hai nghĩa.** Engine lọc kênh suy luận nội bộ và gửi **chuỗi rỗng** làm nhịp tim trong lúc lọc. Bench đo cả `TTFT thô` (callback đầu tiên bất kỳ) lẫn `TTFT nhìn thấy` (mảnh không rỗng đầu tiên — lúc chữ hiện ra và TTS bắt đầu nói được). **Trên Qwen3-VL-2B-Instruct với prompt này hai số TRÙNG NHAU ở cả 40 lượt** ⇒ model không phát khối suy luận nào. Đó là một *kết quả*, không phải lý do bỏ bớt một cột: đổi sang model có `<think>` thì chúng tách ra, và lúc đó chỉ in một số là tự chọn định nghĩa rồi giấu chuyện đã chọn.
3. **Lượt đầu luôn đắt hơn** (dựng đồ thị, chạm trang mmap lần đầu) ⇒ một lượt làm nóng chạy trước và **không** vào thống kê.

**Một đính chính cho [U1b](#u1b--ghim-cudaarchs-và-quyết-định-cách-phát-hành).** Bẫy số 2 của U1b nói "build xong quá nhanh là dấu hiệu phép đo hỏng" (42,9 s ⇒ `CMakeCache.txt` cũ sống sót). Ở phiên này build CUDA xong trong **46 s** và **vẫn đúng** — vì `llama-cpp-sys-2` bản CUDA đã dựng sẵn từ 26/07, chỉ còn link binary mới. ⇒ Đọc heuristic đó cho đúng: nhanh bất thường là **dấu hiệu phải đi kiểm**, không phải kết luận. Phép kiểm thật vẫn là hai dòng log: `ggml_cuda_init: found 1 CUDA devices` và `layer N assigned to device CUDA0`.

**p95 với n nhỏ.** Chương trình in cảnh báo khi `n < 20` rằng p95 khi đó **chính là giá trị lớn nhất**, không phải ước lượng đuôi phân phối. Hai số trên đo ở n = 20 nên p95 là phần tử thứ 19/20 — vẫn mỏng, nhưng không còn là max.

---

### U24 — Hai lỗi nối dây trên đường lip-sync của widget

> **Nguồn gốc.** Rà `github.com/proj-airi` + `moeru-ai/airi` ngày 06/08/2026 (MIT, Vue + Electron, TTS chủ yếu qua API cloud). Ở riêng khâu lip-sync thì **LIVA không thua**: AIRI dẫn khẩu hình bằng biên độ, còn `use3DModel.ts:977-1057` đã là **5 dải tần → 5 viseme** (`aa/oh/ee/ih/ou`) — tinh hơn một bậc. Nhưng đọc kỹ để so sánh thì lộ ra hai lỗi nối dây. Không mục nào dưới đây là port từ AIRI.

**Lỗi 1 — nguồn âm mắc song song: nhân đôi tiếng, và vô hiệu hoá một nửa audio-ducking.**

`useSpeakerPlayback.ts:115` đã nối `source → outputNode`; với widget `useMasterGain: true` (`WidgetApp.vue:479`) thì `outputNode` là `masterGain`, và `masterGain → destination` (`useSpeakerPlayback.ts:95`). Sau đó `startAudioDrivenLipSync` nối thêm nhánh thứ hai `source → analyser → destination` (`use3DModel.ts:1008-1009`). Cùng một buffer về đích bằng **hai đường**, Web Audio cộng lại:

- biên độ khoảng **2×** trên widget;
- `setMasterVolume()` — chính là đường xử lý sự kiện `audio_ducking` ở `WidgetApp.vue:1335-1338` — chỉ hạ được **một** trong hai nhánh, nên hạ hết cỡ vẫn còn nhánh kia kêu nguyên. Comment ở `useSpeakerPlayback.ts:22` ghi master gain là *"required for audio ducking"*; nhánh analyser phá đúng cam kết đó.

**Lỗi 2 — analyser bám vào chunk CHƯA phát, nên miệng đóng giữa lượt nói.**

`onSourceStarted` được gọi bên trong `scheduleBuffer` ngay sau `source.start(nextStartTime)` (`useSpeakerPlayback.ts:128-131`), mà `nextStartTime` nằm ở **tương lai** với mọi chunk sau chunk đầu (`useSpeakerPlayback.ts:129`). `startAudioDrivenLipSync` lại mở đầu bằng `stopAudioDrivenLipSync()` (`use3DModel.ts:1001`) — ngắt analyser khỏi chunk **đang nghe thấy** và `smoothedBandRMS.fill(0)`.

⇒ Từ lúc chunk N+1 được *xếp lịch* tới lúc nó *thật sự kêu*, analyser đọc một nguồn im lặng, `getByteFrequencyData` trả toàn 0, **miệng đóng trong khi LIVA vẫn đang nói**. TTS sinh nhanh hơn thời gian thực thì trạng thái đóng chiếm gần hết lượt.

⚠️ **Test hiện có không bắt được lỗi nào trong hai.** `use3DModel.test.ts:121-146` chỉ gọi start/stop rồi thôi — không khẳng định gì về hình dạng đồ thị âm thanh. Cả hai lỗi đều xanh.

**Cách sửa.** Một analyser **bền, nằm TRONG chuỗi**, thay cho một analyser mắc song song và dựng lại mỗi chunk: `source → analyser → masterGain → destination`, do `useSpeakerPlayback` sở hữu và tạo đúng một lần. `use3DModel.startAudioDrivenLipSync` nhận sẵn `AnalyserNode` thay vì tự tạo. Một đường duy nhất ⇒ hết nhân đôi và ducking hạ được toàn bộ; analyser không bị tháo ở biên chunk ⇒ khẩu hình bám đúng thứ đang kêu.

**Nghiệm thu.** Ba khẳng định chạy được, trong `use3DModel.test.ts` và `useSpeakerPlayback.test.ts`:

1. Sau khi xếp lịch một chunk, **không** node nào nối thẳng tới `ctx.destination` ngoài `masterGain`.
2. Xếp lịch **hai** chunk liên tiếp ⇒ `createAnalyser` được gọi đúng **1** lần, và analyser **không** bị `disconnect()` xen giữa.
3. `startAudioDrivenLipSync` không còn tự gọi `connect(ctx.destination)` — kiểm bằng chính chữ ký hàm: nó nhận `AnalyserNode`, không nhận `AudioContext`.

---

#### ✅ ĐÃ LÀM — 06/08/2026

**Quyền sở hữu đổi chỗ, và đó là toàn bộ bản vá.** Analyser nay thuộc `useSpeakerPlayback` (bật bằng tuỳ chọn mới `enableAnalyser`), nằm ở **đầu** chuỗi ra và dựng đúng một lần cho mỗi `AudioContext`:

```
source → analyser → masterGain → destination
```

`use3DModel.startAudioDrivenLipSync` đổi chữ ký từ `(AudioContext, AudioBufferSourceNode)` sang `(AnalyserNode)` — nó chỉ còn **đọc**, không tạo và không nối gì. Đây không phải chuyện gu thiết kế: chữ ký cũ là thứ *cho phép* cả hai lỗi tồn tại, vì nó trao cho engine avatar đủ quyền để tự dựng một nhánh âm thanh thứ hai.

⚠️ **Một chế độ hỏng MỚI do chính bản vá đẻ ra, đã chặn.** Analyser giờ nằm *trong* chuỗi, nên `stopAudioDrivenLipSync()` mà còn gọi `disconnect()` như cũ thì **cắt đứt tiếng hoàn toàn**, không chỉ tắt khẩu hình. Lệnh `disconnect()` đã bỏ, kèm comment tại chỗ giải thích vì sao không được thêm lại, và một test khẳng định `disconnect` **chưa từng** được gọi.

Điểm bám cũng dời: từ `onSourceStarted` (chạy **mỗi chunk**, đúng nguyên nhân lỗi 2) sang `onPlaybackStarted` (chạy **một lần mỗi lượt nói**). `startAudioDrivenLipSync` thêm cổng bất biến — bám lại cùng một analyser là không-làm-gì, để việc tái khởi động giữa lượt không reset `smoothedBandRMS` và làm miệng giật.

**Đo được:**

```
npx vitest run tests/composables/use3DModel.test.ts tests/composables/useSpeakerPlayback.test.ts \
               tests/components/WidgetApp.test.ts tests/components/VRMEngine.test.ts
   → 53 passed (48 cũ + 5 mới)

npx vue-tsc --noEmit -p tsconfig.app.json   → sạch
npx eslint . --max-warnings 0 --no-warn-ignored → sạch
```

**Một test cũ ĐÃ phải sửa** — `"should expose audio lip sync APIs"` khẳng định chữ ký hai tham số cũ. Sửa test ở đây là đúng, không phải né: nó khoá chính cái hợp đồng vừa được chứng minh là sai. Ba test còn lại của bốn file trên không đụng tới.

**Không đo được trong phiên này:** biên độ thật và mức ducking thật cần tai người trên bản build có TTS chạy. Cái đã chứng minh là **hình dạng đồ thị âm thanh** — một đường duy nhất tới loa, mọi đường qua `masterGain`, một analyser cho cả lượt. Đó là điều kiện cần; phần "nghe có to gấp đôi nữa không" vẫn phải nghe.

---

### U25 — `useVRM.ts` là code mồ côi, và nó đã làm người rà kết luận sai

`liva-ui/src/composables/useVRM.ts` (**≈420 dòng**) không có call-site sản xuất nào — chỉ `tests/composables/useVRM.test.ts` import nó. Bản dùng thật là `use3DModel.ts` (`VRMEngine.vue:16`).

Vấn đề không phải mấy trăm dòng chết, mà là **nó mâu thuẫn với bản đang chạy**: hàm `updateLipSync` của nó dẫn khẩu hình bằng ba hàm `sin` với `speed = 8` cố định, **không đọc một mẫu âm thanh nào**, kèm comment tự khen *"simulates natural speech patterns (NOT random rectangles)"*. Trong đợt rà 06/08/2026, chính file này khiến người rà kết luận "lip-sync của LIVA là giả" và báo cáo sai — trước khi tìm ra `use3DModel.ts` mới là bản chạy. Đây đúng loại bẫy mà mục [code mồ côi](02-no-ky-thuat-va-rui-ro.md) tồn tại để chặn: dòng chết thì vô hại, **dòng chết mô tả sai hệ thống thì không**.

**Nghiệm thu.** Xoá `useVRM.ts` cùng test của nó — hoặc, nếu muốn giữ các hàm thuần (`lerp`, `easeOutQuad`, `easeInQuad`, `randomBlinkInterval`, `weightedRandom`), tách chúng sang `utils/` rồi xoá phần còn lại. Sau đó cả ba phải đạt: `grep -rn "useVRM" liva-ui/src` trả **0** kết quả; `vue-tsc --noEmit -p tsconfig.app.json` sạch; `npm run test:coverage -w liva-ui` **không** tụt ngưỡng nào.

---

#### ✅ ĐÃ LÀM — 06/08/2026

**Khảo sát trước khi xoá lộ ra một chuyện tệ hơn "code chết".** Năm hàm thuần kia tồn tại **hai bản**: bản `export` trong `useVRM.ts` và một bản riêng trong `use3DModel.ts` — **giống nhau từng byte, chỉ khác từ khoá `export`**. Bộ test lại nhập từ bản mồ côi. Nghĩa là suốt thời gian qua, **thứ được kiểm và thứ được chạy là hai bản khác nhau**; chúng khớp nhau thuần tuý do may, và không có gì chặn chúng trôi khỏi nhau.

⇒ Chọn phương án tách thay vì xoá thẳng, vì xoá thẳng sẽ để lại bản đang chạy **không còn test nào**:

| | Trước | Sau |
|---|---|---|
| Bản định nghĩa | 2 (một mồ côi, một đang chạy) | **1** — `liva-ui/src/utils/avatarMath.ts` |
| Test kiểm bản nào | bản mồ côi | bản đang chạy |
| Phải mock để test | THREE + GLTFLoader + `@pixiv/three-vrm` + `useFaceTracking` | **không mock gì** — toán thuần |

Bộ test chuyển sang `tests/utils/avatarMath.test.ts`. Bỏ 5 test `clamp` của bản cũ: chúng kiểm một hàm **định nghĩa ngay trong file test**, không tồn tại trong `src/` — tức test đang kiểm chính nó.

**Đo được:**

```
grep -rn "useVRM" liva-ui/src          → 0 kết quả
npx vue-tsc --noEmit -p tsconfig.app.json → sạch
npx eslint . --max-warnings 0             → sạch
npm run test:coverage -w liva-ui          → 384 passed (384), không ngưỡng nào đỏ
```

Độ phủ `src/composables` **tăng** cả bốn chỉ số vì file mồ côi phủ kém đã biến mất:

| | statements | branches | functions | lines |
|---|---|---|---|---|
| Trước | 73,11 | 56,82 | 81,00 | 74,98 |
| Sau | **74,49** | **59,14** | **81,15** | **76,47** |

Số test giảm 394 → 384 (−34 test của `useVRM.test.ts`, +24 test của `avatarMath.test.ts`). Giảm số test ở đây là **đúng hướng**: 34 test cũ gồm 5 test tự-kiểm-chính-nó và một nhóm "Composable Interface" chỉ khẳng định một composable không ai gọi thì không ném lỗi.

⚠️ **Một sai lầm trong phiên, ghi lại vì nó rẻ và dễ lặp.** Test mới `weightedRandom` với mọi trọng số bằng 0 được viết theo kỳ vọng "trả về phần tử cuối" — đỏ. Hành vi thật: `total = 0` ⇒ `r = 0` ⇒ điều kiện `r <= 0` khớp **ngay vòng đầu** ⇒ trả về phần tử **đầu**. Dòng `return options[options.length - 1]` ở cuối hàm thực chất là phòng thủ gần như không tới được. Đã sửa kỳ vọng theo hành vi thật chứ không sửa hàm.

**Không làm, có chủ đích:** `clamp` đang có **bốn** bản trùng lặp với chữ ký khác nhau — trong `useAvatarLocomotion.ts`, `useFaceTracking.ts`, `footPlantIK.ts` và `WidgetApp.vue:660`. Cùng loại nợ, nhưng khác phạm vi — gộp vào đây là mở rộng mục vượt quá điều kiện nghiệm thu đã chốt. *(Ba file đầu chưa vào cây nên `docs-citations` không phân giải được toạ độ dòng của chúng — đó là lý do chỗ này chỉ nêu tên file.)*

---

### U30 — Bù ngang ở pelvis: sóng răng cưa đồng bộ với nhịp bước

> **Nguồn gốc.** Người dùng báo avatar "khựng khựng" khi đi. Một bản rà do ChatGPT thực hiện ngày 07/08/2026 chỉ vào `FootPlantIK`; **đã đối chiếu lại từng khẳng định với code và đo lại model** — xem "độ tin của bản rà" ở cuối mục. Chẩn đoán đứng vững, nhưng thứ tự thi hành mà bản rà đề xuất thì không.

**Cơ chế, đã kiểm.** `FootPlantIK` không phải IK. `footPlantIK.ts` (`update`) chọn bàn chân thấp hơn rồi neo toạ độ **thế giới** của nó; `useAvatarAnimation.ts` (`applyFootPlant`) bù sai lệch bằng cách **dịch cả `hips`** — không xoay đùi–gối–bàn chân.

Bù pelvis **theo trục đứng** là kỹ thuật chuẩn. Cái sai ở đây là bù theo **phương ngang**: `correction` có đủ `x`, `y`, `z` và cả ba đều được cộng vào `hips.position`. Tức là kéo lùi cả thân người để giữ bàn chân đứng yên — không phải foot-plant, mà là lôi nhân vật ngược lại.

**Vì sao nó đúng bằng nhịp bước.** Bốn hằng số trong constructor (`0.025`, `0.12`, `0.14`, `18`) khép thành một chu trình răng cưa:

| Pha | Điều gì xảy ra |
|---|---|
| Neo | Anchor đứng yên trong thế giới, nhân vật vẫn tiến ⇒ sai lệch tăng **tuyến tính theo tốc độ** |
| Bão hoà | Chạm trần `maximumCorrection = 0.14` sau **khoảng một phần mười giây**, rồi ghim ở đó |
| Đổi chân | Target nhảy về 0; `1 − exp(−18 × 0,0167) ≈ 0,26` mỗi frame ⇒ thân **lao tới trong ~4 frame** |
| Lặp | Mỗi bước chân một lần |

⇒ Đây là dao động răng cưa đồng bộ với nhịp bước — đúng nghĩa đen triệu chứng được báo.

**Bước 0 — làm TRƯỚC mọi thứ khác, và nó là một dòng.**

Bản rà đề xuất dựng bộ đo RAF p50/p95/p99 + Long Animation Frames *trước*, rồi mới A/B tắt `FootPlantIK`. **Ngược.** Tắt `FootPlantIK` là một dòng, nhìn 30 giây là biết. Dựng bộ đo là một ngày. Làm cái rẻ và quyết định trước; nếu tắt đi mà hết khựng thì bạn vừa tiết kiệm một ngày công để chứng minh thứ mắt đã thấy.

⚠️ **Và nếu tắt đi mà VẪN khựng thì toàn bộ mục này sai** — thủ phạm nằm chỗ khác, và [U31](#u31--ba-khoản-phí-mỗi-frame-trên-đường-avatar) mới là nơi đáng nhìn tiếp. Đừng bỏ qua nhánh đó: chưa ai trong hai bản rà **nhìn thấy** hiện tượng, cả hai đều là đọc code. Người duy nhất đã thấy là chủ dự án.

**Công tắc đã có — 07/08/2026.** `LIVA_FOOT_PLANT` đọc trong `useAvatarAnimation.ts` (`footPlantEnabled`), theo đúng lối `LIVA_ECO_MODE` sẵn có. **Mặc định BẬT**, so sánh với `false` chứ không ép boolean, nên quên khai báo không vô tình tắt nó.

```js
LIVA_FOOT_PLANT = false   // tắt ngay trong console, không build lại
LIVA_FOOT_PLANT = true    // bật lại để so sánh
```

Khi tắt, `hips` được **trả về tư thế gốc** chứ không chỉ `return` sớm — thiếu bước đó thì độ lệch của lượt bù cuối cùng đóng băng vĩnh viễn, trông như một lỗi khác hẳn và đủ để làm hỏng chính phép A/B. Có test khoá cả hai vế (`LIVA_FOOT_PLANT = false tắt bù và trả hips về tư thế gốc`).

⚠️ **Bẫy đo — trả giá 07/08/2026: không tự động hoá được phép A/B này.** Dựng dev server rồi bắt vị trí avatar trong 21 giây: toạ độ **không đổi một bit**. Nghi model chưa nạp — sai, `Liva.vrm` và cả **sáu** FBX đều tải 200. Nguyên nhân thật đo được bằng một phép thử:

```
requestAnimationFrame trong 1 giây: 0 lần   ·   document.hidden: true
```

Trình duyệt **treo hẳn `requestAnimationFrame` khi khung nhìn không được hiển thị**. Vòng lặp render không chạy, nên avatar đứng im, còn `update_interactive_zones` vẫn phát đều đặn vì nó nằm trên `setInterval` — tức là có tín hiệu sự sống *giả*, đủ để tin nhầm rằng mọi thứ đang chạy.

⇒ Phép A/B này **bắt buộc phải có người mở cửa sổ ra nhìn**. Không có đường tắt bằng đo đạc tự động, và mọi con số thu được khi khung nhìn ẩn đều vô nghĩa.

**Công thức chạy:**

```bash
npm run dev -w liva-ui
```

Mở `http://localhost:5173/widget.html`, đợi ~3 giây (wander bật mặc định, `WANDER_PAUSE_MIN = 2.5 s` rồi tự chọn đích). Không cần gateway — WebSocket lỗi là bình thường, avatar vẫn chạy.

**Nghiệm thu.**

1. **A/B có bằng chứng, không phải cảm nhận.** Quay hai đoạn `idle → walk → run → stop` cùng đường đi, một có một không `FootPlantIK`, đặt cạnh nhau.
2. Sau khi sửa: **không có bước nhảy vị trí pelvis** ở thời điểm đổi chân trụ — kiểm bằng log `hips.position` theo frame, không bằng mắt.
3. Chân trụ trượt **dưới 2 px/frame** ở tốc độ đi bộ.
4. `npm run test:coverage -w liva-ui` không tụt ngưỡng nào (ba file locomotion đều đang có test).

**Cách sửa, theo thứ tự tăng dần công sức** — dừng ở bậc nào đủ thì dừng:

- **Bậc 1:** bỏ hẳn `x`/`z` khỏi correction, chỉ giữ `y`. Mất khử trượt ngang, nhưng hết răng cưa.
- **Bậc 2:** two-bone IK thật cho đùi–gối–bàn chân; pelvis chỉ bù nhẹ theo trục đứng.
- **Bậc 3:** contact curve trái/phải gắn vào từng clip, IK chỉ chạy khi contact active — thuộc [U33](#u33--locomotion-đúng-nhịp-cố-định-blendspace-distance-matching).

**Độ tin của bản rà — kiểm rồi mới dùng.** Bản rà đưa ra số nghe như đã đo, nên đã đo lại độc lập. Lượt kiểm đầu tưởng nó bịa: đọc file `Liva.vrm` trên đĩa ra **309 node · 3 skin · 750 bone-link**, còn nó nói 333 node · 23 skeleton · 1 484 bone-link. **Sai ở phép kiểm, không phải ở bản rà** — nó đo *sau khi three.js nạp*, còn ở đây đo *file trên đĩa*:

- glTF gộp nhiều primitive vào một mesh; three.js tách mỗi primitive thành một `SkinnedMesh` ⇒ **23** đúng ở runtime (8 + 12 + 3 primitive).
- 309 node + 23 mesh object + 1 scene root = **333**. Khớp chính xác.
- Trước prune: **5 750** bone-link (tính được từ file). Sau `removeUnnecessaryJoints`: 1 484. Hợp lý.

Và các số còn lại khớp tuyệt đối: **250 bone · 44 481 triangle · 456 morph target · 11 bone được retarget**. ⇒ Bản rà **đã thật sự chạy đo**, và các khẳng định về code (hằng số, số lần `updateWorldMatrix`, lerp Euler, `findIndex` mỗi frame, ECO 5 FPS) đều đối chiếu đúng.

*Một chỗ bản rà nói chưa chuẩn:* retarget **có** dùng slerp quaternion **bên trong** mỗi clip (`sampleTrack`); chỉ khâu trộn giữa hai tư thế mới là lerp Euler. Không đổi kết luận, nhưng đừng đi sửa nhầm chỗ.

---

### U31 — Ba khoản phí mỗi frame trên đường avatar

**Đáng làm dù thủ phạm của [U30](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước) là gì** — cả ba đều độc lập với chẩn đoán, và cả ba đều rẻ.

**1. Duyệt lại toàn bộ đồ thị hai lần mỗi frame.** `applyFootPlant` trong `useAvatarAnimation.ts` gọi `scene.updateWorldMatrix(true, true)` **hai lần**: một trước khi đo vị trí bàn chân, một sau khi ghi `hips.position`. Trên đồ thị **333 node** đó là ~666 lần cập nhật ma trận mỗi frame chỉ để đặt bàn chân — chưa kể `vrm.update()` cũng đụng ma trận. Lần thứ hai có thể bỏ nếu để `vrm.update()` phía sau lo.

**2. `removeUnnecessaryJoints` đã deprecated.** `use3DModel.ts` **từng** gọi nó (đã đổi, xem khối ✅ bên dưới); three-vrm trong dự án là **^3.5.2**, bản khuyến nghị `combineSkeletons` — gộp **23** skeleton rời thành một skeleton dùng chung thay vì prune từng cái. Cân nhắc thêm `combineMorphs` (model có **456** morph target).

**3. `findIndex` tuyến tính mỗi bone mỗi frame.** `sampleTrack` trong `mixamoRetarget.ts` quét `track.times.findIndex(...)` từ đầu cho **mỗi** bone, **mỗi** frame. Cache con trỏ keyframe theo hướng tiến là đủ; clip chạy tuyến tính nên lần sau gần như luôn ở ngay cạnh lần trước.

**4. ECO Mode hạ avatar xuống 5 FPS.** `use3DModel.ts` đặt `throttleInterval = 200` khi `LIVA_ECO_MODE` bật. 5 FPS **chắc chắn** giật nếu nhân vật vẫn đang di chuyển — và ECO tồn tại để sống chung với workload nặng, tức đúng lúc người dùng vẫn đang nhìn. Sàn nên là **30 FPS**; muốn tiết kiệm thì hạ DPR, tắt antialias, giảm tần suất spring bone — đừng hạ frame rate.

**Nghiệm thu.** Một bộ đo RAF ghi p50/p95/p99 + thời gian `vrm.update` + render, chạy 20 giây `idle → walk → run → stop`: **p95 dưới 20 ms, p99 dưới 33 ms**, và **không frame nào vượt 50 ms** do mã animation. Đo trước và sau từng khoản trong bốn khoản trên — gộp cả bốn rồi mới đo thì không biết khoản nào có tác dụng.

---

#### ✅ ĐÃ LÀM — 07/08/2026, và **hai trong bốn khoản hoá ra không đáng kể**

Đo riêng từng khoản đúng như nghiệm thu yêu cầu. Kết quả ngược với kỳ vọng ở nửa danh sách:

| Khoản | Đo được | Nghĩa thật |
|---|---|---|
| **(a)** bỏ một lần `updateWorldMatrix(true, true)` | **11 µs**/lần trên cây 333 node ⇒ tiết kiệm **0,66 ms/giây** | **0,07 %** ngân sách khung hình 60 FPS — không sửa được cái giật nào |
| **(c)** nhị phân thay quét tuyến tính | 20 giây hoạt ảnh = 13 200 lần tra cứu: **0,28 ms → 0,17 ms** | ~5 ns/khung. **Không phải sửa lỗi hiệu năng**, chỉ là dọn dẹp |
| **(b)** `combineSkeletons` | chưa đo được — cần GPU thật | Khoản duy nhất còn tiềm năng đáng kể: 23 skeleton → 1 |
| **(d)** sàn ECO 5 → 30 FPS | 200 ms → 33 ms mỗi khung | Hiệu ứng lớn và thấy ngay — **nhưng chỉ khi ECO đang bật** |

⚠️ **Bản rà đã thổi phồng (a) và (c), và tôi đã đồng ý với nó mà chưa đo.** Cả hai đúng về mặt kỹ thuật — đường đi thừa thì nên bỏ — nhưng **không cái nào giải thích được hiện tượng khựng nhìn thấy bằng mắt**. Ai đọc mục này về sau: đừng trông đợi U31 sửa được cái giật; nó là dọn dẹp, và giá trị thật nằm ở (b) với (d).

⇒ Hệ quả cho [U30](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước): nếu tắt `FootPlantIK` mà vẫn khựng, **đừng quay sang U31 để tìm nguyên nhân** như bảng ba-kết-cục gợi ý — U31 đã đo và không đủ lớn. Lúc đó phải dựng bộ đo RAF thật.

**Đã thi hành:**

- **(a)** Bỏ lần duyệt thứ hai trong `applyFootPlant`. Ba lý do độc lập, mỗi lý do đủ để bỏ: idle sway / blink / lookAt chạy ngay sau và ghi đè rotation nên ma trận vừa dựng đã cũ; spring bone của three-vrm tự lo ma trận của nó (`_ancestors[i].updateWorldMatrix(...)`); và `WebGLRenderer.render()` gọi `updateMatrixWorld()` trước khi vẽ. Test đếm riêng lần `(true, true)` — `getWorldPosition` cũng gọi `updateWorldMatrix` nhưng ở dạng `(true, false)` chỉ đi ngược lên cha, rẻ và không tránh được.
- **(b)** `removeUnnecessaryJoints` → `combineSkeletons`. Chính thư viện in cảnh báo: *"deprecated. Use combineSkeletons instead… will be removed in the next major version."* Test khẳng định **cả hai chiều** — có gọi hàm mới, và đã thôi gọi hàm cũ.
- **(c)** Tìm nhị phân, **không** dùng con trỏ cache như đề xuất ban đầu: lúc crossfade có hai lượt lấy mẫu trên cùng một clip ở hai mốc thời gian, xen kẽ nhau, nên một con trỏ dùng chung sẽ bị kéo qua kéo lại. Nhị phân không giữ trạng thái nên miễn nhiễm. Kiểm 1 000 mẫu ngẫu nhiên: **0 lệch** so với ngữ nghĩa cũ, cộng 5 test biên.
- **(d)** `ECO_FRAME_INTERVAL_MS = 33`.

**KHÔNG làm, có chủ đích: `combineMorphs`.** Là tối ưu tuỳ chọn (model có 456 morph target), nhưng nó tái cấu trúc đúng đường morph đang dẫn chớp mắt, khẩu hình và biểu cảm — thứ vừa sửa ở [U24](#u24--hai-lỗi-nối-dây-trên-đường-lip-sync-của-widget). Kiểm chứng cần nhìn tận mắt, mà phiên này không nhìn được (xem bẫy `requestAnimationFrame` ở U30). Có test khẳng định nó **chưa** được gọi, để lần bật sau là một quyết định có ý thức chứ không phải trôi vào.

**Cổng:** `vitest` **398/398** (thêm 9 test) · typecheck sạch · ESLint sạch.

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

#### 🔴 ĐO LẦN BỐN — 02/08/2026: hướng ngưỡng ĐÓNG LẠI, lần này vì lý do sâu hơn

Chạy `tool_calling_probe` tại `548dc06` (catalog vừa thêm `get_weather`):

```
❌ KHÔNG tách bạch: 3 câu trò chuyện có điểm ≥ câu cần-tool thấp nhất (0.8357)
❌ Biên (top1−top2) cũng KHÔNG tách: 5 câu trò chuyện có biên ≥ biên cần-tool thấp nhất
   Cả hai giả thuyết rẻ đều chết.
```

**Lý do quan trọng hơn con số**, và chính probe in ra: toàn bộ điểm nằm trong **0,78–0,91** — dải hẹp là *bản chất họ E5* (cosine luôn cao). Nên **ngưỡng TUYỆT ĐỐI là ý tồi với model này, không chỉ với corpus này**. Kết luận "trống 0,0159" ở lần đo ba không sai lúc đó, nhưng nó mỏng tới mức không sống sót nổi một thay đổi catalog.

⚠️ **Chưa cô lập được nguyên nhân.** Giữa lần đo ba và lần bốn có ít nhất hai thứ đổi: catalog thêm một tool (`get_weather`), và phiên đo khác máy/khác thời điểm. Tôi **không** khẳng định `get_weather` là thủ phạm — nhưng dù thủ phạm là gì, kết luận vận hành vẫn giống nhau và mạnh hơn: **một ngưỡng tuyệt đối phải đo lại mỗi lần thêm tool**, và một tham số như thế thì không đáng đưa vào sản phẩm.

**Hệ quả cho U12:** không cài tiền lọc theo ngưỡng. Đường bật G1 mặc định **vẫn đóng**, và ba hướng còn lại — chưa cái nào được đo:

1. **Cổng rẻ trước embedding**: `route_intent` (tầng 0) đã bắt đúng 13/13 ca trong probe với chi phí ~0. Câu nào tầng 0 phân loại được thì không cần cả embedding lẫn LLM. Đây là hướng rẻ nhất và gần nhất với dữ liệu đang có.
2. **Ngưỡng TƯƠNG ĐỐI** thay vì tuyệt đối — chuẩn hoá điểm theo phân bố của chính câu đó (z-score trên toàn catalog) thay vì so với một hằng số.
3. **Đổi model embedding**: dải 0,78–0,91 là đặc tính E5. Một model có dải rộng hơn sẽ làm mọi ngưỡng dễ thở hơn — nhưng đây là thay đổi lớn, đo trước rồi hãy bàn.

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

### U21 — Sổ đo mỗi lượt — `turn_telemetry`

> **Vì sao số hiệu nhảy qua khối F.** U16–U20 đã chiếm dải kế tiếp và các neo `#u16…`–`#u20…` đang được tài liệu khác trỏ tới. Đánh số lại để cho liền mạch sẽ làm hỏng những neo đó — `docs-citations` là cổng cứng. U21 nằm ở **nhóm E**, đọc theo §2 chứ đừng đọc theo số.
>
> **Nguồn gốc.** Rà `github.com/ethanplusai/jarvis` ngày 01/08/2026 (Python/FastAPI, Claude API + Fish Audio, macOS — **không lấy được dòng mã nào**: ngăn xếp ngược hẳn và giấy phép cấm dùng thương mại). Thứ đáng lấy là **ý tưởng bảng `task_log`** trong `learning.py` của họ: ghi loại việc / thành-bại / thời lượng, rồi dùng thống kê đó để quyết định hành vi. Mục này là bản LIVA của ý tưởng đó, **không** phải bản port.

**Vì sao.** [U14](#u14--tự-động-chuyển-router--expert) ghi rõ chỗ tắc: *"`llm:swap_model` chạy được; **quyết định khi nào swap** thì chưa có. Cần một tín hiệu đánh giá độ khó câu hỏi."* Tín hiệu đó không thể bịa ra lúc viết chính sách — nó phải là dữ liệu đã tích luỹ. Hôm nay LIVA **vứt đi** đúng thứ dữ liệu ấy: `handle_chat_completion_scoped` tính xong `prompt_tokens`/`completion_tokens` rồi trả về cho client và không lưu gì (`lib.rs`, nhánh dựng JSON `usage` ở cuối hàm); độ trễ mỗi lượt thì không ai đo. Kết quả: ngày bắt tay làm U14 là ngày **bắt đầu** gom số từ con số không.

Đây là lý do mục này đáng làm **sớm** dù thuộc nhóm E: nó không phải năng lực mới, nó là **hạ tầng đo** cho một năng lực đã lên lịch. Ghi sớm một ngày là có thêm một ngày dữ liệu thật.

⚠️ **Nhưng quy tắc chặn ở §2 vẫn là quy tắc** — *"Chỉ vào nhóm này sau khi A–C xong"*. Mục này là **ứng viên ngoại lệ có lập luận**, không phải ngoại lệ đã được duyệt. Điều kiện để nhận nó khi A còn dở: nghiệm thu 3 (độ trễ) phải **đạt**, vì đó là thứ duy nhất khiến nó có thể làm hỏng một mục nhóm A. Không đạt thì trả về đúng chỗ trong hàng đợi.

**Ba quyết định thiết kế cần chốt trước khi gõ dòng nào.**

**1. Đặt tên `turn_telemetry`, KHÔNG đặt `task_log`.** Bảng `tasks` (`db.rs`) đã tồn tại và là **to-do của người dùng** (`commands/task.rs`, 4 nhánh CRUD). Một bảng tên `task_log` cạnh nó sẽ đọc thành "nhật ký của bảng `tasks`" — sai hoàn toàn. Đây đúng lớp nhầm lẫn mà doc-comment đầu `commands/task.rs` đã phải viết hẳn một đoạn để cảnh báo (`task_plan_chat` không thuộc miền task dù tên bắt đầu bằng `task`). Đừng tạo thêm một ca nữa.

**2. Móc ở `persist_turn_scoped`, không móc ở từng điểm vào.** Hàm đó có **3 điểm gọi thật** — `lib.rs` (`chat:completion`), `websocket.rs` (đường thoại), `agent/graph.rs` (pipeline graph) — cộng một điểm trong test. Móc một chỗ thì cả ba đường vào được phủ và không thể trôi lệch; móc ba chỗ thì lần thêm đường vào thứ tư sẽ thiếu, im lặng. Giá phải trả: **đổi chữ ký hàm** ⇒ **bắt buộc chạy `impact({target: "persist_turn_scoped", direction: "upstream"})` trước khi sửa**, theo `CLAUDE.md`.

**3. KHÔNG chép plaintext vào bảng này.** `events` cố ý **không** nhân bản `rawUserMsg`/`rawAiReply` — doc-comment của `persist_conversation_event_vector` nêu rõ nội dung đã nằm ở `vectors_meta` nên không lặp lại. Một bảng telemetry chứa nguyên văn câu người dùng sẽ phá quyết định đó **và** tạo một bản sao không mã hoá nằm ngoài mọi đường xoá. Chỉ ghi **số và nhãn**; muốn nối về nội dung thì giữ `event_id`.

**Việc.**
1. Thêm `CREATE TABLE IF NOT EXISTS turn_telemetry (...)` vào `init_schemas` (`db.rs`) — cùng khuôn `execute_batch` với các bảng hiện có. Cột tối thiểu: `id`, `event_id`, `ts`, `entry_path` (`chat`/`voice`/`graph`), `model_id`, `prompt_tokens`, `completion_tokens`, `latency_ms`, `outcome` (`ok`/`err`), `err_kind`.
2. `model_id` lấy từ `current_model_path` (`llm/engine.rs`) — **đây là cột làm nên giá trị của bảng cho U14**; thiếu nó thì không trả lời được "tỷ lệ câu đi vào expert".
3. Đo `latency_ms` bao quanh `spawn_blocking` sinh completion trong `handle_chat_completion_scoped`, truyền xuống cùng token count.
4. Ghi qua `persist_turn_scoped` (xem quyết định 2). Lỗi ghi telemetry **không được** làm hỏng lượt trả lời — nuốt vào `tracing::warn!`, đừng `?`.
5. Một lệnh đọc: `telemetry:summary` trả tỷ lệ theo `model_id`, p50/p95 `latency_ms`, tỷ lệ `outcome='err'`. Không có lệnh đọc thì bảng này là một thư mục ghi rồi quên.

**File.** `liva-native-core/src/db.rs` (schema), `liva-native-core/src/agent/graph.rs` (`persist_turn_scoped`), `liva-native-core/src/lib.rs` (`handle_chat_completion_scoped`), `liva-native-core/src/websocket.rs`, `liva-native-core/src/commands/llm.rs` (lệnh đọc).

**Nghiệm thu — bốn điều kiện, cả bốn đều là lệnh chạy được.**

1. **Có dữ liệu thật.** Chạy gateway với `LIVA_DB_PATH` trên đĩa (**không** `LIVA_DB_IN_MEMORY=1` — DB in-memory không mở lại từ ngoài được, đúng lý do `e2e-memory.mjs` từ chối chạy khi thiếu biến này), rồi `e2e-gateway.mjs`. Mở file SQLite và kiểm: `SELECT COUNT(*) FROM turn_telemetry` ≥ số lượt chat của kịch bản, **và mọi dòng** có `model_id <> ''`, `latency_ms > 0`, `completion_tokens > 0`.
2. **Phủ cả ba đường vào.** Sau một lượt qua `chat:completion` **và** một lượt qua đường thoại, `SELECT DISTINCT entry_path` trả về ít nhất `chat` và `voice`. Chỉ thấy `chat` nghĩa là móc sai chỗ — quay lại quyết định 2.
3. **Không làm chậm đường nóng.** `persist_turn_scoped` được `await` **trước** khi `handle_chat_completion_scoped` trả kết quả, nên đây là chi phí người dùng chịu thật, không phải chi phí nền. Đo p50 của 20 lượt trước và sau bằng cùng một lệnh: **chênh lệch p50 < 5 ms**. Ghi **cả hai con số** vào §1 — ghi "không đáng kể" mà không có số là vi phạm [không bịa số](../README.md#không-bịa-số-liệu).
4. **Không hồi quy cổng nào.** `cargo test` (≥ 564 pass), `cargo clippy --all-targets --message-format=short` **0 warning**, `e2e-gateway-ci.mjs` 8/8, `e2e-memory.mjs` 6/6. Riêng `e2e-memory` phải chạy vì mục này đụng đúng hàm ghi lượt hội thoại.

**Chưa xong nếu chỉ có bảng.** Cùng lý lẽ với [U13](#u13--consolidation-ngữ-nghĩa-l2--l3) (*"Chưa chứng minh được điều đó thì L3 chỉ là schema"*): `turn_telemetry` không có `telemetry:summary` đọc ra được số thì nó là schema, không phải sổ đo — và nghiệm thu 1–3 đều không kiểm được.

**Cái mục này KHÔNG làm, để khỏi bị nhầm là trùng lặp.** Nó **không** thay `skill_signals` + `skills/signals.rs` — sổ ghi đó đo **chất lượng skill** để cộng prior vào *thứ hạng truy hồi*, và nó đã tinh vi hơn (đếm `merge_key` phân biệt, trọng số theo loại, `refuted` không trừ điểm). `turn_telemetry` đo **chi phí và kết cục của một lượt LLM** để phục vụ *quyết định chọn model*. Hai miền khác nhau, đừng gộp.

---

### U22 — Hỏi trước khi trả — nhịp truy xuất

> **Nguồn gốc.** Người dùng đưa ngày 05/08/2026: một "giao thức NEO" cho việc học với AI, gồm bốn nhịp — *cam kết* (viết phỏng đoán trước khi thấy đáp án) → *va chạm* (AI phá cái neo sai) → *nén* (một câu LÕI) → *truy xuất* (hỏi lại, không cho đáp án). Mục này lấy **duy nhất nhịp 4**, và cố ý **bỏ nhịp 2**. Lý do bỏ nằm ngay dưới — nó là phần đáng đọc nhất của mục này.

**Vì sao chỉ lấy một nhịp trong bốn.** Sắp bốn nhịp theo *mức phụ thuộc năng lực model* thì thứ tự đảo ngược hẳn so với giá trị biểu kiến:

| Nhịp | Đòi model làm gì | Hỏng thì ra gì |
|---|---|---|
| 2 — va chạm | Phát hiện **đúng** chỗ người dùng sai | Model 2–4B tự tin bảo bạn nhầm trong khi bạn đúng ⇒ **nó vừa thả neo hỏng lên bạn**, đúng cơ chế giao thức mô tả nhưng ngược chiều |
| 4 — truy xuất | Đọc lại đúng câu đã lưu, **không suy luận** | Hỏi sai lúc — phiền, nhưng không cấy niềm tin sai |

LIVA chạy router 2B offline trên máy người khác (beta là 5 người bạn, laptop, model 2–4B). Nhịp 2 là nhịp **duy nhất** trong bốn có chế độ hỏng làm người dùng *tệ đi so với không dùng gì*, và nó rơi đúng vào việc model nhỏ làm kém nhất. Nhịp 4 thì ngược lại: nó là nhịp con người hay bỏ nhất (không có phần thưởng tức thì) — tức đúng chỗ máy làm thay có lãi.

**⚠️ Hạ tầng KHÔNG sẵn như nhìn lần đầu — đây là cái bẫy của mục này.** Bảng `facts` (`db.rs`, `init_schemas`) có sẵn ba cột `memory_strength`, `last_accessed_at`, `access_count` — đúng ba đại lượng một lịch ôn giãn cách cần, không thừa không thiếu. Nhưng đọc kỹ hai đầu thì chúng **không phải** lịch sử truy xuất:

1. `get_fact` **đọc cả ba rồi không ghi lại gì** — không có `access_count = access_count + 1` trên đường đọc. Nhớ lại một fact hôm nay **không để lại dấu vết nào**.
2. `set_fact` có `ON CONFLICT(key) DO UPDATE SET … memory_strength = excluded.memory_strength, last_accessed_at = excluded.last_accessed_at, access_count = excluded.access_count` — tức **ghi đè cả ba từ payload người gọi**. Mà người gọi là lệnh `memory:set_fact` (payload JSON, xem `main_tests.rs`), không phải một bộ lập lịch.

Hệ quả: đặt trạng thái lịch ôn vào ba cột đó thì **lần `set_fact` bình thường kế tiếp trên cùng `key` sẽ xoá sạch nó**, im lặng, không lỗi. Đây đúng lớp hỏng mà [U18](#u18--trí-nhớ-nhìn-thấy-được-ngay-trên-ui) đã cắn (ba database song song — "không lỗi, không log"). Chốt trước khi gõ dòng nào: **hoặc** sửa nhánh `ON CONFLICT` để ba cột này được giữ lại thay vì lấy từ `excluded` (đổi ngữ nghĩa một hàm dùng chung ⇒ bắt buộc `impact({target: "set_fact", direction: "upstream"})`), **hoặc** giữ lịch ôn ở bảng riêng và để `facts` yên. Đừng chọn bằng cảm tính — đo số điểm gọi `set_fact` rồi chọn.

**Ba quyết định thiết kế.**

**1. Không thêm lượt inference nào.** Nhịp 4 là một nhánh **trước** khi gọi LLM: khớp được `key` trong `facts` thì hỏi lại, không khớp thì đi đường cũ. Chi phí 0 token. Đây là điều kiện để mục này không mâu thuẫn với chủ trương sống chung với tải nặng — một mục "học tập" mà ăn thêm một lượt LLM mỗi sự kiện thì phải bị từ chối.

**2. Mặc định TẮT, một biến môi trường, giống `LIVA_MEMORY_RETENTION_DAYS`.** `memory_retention.rs` đã lập tiền lệ đúng: không cấu hình thì runtime **không tự làm gì**. Một trợ lý giọng nói tự ý hỏi bài là thứ người dùng không xin.

**3. Bám vào khoảnh khắc người dùng tự hỏi lại chủ đề đó — KHÔNG hẹn giờ nhắc.** Đây là khác biệt giữa "ôn đúng lúc" và "bị làm phiền". Tác giả giao thức thừa nhận nhịp 4 khô khan với *người tự chạy*; khi **máy** thi hành nó thì khô khan biến thành phiền, nặng hơn một bậc. Không có bộ hẹn giờ nào trong mục này.

**Việc.**
1. Chốt chỗ chứa trạng thái theo cảnh báo ⚠️ ở trên (sửa `ON CONFLICT`, hay bảng riêng).
2. Ghi lại trên **đường đọc**: `get_fact` (hoặc lớp gọi nó) cập nhật `last_accessed_at` + `access_count`. Lỗi ghi **không được** làm hỏng lượt trả lời — nuốt vào `tracing::warn!`, đừng `?`. Cùng luật với nghiệm thu 4 của [U21](#u21--sổ-đo-mỗi-lượt--turn_telemetry).
3. Nhánh hỏi-trước-khi-trả, sau cổng biến môi trường: khớp fact đã có ⇒ hỏi lại một lượt. Nhớ đúng → tăng `memory_strength`, giãn khoảng cách. Không nhớ → trả lời đầy đủ, rút ngắn khoảng cách.
4. Câu hỏi phải đi qua `sanitize_untrusted` (`llm/prompt/persona.rs`) như mọi nội dung do người dùng sinh ra — nội dung fact là dữ liệu, không phải chỉ thị.

**File.** `liva-native-core/src/db.rs` (`set_fact`/`get_fact`), `liva-native-core/src/commands/memory.rs`, `liva-native-core/src/lib.rs` (nhánh trước khi gọi LLM).

**Nghiệm thu — cả ba đều là lệnh chạy được, không cần người xác nhận.**

1. **Truy xuất để lại dấu vết.** Gateway với `LIVA_DB_PATH` trên đĩa (**không** `LIVA_DB_IN_MEMORY=1` — cùng lý do `e2e-memory.mjs` từ chối chạy khi thiếu biến này). Hỏi LIVA một fact đã lưu **hai lần**, rồi `SELECT access_count, last_accessed_at FROM facts WHERE key = …`: `access_count` phải **tăng đúng 2**, `last_accessed_at` phải khác 0.
2. **Trạng thái sống sót một `set_fact` thường.** Sau bước 1, gọi `memory:set_fact` ghi đè **cùng `key`** đó bằng payload không có ba trường lịch ôn, rồi đọc lại: `access_count` **không** về 0. Đây là nghiệm thu ứng với cảnh báo ⚠️ — thiếu nó thì mục coi như chưa làm, vì chế độ hỏng là im lặng.
3. **Tắt là thật sự tắt.** Không đặt biến môi trường ⇒ chạy lại `e2e-gateway.mjs` **8/8** và `e2e-memory.mjs` **6/6**, và **không** lượt nào bị chèn câu hỏi ngược. Kèm `cargo test` và `cargo clippy --all-targets --message-format=short` 0 warning.

**Cái mục này KHÔNG làm.**
- **Không đụng đường thoại/persona.** `llm/prompt/persona.rs` chốt: câu trả lời được TTS đọc lên, *không markdown, không bullet, 1–3 câu*. Sáu mục có tiêu đề in hoa của giao thức gốc là định dạng **cho mắt**; đọc lên bằng TTS là hỏng. Nếu muốn đủ sáu mục thì đó là **chế độ riêng trên kênh văn bản**, và là một mục khác.
- **Không làm nhịp 2 (va chạm).** Xem bảng ở đầu mục. Đây là quyết định có chủ đích, không phải sót.
- **Không dùng `l3_edges` cho "móc nối".** Nghe hợp, nhưng [U13](#u13--consolidation-ngữ-nghĩa-l2--l3) ghi rõ tầng L3 **chưa có writer nào** — xây lên trên nó là xây trên schema rỗng.

**Vị trí trong hàng đợi: cuối nhóm E, không chen ngang.** Khác [U21](#u21--sổ-đo-mỗi-lượt--turn_telemetry), mục này **không** phải hạ tầng cho một mục đã lên lịch — nó là năng lực mới, nên quy tắc chặn ở §2 áp dụng đầy đủ: A/B/C xong đã. Ghi vào đây để khỏi mất ý tưởng, không phải để làm ngay.

---

### U26 — Control tag đọc được giữa câu, không chỉ ở đầu lượt

> **Nguồn gốc.** Cùng đợt rà proj-airi 06/08/2026 với [U24](#u24--hai-lỗi-nối-dây-trên-đường-lip-sync-của-widget). AIRI đang mở issue #1607 cho đúng bài toán này (dẫn biểu cảm/cử chỉ VRM giàu hơn). Lấy **bài toán**, không lấy code — ngăn xếp của họ là Electron + TresJS.

Trước bản vá này Liva chỉ đổi được cảm xúc **một lần, ở đầu câu trả lời**. Bộ đọc tag dừng ngay khi có chữ hiển thị đầu tiên — `avatarControlTags.ts` (*"Once visible text starts, bracketed text is passed through unchanged"*) và bản song sinh phía Rust `liva-native-core/src/tts/avatar_control.rs` (cờ `reading_control_prefix`). Nghĩa là một câu trả lời dài, đổi giọng điệu giữa chừng, vẫn giữ nguyên một biểu cảm từ đầu tới cuối.

**Cái bẫy phải né khi sửa.** Thiết kế "chỉ đọc ở đầu" **không phải sơ suất** — nó tránh nuốt nhầm ngoặc vuông hợp lệ giữa câu, và có test khoá lại: `dau_ngoac_sau_khi_van_ban_bat_dau_duoc_giu_nguyen` (`avatar_control.rs`) khẳng định `"Kết quả [2 + 2] là 4."` phải đi qua nguyên vẹn. Gỡ tiền tố mà không thay bằng gì khác sẽ làm đỏ test đó — đúng như thiết kế.

**Cách sửa giữ được cả hai.** Đọc tag ở **mọi vị trí**, nhưng chỉ nuốt khi nội dung trong ngoặc **khớp chính xác một tag trong danh sách trắng** (`AVATAR_EMOTIONS` ∪ `AVATAR_ACTIONS`). `[2 + 2]` không nằm trong danh sách ⇒ đi qua nguyên vẹn ⇒ test cũ vẫn xanh. Danh sách trắng phải **giống hệt nhau ở hai phía** TS và Rust, nếu không TTS sẽ đọc lên một tag mà UI đã nuốt.

**Nghiệm thu.** Bốn test, hai bên:
1. `"Chào bạn. [happy] Vui quá!"` ⇒ TTS nhận `"Chào bạn.  Vui quá!"`, UI nhận đúng **1** control `happy`.
2. Test cũ `"Kết quả [2 + 2] là 4."` **vẫn xanh, không sửa test**.
3. Tag bị cắt đôi giữa hai chunk stream (`"…[wa" + "ve] xin chào"`) vẫn ghép đúng ở **giữa** câu, không chỉ ở đầu.
4. Chuỗi tag phía Rust và phía TS cho ra **cùng một** văn bản còn lại trên cùng đầu vào — khoá bằng một bảng ca kiểm chung.

---

#### ✅ ĐÃ LÀM — 06/08/2026

**Cách sửa cuối cùng khác đề xuất ở trên một điểm, và điểm đó quan trọng.** Đề xuất ban đầu là "đọc tag ở mọi vị trí theo danh sách trắng". Làm đúng thế thì **test cũ số 2 đỏ**: `nhieu_tag_va_tag_la_o_dau_cau_deu_bi_loai` khẳng định `[dance]` — một tag **lạ** — vẫn bị nuốt khi nó đứng ở đầu. Danh sách trắng thuần sẽ thả `[dance]` cho TTS đọc lên.

⇒ Bản đã thi hành giữ **hai chế độ khác nhau có chủ đích**, và đó là thứ nên đọc kỹ nếu sau này ai định "dọn cho nhất quán":

| | Tiền tố (trước khi có chữ) | Thân (sau khi có chữ) |
|---|---|---|
| Ngoặc **quen** | nuốt, phát control | nuốt, phát control ← **mới** |
| Ngoặc **lạ** | nuốt, im lặng | **giữ nguyên làm văn bản** |
| Khoảng trắng sau tag | bị trim | giữ nguyên |

Lý do bất đối xứng: ở đầu câu trả lời không có ngoặc vuông hợp lệ nào cần bảo vệ, nên nuốt hết là an toàn; giữa câu thì có, nên chỉ nuốt cái mình biết chắc.

**Một chi tiết nữa không có trong đề xuất: chặn nghẽn luồng.** Giữ văn bản lại chờ dấu `]` là cần cho ca tag bị cắt đôi, nhưng nếu giữ vô điều kiện thì `"Kết quả [2 + 2"` sẽ treo TTS tới khi chunk sau tới — hoặc mãi mãi, nếu ngoặc không bao giờ đóng. Bản vá chỉ giữ lại khi phần đã thấy **còn có thể lớn lên thành một tag thật** (`is_viable_tag_prefix` / `isViableTagPrefix`): `[ha` giữ, `[2 + 2` nhả ngay. Hệ quả phụ đẹp: cuối luồng, phần treo **chắc chắn** là tag dở chứ không bao giờ là văn bản, nên `finish()`/`flush()` bỏ nó đi là đóng an toàn chứ không phải mất chữ.

**Đo được:**

```
cargo test -p liva-native-core --lib tts::avatar_control   → 12 passed; 0 failed
npx vitest run tests/utils/avatarControlTags.test.ts       → 14 passed
```

Bốn test Rust cũ và bốn test TS cũ **không sửa một dòng nào**. Bảng ca kiểm chung 8 ca (`bang_ca_kiem_chung_voi_ban_typescript` ↔ `"khớp từng ca với bản lọc phía Rust"`) là thứ chặn hai bản trôi khỏi nhau — sửa một bên mà quên bên kia thì một trong hai đỏ.

**Phía tiêu thụ không phải sửa gì:** `WidgetApp.vue` đã lặp `for (const control of parsed.controls)` trên **mọi** chunk, nên control phát ra giữa câu chạy thẳng tới `setExpression` / `executeAvatarAction`. Nút thắt xưa nay chỉ nằm ở bộ đọc.

---

### U27 — Hợp đồng giao thức + SDK công khai cho cổng 8002

AIRI phát hành `@proj-airi/server-sdk` và một `plugin-protocol` lên npm, nên người ngoài viết được module cho họ. LIVA có **14 cổng** WebSocket đã đo ([§1](#1-đường-cơ-sở-đã-đo--02082026-tại-260c643-thay-bảng-2907)) nhưng không có hợp đồng công bố ⇒ đứng ở vị trí một *ứng dụng*, không phải một *nền tảng*. Nguyên liệu đã có gần đủ: [`02-giao-thuc-ipc-va-websocket.md`](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) là đặc tả, và `scripts/e2e-gateway.mjs` trên thực tế **đã là một client mẫu chạy được**.

**Nghiệm thu.** Một người chưa từng đọc repo, chỉ đọc trang giao thức, viết được một client nói chuyện với gateway trong dưới 30 phút — đo bằng cách đưa cho một người thật, không tự chấm.

---

#### ◐ ĐANG LÀM — 06/08/2026: khoảng trống hẹp hơn mô tả ở trên

**Khảo sát lật ngược một phần tiền đề.** Mục này viết như thể LIVA thiếu hợp đồng giao thức. Không phải — [`02-giao-thuc-ipc-va-websocket.md`](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md) đã là hợp đồng đầy đủ **974 dòng**, tự khai ngay đầu file là "*bất kỳ ai viết client cho LIVA đều phải theo đúng tài liệu này*", có bảng opcode, catalog lệnh, đối chiếu thiết-kế-gốc-vs-as-built và §11 checklist 13 mục. Và `scripts/lib/ws-client.mjs` đã là một client WebSocket **0 dependency** dựng trên `node:net`, đúng chuẩn RFC 6455 (mask phía client, gộp mảnh TCP, tự trả pong).

⇒ Thiếu không phải đặc tả, cũng không phải code. Thiếu **đường đi từ con số không tới một socket đang chạy**: không có ví dụ nào chép về là chạy được, và ba chế độ hỏng đầu tiên đều có triệu chứng gây hiểu nhầm.

**Đã làm:** [`examples/gateway-quickstart.mjs`](../../examples/gateway-quickstart.mjs) — Node thuần, 0 dependency, không import gì từ repo (kể cả `ws-client.mjs`, vì một ví dụ mở đầu phải kéo theo file khác thì không còn là "chép một file rồi chạy"). Kèm [§12 của tài liệu giao thức](../01-ban-ve/02-giao-thuc-ipc-va-websocket.md#12-bắt-đầu-nhanh--client-chạy-được-trong-một-file).

**Đo được — chạy trên gateway thật, không phải mock:**

```
# gateway debug, LIVA_SERVER_PORT=8099 LIVA_DB_IN_MEMORY=1
node examples/gateway-quickstart.mjs   → 3/3 đạt, thoát 0
PORT=8099 node scripts/e2e-gateway.mjs → 8/8 đạt
```

**Ba chế độ hỏng đã đo và ghi vào §12** — cả ba đều là thứ làm người mới mất buổi đầu:

| Triệu chứng | Thực chất | Bằng chứng |
|---|---|---|
| Kết nối mở rồi đóng ngay, y hệt "server chưa chạy" | `Origin` không thuộc allow-list ⇒ **`HTTP/1.1 403 Forbidden`** thay vì 101 | đo 06/08 bằng handshake mang `Origin: http://evil.example.com` |
| Gõ sai tên lệnh nhưng nhận lỗi *phân quyền* | cổng phân quyền chạy **trước** khi phân giải tên ⇒ tên bịa cũng ra `principal WebSocketRemote is not authorized for command '…'` | output thật của cả quickstart lẫn `e2e-gateway.mjs` |
| `mcp:*` và `vision:ask` bị từ chối | **đúng thiết kế** — socket này mang principal `WebSocketRemote`, không toàn quyền. Toàn quyền nằm ở đường IPC stdin | `e2e-gateway.mjs` khẳng định đúng việc chặn |

**Còn lại — và tôi KHÔNG tự nghiệm thu được.** Điều kiện nghiệm thu của mục này cố ý viết là "*đưa cho một người thật, không tự chấm*". Tôi chứng minh được ví dụ **chạy đúng**; tôi không chứng minh được nó **đủ để người lạ tự viết client trong 30 phút** — đó là phép đo trên người, phải do chủ dự án thực hiện. Chưa làm nữa: gói SDK phát hành được (npm) và bản tiếng Anh; cả hai đều là quyết định phát-ra-ngoài, không phải việc của một phiên làm việc.

---

### U28 — Endpoint tương thích OpenAI trên gateway

Ý lấy từ `unspeech` của AIRI (proxy ASR/TTS dùng chung một dạng API). Mở `/v1/chat/completions`, `/v1/audio/speech`, `/v1/audio/transcriptions` ngay trên lõi Rust thì mọi công cụ sẵn có gọi được LIVA mà không cần biết gì về giao thức riêng — và quan trọng hơn cho hồ sơ: **VieNeu-TTS tiếng Việt trở thành dịch vụ mà máy khác trong nhà dùng được**, offline.

**Nghiệm thu.** `curl` đúng payload OpenAI vào `/v1/audio/speech` trả về WAV phát được; và một client OpenAI SDK bất kỳ (chưa sửa dòng nào) chat được với LIVA qua `base_url` trỏ vào cổng 8002.

**Cảnh báo phạm vi.** Chỉ làm phần *hình dạng API*. Không kéo theo xác thực, hạn mức, hay đa phiên — đó là dự án khác.

---

#### ✅ ĐÃ LÀM — 06/08/2026

**File:** `liva-native-core/src/openai_api.rs`, nối vào `boot.rs` mục 5b. Bật bằng `LIVA_OPENAI_PORT`; **không đặt biến thì không mở socket nào**.

**Ba quyết định đáng ghi lại, vì mỗi cái đều là một ngã rẽ.**

**1. Dùng `hyper`, không `axum` — và điều đó tốn 0 crate mới.** Lõi không có framework HTTP nào: cổng 8002 là `TcpListener` thô đưa thẳng kết nối cho `tokio-tungstenite`. Nhưng `hyper 0.14` **đã nằm sẵn trong cây** qua `reqwest`, nên bật thêm feature `server`/`http1`/`tcp`/`stream` không kéo về gì — `httparse`, `http`, `http-body`, `tower-service`, `want`, `h2` đều đã có trong `Cargo.lock`. Kiểm bằng cách diff danh sách crate trước/sau: **rỗng**. `axum` thì là một cây mới hoàn toàn, cho một crate chỉ cần định tuyến ba đường dẫn.

**2. Cổng RIÊNG, không phải 8002 — lệch với điều kiện nghiệm thu viết ở trên, có chủ đích.** Mục này viết "`base_url` trỏ vào cổng 8002". Làm đúng thế đòi hỏi soi trước vài byte của mỗi kết nối TCP rồi *phát lại* chúng cho `tokio-tungstenite` — một chỗ dễ sai nằm ngay trên đường thoại đang chạy tốt, đổi lấy đúng một thứ: trùng số cổng. Ý định thật của điều kiện là "SDK OpenAI gọi được LIVA", và điều đó đã đạt.

**3. Mặc định TẮT.** Bề mặt này không có xác thực, và **kém an toàn hơn cổng 8002** ở một điểm cụ thể: nó không có cả hàng rào `Origin`, vì client HTTP không gửi header đó. Bật-mặc-định là mở thêm một cửa không khoá mà người dùng không yêu cầu.

**Một lỗi chỉ lộ ra khi chạy thật.** Lượt đo đầu tiên trả về:

```json
{"message":{"content":"[happy][wave] Xin chào bạn nhé.","role":"assistant"}}
```

Tag điều khiển avatar **rò thẳng ra API**. Đường thoại và giao diện đều lọc chúng; endpoint mới thì chưa — và một công cụ bên ngoài không có cách nào biết `[happy]` là chỉ thị chứ không phải chữ LIVA muốn nói. Đã vá bằng chính `AvatarSpeechFilter` của [U26](#u26--control-tag-đọc-được-giữa-câu-không-chỉ-ở-đầu-lượt), ở **cả hai** đường. Ở đường stream, bộ lọc phải sống qua cả vòng lặp chứ không dựng lại mỗi token — một tag có thể bị cắt đôi giữa hai token, và bộ lọc mới mỗi mẩu sẽ không bao giờ ghép được hai nửa. Có test hồi quy.

**Nghiệm thu — SDK OpenAI chính chủ v7.4.0, không sửa dòng nào, chỉ đổi `baseURL`:**

```
✅ client.models.list() — liva-local
✅ client.chat.completions.create() — "Thủ đô của Việt Nam là Hà Nội."
✅ không rò tag điều khiển avatar
✅ có usage token — 577 token
✅ stream: true (SSE) — "Một, hai, ba nhé."
✅ client.audio.speech.create() — 169004 byte, RIFF

6/6 đạt
```

`curl` trực tiếp cũng đạt: `/v1/audio/speech` trả **HTTP 200 · `audio/wav` · 176 684 byte**, header hợp lệ (mono, 48 kHz, 16-bit, 1,84 giây), và cỡ ghi trong header khớp cỡ file thật. Luồng SSE có đủ 13 mẩu, kết bằng `finish_reason:"stop"` rồi `data: [DONE]`.

**Tái lập được mà không cần cài gì:** `examples/openai-api-check.mjs` — `node:http` thuần, 0 dependency, **11/11 đạt**. Repo không phải gánh gói `openai` chỉ để chạy một bộ kiểm; bản đo bằng SDK chính chủ ở trên đã chạy riêng một lần và ghi lại ở đây.

```bash
# lõi: LIVA_SERVER_PORT=8099 LIVA_OPENAI_PORT=8003 LIVA_DB_IN_MEMORY=1 LIVA_TTS_VIENEU=1
node examples/openai-api-check.mjs   → 11/11 đạt
```

⚠️ **Bẫy đo, trả giá ngay trong phiên: cổng mở TRƯỚC khi model nạp xong.** Lần chạy đầu của bộ kiểm được **5/11** với `"No model loaded"`, rồi lần sau **quá hạn 60 giây**. Cả hai đều không phải lỗi của bề mặt API:

- Socket HTTP mở ở khoảng giây thứ 20 của boot; router LLM nạp bất đồng bộ sau đó.
- Và request đầu tiên **không trả lỗi rồi thôi — nó CHẶN** sau khoá của engine tới khi nạp xong. Đo được: **95 giây** với Qwen3-VL-2B trên máy dev. Nên "chờ tới khi cổng mở" là điều kiện sai; hạn 60 giây mỗi lượt cũng sai.

Bộ kiểm nay tự chờ model (hạn 240 s mỗi lượt, ngân sách chung 420 s) và **coi quá hạn là "vẫn đang nạp"** chứ không phải trượt. Ai viết bộ kiểm khác cho bề mặt này nên đọc lại đoạn `doiModel()` trước, kẻo mất buổi đi tìm lỗi ở đúng chỗ không có lỗi.

**Cổng đã chạy lại sau khi đụng `Cargo.toml`:** `cargo test` **553 passed** (548 + 5 test mới) · clippy **0** · `cargo fmt` sạch · `cargo audit` **22 cảnh báo allowed, 0 lỗ hổng** — đúng con số đường cơ sở, không phát sinh gì từ `hyper`.

**Chưa làm, nêu rõ:**
- **`/v1/audio/transcriptions`** — cần phân tích `multipart/form-data` thủ công, đủ lớn để thành mục riêng.
- **Trường `model` không có tác dụng.** LIVA luôn dùng model router đang nạp; giá trị gửi lên được phản chiếu nguyên văn vào hồi âm cho khớp kỳ vọng SDK. Muốn đổi model thì vẫn là `llm:swap_model` ở cổng 8002.
- **Đây là LIVA, không phải proxy LLM trung tính.** Persona được chèn khi request không có message `system`, có truy hồi RAG, có ghi lượt vào bộ nhớ (scope riêng `openai_api`, không trộn vào hội thoại cục bộ). Ai muốn model trần thì tự gửi message `system`.

---

### U29 — Vòng lặp chủ động có ngân sách tick

Trụ "chủ động" vẫn là mảnh thiếu lớn nhất trong ba trụ. AIRI có ba repo chơi game (`game-playing-ai-balatro`, `-dome-keeper`, `-playground-2d`) và tất cả đều dùng **cùng một khuôn**: `perception → state → decision → action`, chạy theo tick có ngân sách. LIVA đã có sẵn ba trong bốn khâu — passive vision, governor, và Qwen3-VL làm bộ quyết định. Thiếu đúng cái **vòng lặp** khép chúng lại, cộng một đường xuất hành động.

**Nghiệm thu.** LIVA tự mở lời **đúng một lần** trong một tình huống định trước (ví dụ: phát hiện một lỗi build trên màn hình) mà không ai gõ gì; và governor cắt được vòng lặp đó khi tải GPU vượt ngưỡng — chứng minh bằng log, không bằng mô tả.

**Ước lượng: cao, nhiều tuần.** Ghi ra đây để có chỗ neo, không phải để nhận trong một phiên.

---

### U32 — Retarget đang vứt đi phần lớn chuyển động

`CONTROLLED_BONES` trong `useAvatarAnimation.ts` giữ đúng **11** xương — hai chân, hai tay, hips. Bị bỏ: toàn bộ **spine, shoulder, hand, finger**, và **toàn bộ hips position**.

**Đo được (07/08/2026):** mỗi FBX Mixamo trong `liva-ui/public/animations/mixamo/` chứa **68 tên xương `mixamorig:` duy nhất** — đếm bằng `strings` trên `walk.fbx`, `run.fbx`, `idle.fbx`, cả ba đều ra 68. Trong đó có vài mục không phải xương động (`HeadTop_End`, và một `Hipsf` trông như rác), nên số track hoạt hình thật thấp hơn 68 nhưng vẫn **hơn 11 rất nhiều**. *(Bản rà gốc ghi "52 track quaternion + 1 track vị trí hips"; con số đó **chưa tự kiểm** — muốn chính xác phải nạp FBX qua three.js và đếm `clip.tracks`. Kết luận không đổi dù con số đúng là 52 hay khác.)*

Hệ quả: dáng đi không có chuyển động thân trên, tay không đánh theo vai, và vì hips position bị bỏ nên không có thành phần nhấp nhô/đưa ngang tự nhiên của bước chân — thứ mà [U30](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước) đang cố bù lại bằng một cơ chế sai.

Thêm một khoản riêng: khâu **trộn giữa hai tư thế** dùng lerp trên góc Euler (`blendPose`). Lerp Euler có thể sinh pop khi một góc đi qua biên ±π. *(Đây là cơ chế có thật nhưng **chưa kiểm chứng là có xảy ra với sáu clip hiện có** — chu kỳ đi bộ hiếm khi vung chi qua ±π. Đo trước khi sửa.)* Bên trong mỗi clip thì đã slerp quaternion đúng cách rồi.

**Hai hướng, chọn một:**

- **Giữ pipeline tự viết** nhưng mở rộng danh sách bone và giữ pose ở dạng quaternion, trộn bằng slerp.
- **Chuyển clip đã retarget thành `THREE.AnimationClip`** rồi dùng `AnimationMixer` — được `crossFadeTo`, `syncWith`, time warping sẵn có, và bỏ được cả `blendPose` lẫn `sampleTrack` tự viết.

**Nghiệm thu.** Số track giữ lại tăng từ 11 lên đủ chuỗi spine + shoulder + hand; một đoạn quay đặt cạnh bản cũ cho thấy thân trên có chuyển động; và `npm run test:coverage -w liva-ui` không tụt ngưỡng nào.

---

### U33 — Locomotion đúng: nhịp cố định, blendspace, distance matching

**Điều kiện vào: [U30](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước) đã xong và đã xác nhận hết khựng.** Đây là **viết lại hệ locomotion**, nhiều tuần — không phải bước tiếp theo tất yếu của việc sửa giật. Ghi ở đây để có chỗ neo, không phải để nhận ngay.

- **Nhịp mô phỏng cố định 60 Hz** với accumulator + nội suy lúc render, thay cho variable delta chặn ở `0.1 s`. Giữ chuyển động ổn định khi frame time dao động.
- **Tốc độ đã làm mượt** bằng bộ điều khiển critically-damped / giới hạn jerk, thay cho `motionWeight` thô.
- **Blendspace `idle ↔ walk ↔ run`** theo tốc độ, dùng chung một gait phase.
- **Phase theo quãng đường, không theo thời gian**: `phase += distance / strideLength`. Đây là thứ khử trượt chân tận gốc, thay vì bù sau bằng IK. Hằng số `STRIDE_HZ` hiện tại (`walk 1.05`, `run 1.9`) là nhịp theo *thời gian* — đúng loại cần thay.
- **Hiệu chỉnh authored stride speed** của từng clip và giới hạn playback rate.
- **Contact curve trái/phải** gắn vào từng clip; khi contact active thì giải two-bone IK cho đùi–gối–bàn chân, pelvis chỉ bù nhẹ theo trục đứng.

**Nghiệm thu.** Route 20 giây `idle → walk → run → stop`, chạy cả khi đang chat lẫn khi rảnh: p95 frame interval **< 20 ms**, p99 **< 33 ms**; chân trụ trượt **< 2 px/frame**; **không có bước nhảy pelvis** khi đổi chân; và **vị trí kết thúc tương đương** ở ba lịch frame 30/60/120 Hz cũng như khi chèn một frame spike — đây là phép kiểm chứng minh nhịp cố định thật sự hoạt động.

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

### U23 — Màn Kỹ năng đang báo 1 trong khi lõi có 7

**Phát hiện 05/08/2026** khi người dùng mở Dashboard → *Quản lý Kỹ năng* và thấy đúng **một** thẻ: `smart_home_control`, dưới nhãn "HỆ THỐNG CỐT LÕI".

**Vì sao đây không phải "thiếu tính năng".** Màn hình không đếm gì cả — nó đọc một hằng số:

```rust
// liva-native-core/src/commands/config.rs
"get_skills_list" => Ok(json!([integrations::smart_home::get_metadata()])),
```

Mảng một phần tử, viết cứng trong mã. `SkillsView.vue` gửi `get_skills_list` rồi render đúng cái nó nhận. Con số 1 vì thế **không đo gì**, và mọi nút trên màn đó — *Kiểm tra tất cả* · *Bật tất cả* · *Tắt tất cả* — chỉ thao tác trên một mục.

**Lõi thật sự có 7 tool**, tất cả đã nghiệm thu và chạy được (`mcp/server.rs`):

| Tool | Trạng thái |
|---|---|
| `read_markdown` · `write_markdown` · `search_vault` | vault Obsidian, có e2e |
| `control_smarthome` | **duy nhất lên được màn hình** |
| `control_volume` · `control_media` | chạm máy thật qua `SendInput` — [U19](#u19--ba-tool-os-thật) nghiệm thu **10/10** |
| `get_weather` | |

Và một hệ **thứ hai** nữa: kho skill có schema riêng, bảng SQLite, phiên bản hoá, tín hiệu chất lượng — 7 lệnh `skills:sync` / `skills:list` / `skills:search` / `skills:signal` / `skills:signals` / `skills:history` / `skills:pin_ids` (`commands/skill_store.rs`). Grep toàn bộ `liva-ui/src` cho `skills:` → **0 kết quả**: chưa màn hình nào gọi tới. Kho cũng đang rỗng — `skills_root()` trỏ `skills/` (đổi bằng `LIVA_SKILLS_DIR`), thư mục đó không tồn tại.

⇒ Ba lớp chồng nhau, không phải một bug: (1) `get_skills_list` là **di sản thời Node.js** — tên `get_`/`toggle_` không có `namespace:`, có từ hồi chỉ tồn tại mỗi smart_home; (2) 7 tool MCP không có đường ra UI nào; (3) kho skill thật chưa nối dây và chưa có nội dung.

**Vì sao xếp vào nhóm F chứ không phải nhóm E.** Nó **không thêm năng lực nào** — năng lực đã có, đã đo, đã nghiệm thu. Nó chỉ bật đèn cho thứ đang tắt, đúng định nghĩa của nhóm này. Đây cũng là ca "vàng đang tắt điện" rõ nhất còn lại sau [U17a](#u17a--bộ-chọn-giọng--xong-26072026) (10 giọng đã tải về mà vô hình).

⚠️ **Nhưng nó nặng hơn một mục nhóm F thông thường, vì màn hình đang NÓI DỐI.** Dự án có một nguyên tắc đã ghim — *"instruments that admit ignorance"*: `sysinfo.rs` trả `null` và UI hiện `--` thay vì bịa một con số dễ chịu. Một màn hình hardcode `1` vi phạm thẳng nguyên tắc đó, và nó nằm trên đường đi của beta tester lẫn giám khảo. Cùng họ với `cpuUsage: 12` mà `sysinfo.rs` sinh ra để dẹp.

📌 **Kéo theo một chỗ nữa, đừng sửa một nửa.** `system_status.rs` lấy độ dài từ **chính mảng này** (có comment nói rõ, cố ý để hai lệnh không lệch nhau), nên màn *Hệ thống* cũng đang báo 1. Hai chỗ nhất quán với nhau và cùng sai — sửa `get_skills_list` mà quên chỗ kia thì tạo ra lệch số giữa hai màn.

**Việc.**
1. `get_skills_list` trả **danh mục thật**: gộp tool MCP (`NativeMcpServer::list_tools()`) với `SkillStore::list()`. **Giữ nguyên hình dạng JSON** để `SkillsView.vue` không phải đổi — đây là điều khiến mục này rẻ.
2. Trạng thái bật/tắt phải **thật**: `toggle_skill` hiện đổi cái gì? Nếu không có nơi lưu, thà hiện tất cả là "đang bật" và **bỏ hẳn công tắc** còn hơn một công tắc bấm xong không đổi gì.
3. Kiểm lại `system_status.rs` cùng lượt (xem ghi chú trên).
4. Không đụng `integrations:list` trong cùng lát — nó là miền khác và đang có nợ riêng.

**File.** `liva-native-core/src/commands/config.rs`, `liva-native-core/src/system_status.rs`, `liva-native-core/src/mcp/server.rs` (chỉ đọc), `liva-native-core/src/commands/skill_store.rs` (chỉ đọc).

---

#### ✅ ĐÃ LÀM — 07/08/2026 tại `dec1c14`

**Cách sửa đi vào gốc chứ không vá số.** Thêm `NativeMcpServer::list_skills()` — nó ánh xạ `list_tools()` sang đúng hình dạng JSON 5 khoá mà UI trông đợi. Rồi **cả hai** lệnh cùng gọi nó:

| | Trước | Sau |
|---|---|---|
| `commands/config.rs` | `json!([smart_home::get_metadata()])` | `json!(state.mcp_server.list_skills())` |
| `system_status.rs` | đếm độ dài của **chính mảng literal đó** | `state.mcp_server.list_skills().len()` |

⇒ Hai màn **không thể lệch nhau nữa về mặt cấu trúc**, chứ không phải "đã kiểm thấy khớp". Đó là điểm khác nhau giữa sửa gốc và vá số.

`SkillsView.vue` **không phải sửa một dòng nào** — hình dạng JSON giữ nguyên, đúng như mục này dự tính.

**Nghiệm thu — 3 test trong `tests/verify_commands.rs`:** (a) `get_skills_list` trả **≥ 7** phần tử; (b) độ dài của nó **bằng** `skillsLoaded` trong `system_status` — đây mới là test đáng giá, nó khoá đúng chế độ hỏng cũ; (c) mỗi phần tử đủ 5 khoá `name`/`category`/`short_desc`/`description`/`parameters`.

**Chưa làm, đúng phạm vi đã chốt:** kho skill SQLite (`commands/skill_store.rs`) vẫn chưa nối — kho đang rỗng và 7 lệnh `skills:*` vẫn chưa màn hình nào gọi. Công tắc bật/tắt cũng chưa đụng tới.

**⚠️ Bắt buộc trước khi sửa:** `impact({target: "get_skills_list", direction: "upstream"})` — nó nằm trong allow-list phân quyền (`authorization.rs:58`) và có mặt ở cả hai đường vào (`websocket.rs:1042` và Tauri IPC), nên bán kính ảnh hưởng chạm cả hai profile chạy.

**Nghiệm thu — ba điều kiện, cả ba đếm được.**

| | |
|---|---|
| 1 | Qua socket thật: `get_skills_list` trả **≥ 7** phần tử, và tập tên **khớp đúng** `mcp:list_tools`. Lệch một tên là sai — hai lệnh phải kể cùng một câu chuyện |
| 2 | Màn *Kỹ năng* và màn *Hệ thống* hiện **cùng một con số**. Đây là điều kiện bắt lỗi sửa-một-nửa |
| 3 | Test vitest cho `SkillsView.vue` khoá lại: danh sách 7 phần tử render đủ 7 thẻ. **Viết vitest, đừng cố xem bằng Browser pane** — xem bẫy đã cắn hai lần ở [U18](#u18--trí-nhớ-nhìn-thấy-được-ngay-trên-ui) và [U20](#u20--bộ-nhớ-thị-giác-offline-tuỳ-chọn-đắt-có-mìn) |

**Chưa xong nếu chỉ đổi con số.** Cùng lý lẽ với [U13](#u13--consolidation-ngữ-nghĩa-l2--l3) và [U21](#u21--sổ-đo-mỗi-lượt--turn_telemetry): một màn hình liệt kê 7 tool mà công tắc không điều khiển được gì thì vẫn là màn hình nói dối, chỉ dối ở quy mô lớn hơn.

---

## 9. Cái KHÔNG nên làm

Mục này tồn tại để phiên sau không đốt thời gian vào việc trông có vẻ hữu ích.

- **Đừng viết thêm tài liệu mới.** Bộ tài liệu đã ~1,1 MB với 20 file lỗi thời. Thêm file làm tỷ lệ lỗi thời **tệ hơn**, không tốt hơn. Sửa file có sẵn; tài liệu này đã là ngoại lệ cuối cùng nên có.
- **Đừng dọn `unwrap()` trong khối `#[cfg(test)]`** (332 điểm). Trong test, `unwrap()` panic **chính là** cơ chế báo lỗi. Đụng vào là làm test yếu đi trong khi trông như đang cải thiện.
- **Đừng hạ ngưỡng coverage** trong `vitest.config.ts` để cổng xanh.
- **Đừng bỏ `DEFAULT_ENCRYPTION_KEY`** (`crypto.rs:16`) mà chưa có đường di trú dữ liệu. Nó cảnh báo lớn nhưng không chặn boot — đó là **quyết định có chủ đích**, đã ghim, không phải sơ suất.
- **Đừng tin số ở §1 nếu ngày đã cũ.** Chạy lại lệnh. Đây là điều kiện tiên quyết, không phải lời khuyên.
- **Đừng gộp nhiều mục U vào một lần sửa.** Mỗi mục có nghiệm thu riêng; gộp lại thì không biết cái nào hỏng khi cổng đỏ.
- **Đừng dựng bộ đo trước khi thử tắt cái đang bị nghi.** Nếu nghi can có công tắc một dòng, bật/tắt nó rồi nhìn là phép thử rẻ nhất và quyết định nhất. Bộ đo dùng để **định lượng** một nguyên nhân đã xác định, hoặc để tìm nguyên nhân khi *không* có nghi can — không phải để chứng minh thứ mắt đã thấy. Đây là chỗ bản rà locomotion 07/08/2026 xếp sai thứ tự (xem [U30](#u30--bù-ngang-ở-pelvis-sóng-răng-cưa-đồng-bộ-với-nhịp-bước)).
- **Đừng làm Motion Matching hay PFNN cho locomotion.** Motion matching chỉ phát huy khi có cơ sở dữ liệu lớn gồm start/stop/turn/pivot và nhiều quỹ đạo. Dự án hiện có **đúng sáu clip** (`idle`, `walk`, `run`, `jump`, `wave`, `thinking` — đếm trong `liva-ui/public/animations/mixamo/` ngày 07/08/2026); chi phí vượt xa lợi ích, và [U33](#u33--locomotion-đúng-nhịp-cố-định-blendspace-distance-matching) giải quyết được cùng vấn đề với công sức nhỏ hơn một bậc.
- **Đừng tiết kiệm tài nguyên bằng cách hạ frame rate của avatar đang nhìn thấy.** Hạ DPR, tắt antialias, giảm tần suất spring bone — những thứ đó người dùng khó nhận ra. 5 FPS thì nhận ra ngay, và ECO Mode tồn tại để chạy đúng lúc người dùng vẫn đang nhìn (xem [U31](#u31--ba-khoản-phí-mỗi-frame-trên-đường-avatar) khoản 4).

### Bốn thứ KHÔNG lấy từ proj-airi — chốt 06/08/2026

Rà xong `github.com/proj-airi` + `moeru-ai/airi` (MIT, lấy code được về mặt pháp lý). Bốn thứ dưới đây **có vẻ đáng lấy nhưng không**, ghi ra để phiên sau khỏi rà lại:

- **DuckDB-WASM / pglite làm bộ nhớ trong trình duyệt.** AIRI cần vì lõi của họ chạy trong trình duyệt. LIVA có SQLite trong lõi Rust — hợp với luận điểm offline hơn, và đổi sang là đi lùi.
- **Electron.** LIVA đã có Tauri v2. Không bàn lại.
- **Multi-provider TTS cloud** (ElevenLabs / Azure / Alibaba). Ngược thẳng luận điểm "chạy offline trên máy người dùng".
- **Mở rộng nhánh Live2D cho ngang bằng VRM.** Vừa chốt VRM xong (`Liva.vrm` + bộ retarget Mixamo); nuôi hai nhánh render lúc này là tự tạo nợ, không phải thêm năng lực.

Và một thứ **không phải "không nên làm"** nhưng cần người quyết định, không phải agent: xin đưa LIVA vào danh sách [`awesome-ai-vtubers`](https://github.com/proj-airi/awesome-ai-vtubers) (475 sao). Chi phí gần bằng 0, và với hồ sơ dự thi thì một mục do bên thứ ba liệt kê là **bằng chứng**, khác hẳn tự tuyên bố. Đây là hành động phát ra ngoài repo ⇒ chủ dự án tự làm.

---

## Liên quan

- [01-doi-chieu-tuyen-bo-vs-thuc-te.md](01-doi-chieu-tuyen-bo-vs-thuc-te.md) — tuyên bố vs bằng chứng `file:dòng` (**cần U4**)
- [02-no-ky-thuat-va-rui-ro.md](02-no-ky-thuat-va-rui-ro.md) — rủi ro xếp hạng và code mồ côi
- [03-lo-trinh-sua-loi-va-nang-cap.md](03-lo-trinh-sua-loi-va-nang-cap.md) — lộ trình sửa lỗi GĐ0–GĐ4 (tài liệu này nối tiếp nó)
- [../02-van-hanh/04-kiem-thu-va-ci.md](../02-van-hanh/04-kiem-thu-va-ci.md) — bề mặt kiểm thử và CI
- [../02-van-hanh/03-trien-khai-va-runtime.md](../02-van-hanh/03-trien-khai-va-runtime.md) — cách chạy đúng (**cần U2**)
- [../01-ban-ve/01-kien-truc-tong-the.md](../01-ban-ve/01-kien-truc-tong-the.md) — hai profile chạy (**cần U8**)
- [../README.md](../README.md) — mục lục và quy ước tài liệu
