# Kế hoạch xử lý — rà soát ngày 18/08/2026

> **Phạm vi.** Rà soát trạng thái cây làm việc tại nhánh `test/perf-threshold-baseline`, HEAD `67eb98b`,
> với **65 mục đang treo** (27 `M` + 38 `??`). Mọi con số dưới đây do **chạy thật** trong phiên này,
> không trích từ tài liệu. Lệnh tái lập đi kèm từng mục.
>
> **Tài liệu này không thay thế** [`docs/03-danh-gia/05-nang-cap-toan-dien.md`](docs/03-danh-gia/05-nang-cap-toan-dien.md).
> Backlog U1–U33 vẫn là nguồn việc chính; đây là danh sách **chặn phát hành** phát sinh từ đợt việc
> đang treo, phải đóng trước khi quay lại backlog.

---

## 0. Cảnh báo vận hành — đọc trước khi gõ phím

**Có 2 phiên Claude Code khác đang chạy song song trên chính cây làm việc này.** Đo lúc 13:59–14:12:

- `Get-CimInstance Win32_Process` → **3** tiến trình `claude-code`, một trong số đó là phiên này.
- `cargo test --workspace` đang chạy từ phiên khác (khoá `target/`, nên mọi lệnh `cargo` mới sẽ chờ).
- `liva-native-core/src/db.rs` và `src/llm/engine.rs` bị **sửa lúc 13:58:59 và 13:59:15** — tức *trong lúc*
  phiên này đang rà soát. `db.rs` không có trong ảnh chụp `git status` đầu phiên nhưng có ở lần chụp sau.

⇒ **Hệ quả cho người thi hành kế hoạch này:** đây đúng là cái bẫy đã ghi ở §0.1 của backlog
("Bẫy đo khi cây làm việc đang bị sửa song song — trả giá 06/08/2026"). Trước khi kết luận bất kỳ phép đo
nào là "đỏ ổn định", chạy `ls -l --time-style=+%H:%M:%S <file>` để chắc file không vừa đổi.
**Việc đầu tiên là dừng hoặc đồng bộ các phiên song song**, nếu không mục 1 và 2 dưới đây sẽ bị ghi đè.

---

## 1. Trạng thái cổng CI — đo tại 18/08/2026

CI (`.github/workflows/test.yml`) có **25 bước, mỗi bước là một cổng**. Bảng dưới là kết quả chạy tay
từng cổng chạy được (không tính các cổng cần build Rust đầy đủ, vì `target/` đang bị phiên khác khoá).

| # | Cổng | Lệnh | Kết quả | Ghi chú |
|---|------|------|:-------:|---------|
| 1 | docs-check | `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` | 🔴 **exit 1** | 2 lỗi — mục **P0-2** |
| 2 | docs-citations | `node scripts/docs-citations.mjs` | 🟢 exit 0 | 63 tài liệu, 1321 trích dẫn, 0 neo hỏng |
| 4 | devkit:lint | `npm run devkit:lint` | 🟢 exit 0 | "All checks passed" |
| 5 | npm audit | `npm audit --audit-level=high` | 🟢 exit 0 | **0 vulnerabilities** (14/08 từng 5 findings — nay sạch) |
| 6 | cargo fmt | `cargo fmt --all -- --check` | 🔴 **exit 1** | 3 chỗ — mục **P0-1** |
| 10 | typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` | 🟢 exit 0 | |
| 11 | eslint | `npx eslint . --max-warnings 0` | 🟢 exit 0 | |
| 12 | vitest + coverage | `npm run test:coverage -w liva-ui` | 🟢 exit 0 | ngưỡng coverage vẫn đạt |
| — | skills audit | `npm run skills:audit` | 🟢 exit 0 | 52 SKILL.md, 58 ghi chép vault, 0 lỗi 0 cảnh báo |

**Chưa đo được trong phiên này** (vì `target/` bị `cargo test --workspace` của phiên khác giữ khoá):
cargo-deny, `cargo test`, `e2e-gateway-ci.mjs`, `cargo test -p liva-desktop`, `cargo check`, clippy.
Chúng nằm trong nghiệm thu của mục **P0-1** bên dưới.

> ⚠️ **docs-check đỏ ở bước 1/25 nghĩa là 24 bước còn lại KHÔNG chạy.** CI hiện tại sẽ đỏ ngay
> và không nói gì được về chất lượng mã nguồn. Đây là lý do P0-2 phải đóng cùng lúc với P0-1.

---

## 2. Vấn đề phát hiện — xếp theo mức chặn

### P0-1 · `cargo fmt --all -- --check` đỏ 3 chỗ · **CI đỏ bước 6/25**

Đo: `cargo fmt --all -- --check` → exit 1.

| File | Dòng | Nội dung |
|---|---|---|
| `liva-native-core/src/db.rs` | 140 | `path.file_name().map(...).unwrap_or_default()` cần xuống dòng |
| `liva-native-core/src/db.rs` | 147 | như trên |
| `liva-native-core/src/llm/engine.rs` | 226 | `super::prompt::GEMMA4_MARKERS.store(...)` cần xuống dòng |

Cả 3 nằm trong **mã đang sửa dở** (`db.rs`, `engine.rs` đều ở trạng thái `M`). Đây là lần **thứ năm**
fmt hỏng trong lịch sử gần đây — `6a31371` đã thêm chặn ở pre-commit chính vì việc này, nghĩa là
hook **không bắt được** đường đi hiện tại (file chưa `git add` thì hook không thấy).

**Nghiệm thu.** `cargo fmt --all -- --check` exit 0 **và** giải thích được vì sao pre-commit hook không chặn.

---

### P0-2 · `docs-check --strict-stale` đỏ 2 lỗi · **CI đỏ bước 1/25, 24 bước sau không chạy**

```
✗ docs/03-danh-gia/LIVA_4TIER_MEMORY_ARCHITECTURE_AUDIT_AND_BLUEPRINT.md: LỖI THỜI — 1 file trong `covers` đã đổi kể từ f35961c
✗ docs/03-danh-gia/LIVA_SYSTEM_AUDIT_AND_ROADMAP_2026.md:                 LỖI THỜI — 1 file trong `covers` đã đổi kể từ f35961c
```

Cả hai là file **`??` (chưa track)**, `covers: liva-native-core/src/db.rs` — mà `db.rs` vừa bị sửa (xem §0).
Đây là đúng loại lỗi mà `07-gỡ hai tài liệu audit mới khỏi tracking — commit nhầm ở d4ed541` đã xử lý một lần rồi.

Hai bản sao khác của cùng nội dung nằm ở `docs/architecture/` (`4_tier_memory_deep_audit_report.md`,
`4_tier_memory_optimization_blueprint.md`) — cũng `??`, cũng `commit: f35961c`, cũng LỖI THỜI nhưng chỉ ở mức
cảnh báo vì ngoài `docs/03-danh-gia/`. **Có 4 tài liệu cho cùng một nội dung audit.**

**Nghiệm thu.** `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` exit 0, **và** không còn hai
bản sao cùng nội dung ở hai thư mục. Nhắc lại luật ở §0.2 backlog: `commit:` và `stale-ok:` **không thay
thế nhau** — chỉ đặt `stale-ok: 67eb98b` nếu thật sự đã đọc diff `db.rs` và thấy không cần sửa nội dung.

---

### P0-3 · `PROJECT.md` và `TEST_INFRA.md` đang bị viết lại để **xoá phần thu hồi** — tái phạm lần 3

Đây là vấn đề nghiêm trọng nhất trong đợt rà soát, vì nó phá chính cơ chế mà dự án dùng để tự sửa sai.

**Đã xảy ra hai lần trước, cả hai đều đã được ghi nhận và sửa:**

1. **16/08/2026** — thu hồi con số "177/177 passed": suite `e2e-test-suite.mjs` cũ assert vào bản cài đặt
   lại bằng JavaScript trong `scripts/e2e/helpers.mjs`, không chạm lõi Rust. Thay bằng 36 phép kiểm socket thật (`63419b8`).
2. **17/08/2026** — sửa `TEST_READY.md`: các tuyên bố *"8 sections / 41 live socket assertions"*,
   *"100% Pass Rate"*, *"150+ Scenarios"* đều sai. `TEST_READY.md` hiện có bảng đo rõ suite nào chạm LIVA thật, suite nào không.

**Lần thứ ba, đang treo trong cây làm việc ngay lúc này:**

| File | HEAD (đúng) | Bản đang treo (sai) |
|---|---|---|
| `PROJECT.md` | M5 = **"NOT ACCEPTED — status reverted 16/08/2026"**, kèm liên kết bằng chứng `TEST_READY.md` | M5 = **DONE**, và **15/15 feature = DONE** |
| `TEST_INFRA.md` | Mở đầu bằng khối **⚠ SUPERSEDED 16/08/2026** giải thích suite cũ "green by construction" | Khối cảnh báo **bị xoá sạch**, thay bằng bảng 11 vùng tính năng đều "5 / 5 / ✓ / ✓" |

Phần bị xoá khỏi `TEST_INFRA.md` chứa chính câu giải thích vì sao con số nhỏ hơn là **có chủ đích**:
`authorization.rs:154` chỉ cấp cho `CommandPrincipal::WebSocketRemote` đúng 9 lệnh trong `REMOTE_COMMANDS`,
nên F1–F15 **không thể** chạy hết qua socket đó — "Padding a socket suite out to 177 would mean faking the difference".

**Nghiệm thu.** `git diff PROJECT.md TEST_INFRA.md` giữ nguyên khối thu hồi/SUPERSEDED và liên kết
tới `TEST_READY.md`; mọi ô "DONE" mới thêm phải kèm lệnh đo được. **Không commit bản viết lại này.**

---

### P1-4 · 6 bộ "E2E" mới là **tautology**, và `test:all-e2e` đã đưa chúng vào `package.json`

`TEST_READY.md` (17/08) đã liệt kê chính xác 6 bộ này ở cột "Reaches real LIVA? = **NO**". Chúng vẫn còn nguyên,
và `package.json` đang treo thêm **7 script mới** gọi chúng, trong đó:

```json
"test:all-e2e": "node scripts/audit-liva-skills.mjs && node scripts/test-skill-scenarios.mjs && node scripts/e2e-cross-feature-suite.mjs && node scripts/e2e-real-world-scenarios.mjs && npm run test -w liva-ui"
```

Kiểm chứng trong phiên này — **0/6 file có `WebSocket`, `ws://` hoặc `spawn(`**:

```bash
grep -c "WebSocket\|ws://\|spawn(" scripts/e2e-cross-feature-suite.mjs   # → 0
```

Chạy thật `node scripts/e2e-cross-feature-suite.mjs` → *"ALL 5 CROSS-FEATURE INTEGRATION FLOWS PASSED
100% INVARIANT VERIFICATION"*, exit 0. Cơ chế, trích nguyên văn `e2e-cross-feature-suite.mjs:211-214`:

```js
const aec3SuppressionGainDb = ttsPlaybackActive ? 48.5 : 0.0;
flow.assertInvariant('AEC3 Echo Suppression Level', aec3SuppressionGainDb >= 40.0, 'Echo suppression >= 40dB');
```

và dòng 237: `frame_time_ms: 16.4` rồi assert `<= 16.7`. **Hằng số tự đặt, tự kiểm.** Xoá toàn bộ mã Rust
và Vue thì cả 6 bộ vẫn xanh. Nhãn thì ghi "WebRTC AEC3", "Swarm DAG", "60 FPS".

**Điểm cần công bằng:** phần *lint scenario JSON* trong `test-skill-scenarios.mjs` và `verify-skill-scenarios.mjs`
(đối chiếu `scenarios.json` với `agents/openai.yaml`, kiểm parity `.agents/` ↔ `.claude/`) **là kiểm tra thật và có giá trị**.
Vấn đề là **tên gọi và cách dùng**: gọi chúng là "E2E Tier 3/4" rồi lấy làm căn cứ cho "M5 DONE".

**Nghiệm thu.** Hoặc (a) đổi tên + di chuyển sang nhóm `lint:*`/`skills:*` và ghi rõ trong đầu file
"đây là ghi chép thiết kế chạy được, KHÔNG phải kiểm chứng", hoặc (b) xoá. Trong cả hai trường hợp:
`test:all-e2e` **không được** trộn bộ giả với `npm run test -w liva-ui`.

---

### P1-5 · 7 thư mục `target_*` không nằm trong `.gitignore` — rác đĩa và bẫy `git add -A`

`.gitignore:11` có `**/target/` — **không khớp** `target_m1/`, `target_survey/`… nên cả 7 hiện là `??`:

```
target_auditor_m1  target_challenger  target_explore  target_m1
target_rev2        target_reviewer_m1 target_survey
```

Đo xong bằng `du -sb` (mất ~15 phút nên phải chạy nền):

| Thư mục | Dung lượng | | Thư mục | Dung lượng |
|---|---:|---|---|---:|
| `target_explore` | 16,11 GB | | `target_auditor_m1` | 5,08 GB |
| `target_m1` | 10,83 GB | | `target_survey` | 2,48 GB |
| `target_reviewer_m1` | 10,44 GB | | `target_rev2` | 2,37 GB |
| `target_challenger` | 8,94 GB | | **Tổng 7 thư mục** | **56,25 GB** |

Để so sánh: `target/` chính (đã được gitignore) là **127,66 GB** — tức riêng đống rác này đã bằng 44% cây build thật.

Việc này **đã bị cắn một lần rồi và được vá sai cách**: `.gitignore:142` có đúng một dòng
`liva-native-core/target_challenger_4_2/` — vá từng cái tên thay vì vá bằng mẫu. Lần này có thêm 7 cái nữa.

Hai hệ quả: (1) mất đĩa, (2) **`git add -A` sẽ nuốt hàng chục GB** — và đây là repo mà `git status`
đã có 65 mục treo, tức nguy cơ này không lý thuyết. Ngoài ra mọi `grep -r` từ gốc repo đều chậm hẳn
(một lệnh `grep -rn` trong phiên này bị timeout 120 s vì quét vào các thư mục này).

**Nghiệm thu.** Thêm `target_*/` vào `.gitignore`; `git status --porcelain | grep -c '^??'` giảm 7;
`git check-ignore -v target_m1` trả về dòng khớp. Xoá các thư mục sau khi xác nhận không phiên nào đang dùng.

---

### P1-6 · Sự kiện `ai_expert_suggestion` phát ra nhưng **không có bên nhận**

`websocket/dialogue.rs:189-199` phát event `ai_expert_suggestion` khi câu hỏi được phân loại `DoKho::Kho`
và có expert model trên máy. Grep toàn bộ `liva-ui/src`:

```
ai_expert_suggestion  → 0 khớp
goi_y_expert / do_kho → 0 khớp
```

Tương tự, `agent/graph/pipeline.rs:158-172` ghi `do_kho` và `goi_y_expert` vào `state.context` nhưng
**không node nào phía sau đọc hai khoá đó** — không có chỗ nào thật sự chuyển sang expert model.

Đây đúng lớp lỗi mà `6e08b34 fix(ui): stop sending two events the backend has no handler for` và
`1d7a684 … make the event contract honest` đã sửa — lần này ngược chiều (core → UI thay vì UI → core).

**Nghiệm thu.** Chọn một trong hai và làm trọn: (a) `useGateway.ts` xử lý `ai_expert_suggestion` và UI
hiện được gợi ý, có test Vitest chứng minh; hoặc (b) gỡ event + hai khoá context, giữ lại `phan_loai_do_kho`
như hàm thuần đã có test riêng. **Không để nguyên trạng thái "phát vào hư không".**

---

### P2-7 · Hai test mới **không thể đỏ** vì tự tính kỳ vọng bằng chính biểu thức của mã sản phẩm

`agent/graph/pipeline.rs:624-627`:

```rust
let co_expert = crate::paths::configured_expert_model_path().is_some_and(|p| p.exists());
assert_eq!(routed.context.get("goi_y_expert").and_then(|v| v.as_bool()), Some(co_expert));
```

Vế phải là **đúng biểu thức** mà `pipeline.rs:160` dùng để sinh vế trái. Test này xanh trên mọi máy,
kể cả khi logic sai. `websocket/dialogue.rs:382-403` có cùng khuôn (`if co_expert { … } else { … }`).

**Chỗ thứ ba tệ hơn hai chỗ trên**, và trớ trêu là nó nằm trong file tên "adversarial challenge" —
`tests/m4_escalation_adversarial_challenge.rs:81,96-100`:

```rust
let expert_model_exists = configured_expert_model_path().is_some_and(|p| p.exists());   // :81
// … trong vòng lặp, sau khi đã assert do_kho == DoKho::Kho:
let co_expert = configured_expert_model_path().is_some_and(|p| p.exists());
let expected_goi_y = matches!(do_kho, DoKho::Kho) && co_expert;
assert_eq!(expected_goi_y, expert_model_exists, "Escalation flag mismatch…");
```

`do_kho` vừa được assert bằng `DoKho::Kho` ở ngay trên, nên `expected_goi_y` **rút gọn đúng bằng**
`co_expert`, mà `co_expert` và `expert_model_exists` là cùng một lời gọi hàm. Tức dòng cuối là
`assert_eq!(x, x)` — và **không có lời gọi nào vào `build_pipeline_graph` hay `handle_user_voice_text`**,
tức logic escalation thật không hề được chạy. Phần duy nhất có giá trị trong vòng lặp đó là
`assert_eq!(phan_loai_do_kho(prompt), DoKho::Kho)` — cái đó thật, giữ lại.

**Nghiệm thu.** Đường dẫn expert model phải **tiêm được** trong test (biến môi trường hoặc tham số),
để có ít nhất một ca khẳng định `goi_y_expert == true` không phụ thuộc máy chạy. Kiểm chứng bằng cách
đảo `matches!(do_kho, DoKho::Kho)` thành `DoKho::Thuong` — test phải **đỏ**.

---

### P2-8 · `espeak_ipa` mới có nguy cơ **kẹt pipe** với văn bản dài — cần đo

`tts/espeak.rs:50-110` thay `.output()` bằng `spawn()` + vòng lặp `try_wait()`, và chỉ `read_to_end`
stdout/stderr **sau khi** tiến trình con thoát (dòng 73, 76).

`std::process::Command::output()` đọc stdout/stderr **song song** với lúc chờ, chính vì lý do này.
Với mã mới: nếu espeak ghi ra nhiều hơn bộ đệm pipe của HĐH, tiến trình con **kẹt ở lệnh ghi**,
`try_wait()` mãi trả `Ok(None)`, vòng lặp chạy hết 10 s rồi `taskkill`. Triệu chứng người dùng:
câu dài thì TTS im lặng sau 10 giây, câu ngắn thì bình thường.

Phần thêm hạn chờ **là đúng và đáng giữ** — chỉ cách đọc ống là sai.

**Nghiệm thu.** Đo trước rồi hãy sửa: gọi `espeak_ipa("vi", <đoạn ~4000 ký tự>)` và xem có trả về
trong <1 s hay treo tới hạn 10 s. Nếu treo: đọc ống bằng luồng riêng (hoặc `wait_with_output()` có hạn chờ)
để giữ được cả hạn chờ lẫn tính đúng.

---

### P2-9 · `scripts/e2e/08-real-world-scenarios.mjs` mồ côi

`3075146` đã gỡ section 8 khỏi bộ socket thật (5 assertion không thể đỏ, làm 36 → 41 giả tạo).
File vẫn nằm ở `scripts/e2e/` trạng thái `??`, cạnh 7 file section thật, và **không runner nào nạp nó**
(`grep "08-real-world" scripts/e2e-gateway*.mjs` → rỗng).

Đặt cạnh 7 file thật, phiên sau rất dễ tưởng nó cùng loại và nối lại vào suite.

**Nghiệm thu.** Xoá, hoặc chuyển ra ngoài `scripts/e2e/` với tên nói rõ nó là mô phỏng.

---

## 3. Những thay đổi đang treo **đáng giữ** — đừng vứt cùng lúc

Rà soát này không kết luận "vứt hết đợt việc đang treo". Bốn thay đổi dưới đây là sửa lỗi thật:

| Thay đổi | File | Vì sao đáng giữ |
|---|---|---|
| `nen_sinh_tiep()` | `llm/engine.rs:108-130` + 4 điểm gọi | Callback cũ trả `true` cứng ⇒ client đóng kết nối rồi mà engine vẫn sinh tiếp tới `max_tokens`, giữ khoá `AppState.llm`. Nay trả `false` khi receiver đã drop. |
| Nạp model trong `spawn_blocking` | `llm/engine.rs:198+` | `LlamaModel::load_from_file` chặn Tokio runtime — vi phạm chính bất biến "không chặn Tokio runtime" của LIVA. |
| `embed` chuyển sang `spawn_blocking` + bỏ `unwrap()` | `commands/llm.rs:101-128` | Cùng lý do; và 2 `serde_json::to_value(...).unwrap()` nay trả `Err`. |
| `check_embed_tokens_fit()` | `llm/embed.rs:15-24` | Chặn `GGML_ASSERT` → `abort()` cả tiến trình. Đúng họ với lỗi `6138f57` (`n_batch`), và **có lý do rõ vì sao không dùng lại `check_prompt_fits`** (đường embedding không cần chừa 512 token). |

Hạn chờ cho `reqwest` và `ffmpeg` ở `telegram.rs:413-520` cũng đúng hướng (trước đó không có hạn chờ nào).

---

## 4. Thứ tự thi hành

Chia làm 4 lát, **mỗi lát một commit**, theo luật §0.2 của backlog: **tách commit mã nguồn khỏi commit tài liệu**.

### Lát 0 — đồng bộ phiên (trước mọi thứ)

1. Xác định 2 phiên Claude Code còn lại đang làm gì; dừng hoặc thống nhất phân chia file.
2. Chờ `cargo test --workspace` của phiên kia xong (đang giữ khoá `target/`).
3. Chụp lại `git status` + `mtime` các file sẽ sửa, để phép đo sau đó có nghĩa.

**Không bỏ qua bước này.** P0-1 và P0-3 đều nằm trên file mà phiên khác đang gõ.

### Lát 1 — làm CI xanh trở lại (P0-1, P0-2, P1-5)

```bash
cargo fmt --all
```
```bash
node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia
```

- Chạy `cargo fmt --all` (3 chỗ), rồi `cargo fmt --all -- --check` phải exit 0.
- Quyết định 4 tài liệu audit trùng nội dung: giữ **một** bản, đặt `stale-ok: 67eb98b` **sau khi đã đọc diff `db.rs`**.
- Thêm `target_*/` vào `.gitignore`, xoá 7 thư mục rác.
- **Nghiệm thu lát 1:** docs-check exit 0 · cargo fmt exit 0 · `git status --porcelain | grep -c '^??'` giảm ≥ 7.

### Lát 2 — sửa hồ sơ dự án cho trung thực (P0-3, P1-4, P2-9)

- Khôi phục khối thu hồi trong `PROJECT.md` (M5 = NOT ACCEPTED) và `TEST_INFRA.md` (SUPERSEDED 16/08).
  Nếu muốn cập nhật hai file này, cập nhật *quanh* khối đó, không xoá nó.
- Đổi tên/di chuyển hoặc xoá 6 bộ tautology; sửa `test:all-e2e` trong `package.json`.
- Xử lý `scripts/e2e/08-real-world-scenarios.mjs`.
- **Nghiệm thu lát 2:** mỗi ô "DONE" trong `PROJECT.md` chỉ được tới từ một lệnh chạy được;
  `TEST_READY.md` và `PROJECT.md` không mâu thuẫn nhau.

### Lát 3 — đóng hai lỗ trong mã đang treo (P1-6, P2-7, P2-8)

- **Chạy `impact()` trước** khi sửa `handle_user_voice_text`, `build_pipeline_graph`, `espeak_ipa`
  (bắt buộc theo `CLAUDE.md`); báo blast radius trước khi gõ.
- P1-6: chọn (a) nối UI hoặc (b) gỡ event. Đừng để lửng.
- P2-7: cho tiêm đường dẫn expert model trong test, chứng minh test **đỏ được**.
- P2-8: **đo trước** rồi mới sửa cách đọc ống.
- **Nghiệm thu lát 3:** `cargo test` xanh; `npm run test:coverage -w liva-ui` xanh; đảo một điều kiện
  trong logic escalation làm test mới **đỏ**.

### Lát 4 — chạy lại đủ đường cơ sở

Đây là bước 1 của giao thức backlog và chưa làm được trong phiên này (khoá `target/`):

```bash
node scripts/e2e-gateway-ci.mjs
```

- `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo deny check`,
  `e2e-gateway-ci.mjs` (8/8), `e2e-memory.mjs` (6/6), `cargo test -p liva-desktop`.
- So với bảng §1 của [`05-nang-cap-toan-dien.md`](docs/03-danh-gia/05-nang-cap-toan-dien.md).
  **Số nào tụt là hồi quy, xử lý trước backlog.**
- Cập nhật `updated:`/`commit:` trong front-matter tài liệu bị chạm.

---

## 5. Tóm tắt mức độ

| Mã | Vấn đề | Mức | Chặn cái gì |
|---|---|---|---|
| P0-1 | `cargo fmt` đỏ 3 chỗ | Chặn CI | bước 6/25 |
| P0-2 | `docs-check` đỏ 2 lỗi | Chặn CI | bước 1/25 ⇒ **24 bước sau không chạy** |
| P0-3 | Xoá khối thu hồi trong `PROJECT.md`/`TEST_INFRA.md` | Chặn hồ sơ | tái phạm lần 3 của cùng một lỗi |
| P1-4 | 6 bộ "E2E" tautology + `test:all-e2e` | Cao | tạo tín hiệu xanh giả |
| P1-5 | 7 thư mục `target_*` không gitignore — **56,25 GB** | Cao | đĩa + `git add -A` |
| P1-6 | `ai_expert_suggestion` không ai nhận | Cao | tính năng M4 escalation chưa thật sự chạy |
| P2-7 | 3 test không thể đỏ (1 chỗ là `assert_eq!(x, x)`) | Vừa | coverage giả |
| P2-8 | `espeak_ipa` nguy cơ kẹt pipe | Vừa | cần đo trước khi kết luận |
| P2-9 | `08-real-world-scenarios.mjs` mồ côi | Thấp | bẫy cho phiên sau |

---

*Rà soát: 18/08/2026 · HEAD `67eb98b` · nhánh `test/perf-threshold-baseline` · mọi con số từ lệnh chạy thật trong phiên.*
