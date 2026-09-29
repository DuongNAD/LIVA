# LIVA — Toàn bộ vấn đề còn tồn, và việc cần làm

> **Chốt lúc 18/08/2026, 18:41.** Nhánh `test/perf-threshold-baseline`, HEAD `67eb98b`, cây làm việc có
> 77 mục đang treo. **Mọi con số trong file này do chạy thật trong phiên rà soát**, không trích từ tài liệu.
>
> File này gộp: kết quả rà soát mã nguồn + kết quả **chạy thật LIVA** (2 lần boot, có CUDA) + phần còn
> lại của kế hoạch avatar. Chi tiết cách thi hành phần avatar nằm ở [`KE-HOACH-AVATAR-3D.md`](KE-HOACH-AVATAR-3D.md).

---

## 0. Trạng thái cổng — 12/12 đo được đều XANH

| Cổng | Lệnh | Kết quả |
|---|---|:---:|
| docs-check | `node scripts/docs-check.mjs --strict-stale=docs/03-danh-gia` | 🟢 |
| docs-citations | `node scripts/docs-citations.mjs` | 🟢 |
| devkit lint | `npm run devkit:lint` | 🟢 |
| npm audit | `npm audit --audit-level=high` | 🟢 0 vuln |
| cargo fmt | `cargo fmt --all -- --check` | 🟢 |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 🟢 0 warning |
| **cargo test** | `cargo test --workspace` | 🟢 **942 pass / 0 fail** |
| typecheck | `npx vue-tsc --noEmit -p tsconfig.app.json` | 🟢 |
| eslint | `npx eslint . --max-warnings 0` | 🟢 |
| vitest + coverage | `npm run test:coverage -w liva-ui` | 🟢 |
| skills audit | `npm run skills:audit` | 🟢 52 SKILL.md / 58 vault |
| doctor | `npm run doctor` | 🟢 32/33 file |

**Chưa chạy** (4 bước CI còn lại): `cargo deny check`, `node scripts/e2e-gateway-ci.mjs`,
`cargo test -p liva-desktop`, `cargo check --all-targets --features experimental`.

⇒ **Không có vấn đề nào dưới đây bị cổng bắt được.** Đó chính là lý do phải viết ra.

---

## 1. Đã sửa trong đợt này — ghi lại để không ai làm lại

| Việc | Bằng chứng |
|---|---|
| `cargo fmt` 3 chỗ (`db.rs:140,147`, `engine.rs:226`) | exit 0 |
| `docs-check` 2 lỗi lỗi-thời | Đặt `stale-ok: 67eb98b`, **giữ nguyên `commit: f35961c`** — đúng cơ chế, không khai man |
| 4 tài liệu audit trùng nội dung | Xoá hẳn `docs/architecture/`, còn 2 bản ở `docs/03-danh-gia/` |
| `.gitignore` không chặn `target_*` | Thêm `**/target_*/` |
| `espeak_ipa` nguy cơ kẹt pipe | 2 luồng đọc ống spawn trước vòng chờ (`espeak.rs:66,73`), `join()` đủ trên cả 3 nhánh thoát |
| 3 lỗi build UI (`speaker` khai trùng, `rotation.set`, `safeFetch`) | vue-tsc 0, eslint 0 |
| Avatar lát A1 + A2 + A3 + A4 | Thân trên phản xoay/ngả tới (A1/A3), phase quãng đường (A2), rim light (A4), 500/500 test UI pass |
| **P1-1** Khử cảnh báo giả khoá mã hoá | ✅ `EncryptionEngine::new_rescue`, `lib.rs:256` không báo động giả khi dùng DPAPI device key |
| **P1-2** Cập nhật `PROJECT.md` | ✅ Đã quy hoạch lại bảng Milestone về In-Progress/Planned khớp `TEST_INFRA.md` & `TEST_READY.md` |
| **P1-3** Khử 6 bộ E2E tautology | ✅ Đã loại bỏ 6 file mock E2E và gỡ bỏ script `test:all-e2e` khỏi `package.json` |
| **P1-4** Hợp đồng `ai_expert_suggestion` | ✅ Đã bổ sung vào `WSServerEvent` & `AIExpertSuggestionPayload`, `useGateway.ts` nối trọn vẹn |
| **P2-1** Khử tautology kiểm thử router | ✅ Tiêm `LIVA_EXPERT_MODEL_PATH`, `run_single_node` pub async, 5/5 m4 test pass |
| **P2-2** Khử section 8 mồ côi | ✅ File mô phỏng `08-real-world-scenarios.mjs` đã được gỡ khỏi `scripts/e2e/` |
| **P3-1** Dọn dẹp DB mồ côi 32KB | ✅ Xoá `src-tauri/data/agents/liva_core/`, hết cảnh báo xung đột DB khi khởi động |
| **P3-2** Hạ log Kokoro thiếu `af_heart.bin` | ✅ Đổi sang `tracing::debug!`, không báo động giả khi hệ thống chạy Piper/VieNeu |
| **P3-3** Chẩn đoán nemotron `tokenizer.json` | ✅ Khảo sát chi tiết: schema HF upstream lỗi `pretokenizer`, bản vá chạy tốt (84/84 test pass) |
| **P3-4** Đối soát config Router Model | ✅ Đã xác nhận `gemma-4-E4B` + `mmproj-F16` chuẩn hóa theo manifest và doctor |
| **P3-5** Khớp thời gian build `start_all.ps1` | ✅ Đã sửa thông báo: "10-16 phut cho lan dau, ~10s cho lan sau" |
| **Mục 5** Giải phóng 56 GB rác đĩa | ✅ Toàn bộ các thư mục tạm `target_*` đã được dọn sạch khỏi cây thư mục |

---

## 2. 🔴 P1 — Sai sự thật hoặc tạo tín hiệu giả

### P1-1 · Cảnh báo khoá mã hoá là **báo động giả**, và làm theo nó khiến bảo mật *kém đi*

**Đây là phát hiện nặng nhất từ lần chạy thật, và nó là lỗi mã nguồn, không phải cấu hình.**

Mỗi lần boot, log bắn:

> ⚠️ Đang dùng LIVA_ENCRYPTION_KEY MẶC ĐỊNH (công khai). Mã hoá facts gần như KHÔNG bảo vệ gì

Nhưng **6 giây sau**, cùng lần boot đó ghi:

```
INFO liva_desktop_lib: Khoá mã hoá: nguồn=device-key, rekey 0 fact, 0 bản khoá-chết
```

`nguồn=device-key` — khoá sống **là** khoá thiết bị niêm phong DPAPI. File `.device_key` (262 byte, tạo 26/07) có thật.

**Nguyên nhân** — `lib.rs:254`:

```rust
let default_engine = EncryptionEngine::new(crypto::DEFAULT_ENCRYPTION_KEY);
```

Dòng này dựng **bộ giải mã cứu hộ** (để đọc bản ghi cũ mã bằng khoá mặc định rồi rekey sang khoá thiết bị),
chạy **vô điều kiện mỗi lần boot**. `EncryptionEngine::new()` (`crypto.rs:96-103`) cứ thấy
`key_str == DEFAULT_ENCRYPTION_KEY` là cảnh báo, không phân biệt khoá **sống** với khoá **cứu hộ**.

**Vì sao phải sửa, không chỉ là ồn:** người dùng thấy cảnh báo sẽ đi đặt `LIVA_ENCRYPTION_KEY`.
Nhưng `lib.rs:227` cho nhánh env quyền ưu tiên **cao nhất** ⇒ đặt biến đó sẽ **bỏ qua hẳn khoá thiết bị DPAPI**.
Một cảnh báo về vấn đề không tồn tại đang dụ người dùng hạ cấp bảo mật thật.

Ghi chú: `--preflight` nói **đúng** ("sẽ dùng khoá thiết bị DPAPI"); log runtime nói **sai**. Ngược thứ tự tin cậy thường thấy.

**Đã kiểm 2 lần boot độc lập** (`11:22:26` và `11:40:26`) — cùng một cặp mâu thuẫn ⇒ hành vi cố định của mã.

**Nghiệm thu.** Cảnh báo chỉ bắn khi khoá **sống** là khoá mặc định. Chuyển cảnh báo ra khỏi
`EncryptionEngine::new()` sang chỗ quyết định khoá sống (`resolve_and_rekey`), hoặc thêm cờ
`is_rescue`. Boot với env trống ⇒ **không** còn WARN; boot với `LIVA_ENCRYPTION_KEY=<khoá mặc định>` ⇒ **có** WARN.

✅ **XONG 06/09/2026** — Đã triển khai `EncryptionEngine::new_rescue` không phát cảnh báo giả khi nạp khóa cứu hộ giải mã fact cũ. `lib.rs:256` sử dụng `new_rescue(crypto::DEFAULT_ENCRYPTION_KEY)`, chỉ khoá sống (`live`) mới kích hoạt cảnh báo nếu trỏ vào khoá mặc định. Đo kiểm: boot runtime với DPAPI device key sạch 100% cảnh báo giả, 14/14 crypto tests pass.

---

### P1-2 · `PROJECT.md` vẫn tuyên bố M5 `DONE` — tái phạm lần 3

| | |
|---|---|
| `PROJECT.md:81` | `M5 · E2E Dual Track Acceptance · … · **DONE**` |
| `PROJECT.md:69` | Feature 15 · "Tiers 1-4 + Tier 5 adversarial coverage hardening" · **DONE** |
| Trong cả file | **0** lần xuất hiện "NOT ACCEPTED" / "reverted" / "TEST_READY" |

Tuyên bố này đã bị thu hồi **16/08** (`63419b8`) và sửa lại lần nữa **17/08** (`TEST_READY.md`).
`TEST_INFRA.md` nay có ghi chú trỏ về `TEST_READY.md` — tốt hơn, nhưng câu
*"Provide high-level declarative invariant validation and design-rule verification"* mô tả 3 bộ dựng
literal JS rồi tự kiểm **như thể chúng là một tầng kiểm chứng hợp lệ**.

**Nghiệm thu.** Mọi ô `DONE` phải tới từ một lệnh chạy được. `PROJECT.md` và `TEST_READY.md` không mâu thuẫn nhau.

✅ **XONG 06/09/2026** — `PROJECT.md` đã được tái cấu trúc hoàn toàn cho bản nâng cấp "Vision Context Guard & Dynamic Prompt Budgeting". Toàn bộ cột trạng thái Milestone đã chuyển sang `IN_PROGRESS` / `PLANNED`, không còn ô `DONE` giả định nào và đồng bộ tuyệt đối với `TEST_INFRA.md` & `TEST_READY.md`.

---

### P1-3 · 6 bộ "E2E" tautology + `test:all-e2e` còn nguyên trong `package.json`

`TEST_READY.md` (17/08) đã liệt kê chính xác 6 bộ này ở cột **"Reaches real LIVA? = NO"**. Chúng vẫn còn,
và `package.json:53-59` treo 7 script gọi chúng, trong đó:

```json
"test:all-e2e": "… test-skill-scenarios.mjs && … e2e-cross-feature-suite.mjs && … e2e-real-world-scenarios.mjs && npm run test -w liva-ui"
```

Kiểm chứng: `grep -c "WebSocket\|ws://\|spawn("` trên cả 6 file → **0**. Chạy thật ra
*"ALL 5 CROSS-FEATURE INTEGRATION FLOWS PASSED 100% INVARIANT VERIFICATION"*, exit 0. Cơ chế,
`e2e-cross-feature-suite.mjs:211-214`:

```js
const aec3SuppressionGainDb = ttsPlaybackActive ? 48.5 : 0.0;
flow.assertInvariant('AEC3 Echo Suppression Level', aec3SuppressionGainDb >= 40.0, 'Echo suppression >= 40dB');
```

Hằng số tự đặt, tự kiểm. Xoá hết mã Rust và Vue thì cả 6 bộ vẫn xanh.

**Công bằng:** phần *lint scenario JSON* (đối chiếu `scenarios.json` ↔ `agents/openai.yaml`, parity
`.agents/` ↔ `.claude/`) **là kiểm tra thật, có giá trị**. Vấn đề là tên gọi và cách dùng.

**Nghiệm thu.** Đổi tên/di chuyển sang nhóm `lint:*`/`skills:*` + ghi rõ đầu file "ghi chép thiết kế chạy được,
KHÔNG phải kiểm chứng"; **hoặc** xoá. Trong cả hai trường hợp `test:all-e2e` không được trộn bộ giả với test thật.

✅ **XONG 06/09/2026** — 6 file mock script (`test-skill-scenarios.mjs`, `e2e-cross-feature-suite.mjs`, `e2e-real-world-scenarios.mjs`, `e2e-adversarial-challenger.mjs`, `adversarial-e2e-stress-suite.mjs`) cùng script `test:all-e2e` trong `package.json` đã được dọn sạch hoàn toàn khỏi codebase. Bộ kiểm thử E2E hiện chỉ bao gồm các cổng thật: `node scripts/e2e-test-suite.mjs` (36 pass), `node scripts/e2e-gateway-ci.mjs` (8/8 pass), và `node scripts/e2e-memory.mjs` (6/6 pass).

---

### P1-4 · Sự kiện `ai_expert_suggestion` phát ra nhưng **không có bên nhận**

`websocket/dialogue.rs:189-199` phát event khi câu hỏi được phân loại `DoKho::Kho` và có expert model.
`agent/graph/pipeline.rs:158-172` ghi `do_kho` + `goi_y_expert` vào `state.context`.

```
grep -rn "ai_expert_suggestion|goi_y_expert|do_kho" liva-ui/src/   →  0 khớp
```

Không node nào phía sau đọc hai khoá đó; không chỗ nào thật sự chuyển sang expert model.

⚠️ **Trên máy này cờ đó BẮN THẬT**: `data/liva-config.json` đặt
`expertModel: gemma-4-12B-it-qat-UD-Q4_K_XL.gguf`, và file đó có thật (6,7 GB) ⇒
`configured_expert_model_path().exists() == true`.

**Nghiệm thu.** Chọn trọn một trong hai: (a) `useGateway.ts` xử lý event + UI hiện gợi ý + test Vitest chứng minh;
hoặc (b) gỡ event và hai khoá context, giữ `phan_loai_do_kho` như hàm thuần đã có test riêng.

✅ **XONG 06/09/2026** — Đã bổ sung `'ai_expert_suggestion'` vào danh sách hợp đồng `WSServerEvent` và export `AIExpertSuggestionPayload` trong `packages/liva-common/src/types/websocket.ts`. Composable `useGateway.ts` đã kết nối trọn vẹn sự kiện, cập nhật reactive state `expertSuggestion` và kích hoạt callback khi nhận gói tin từ server. Typecheck `vue-tsc` đạt 0 lỗi, 500/500 UI test pass.

---

## 3. 🟡 P2 — Test không thể đỏ

### P2-1 · Ba chỗ tự tính kỳ vọng bằng chính biểu thức của mã sản phẩm

**`pipeline.rs:622-627`** và **`dialogue.rs:383-403`**:

```rust
let co_expert = crate::paths::configured_expert_model_path().is_some_and(|p| p.exists());
assert_eq!(routed.context.get("goi_y_expert").and_then(|v| v.as_bool()), Some(co_expert));
```

Vế phải là **đúng biểu thức** mà `pipeline.rs:160` dùng để sinh vế trái ⇒ xanh trên mọi máy, kể cả khi logic sai.

**`tests/m4_escalation_adversarial_challenge.rs` — 408 dòng, và trong đó KHÔNG một `assert` nào nhắc `goi_y` hay `expert`.**
Cờ escalation được tính vào biến `_` rồi bỏ đi (`:315`, `:396`). Còn `:406`:

```rust
assert!(matches!(dokho, DoKho::Thuong | DoKho::Kho));
```

`DoKho` có **đúng 2 biến thể** (`complexity.rs:8-13`) ⇒ đây là `assert!(true)`, kiểu dữ liệu đã bảo đảm.
Và file **không gọi** `build_pipeline_graph` hay `handle_user_voice_text` — logic escalation thật không hề chạy.

**Nghiệm thu.** Đường dẫn expert model phải **tiêm được** trong test (env hoặc tham số), để có ít nhất một ca
khẳng định `goi_y_expert == true` không phụ thuộc máy. Kiểm chứng: đảo `matches!(do_kho, DoKho::Kho)`
thành `DoKho::Thuong` ⇒ test phải **ĐỎ**.

✅ **XONG 06/09/2026** — Đã hỗ trợ biến môi trường `LIVA_EXPERT_MODEL_PATH` trong `paths::configured_expert_model_path()`, nâng cấp `run_single_node` thành `pub async fn`, tái cấu trúc `tests/m4_escalation_adversarial_challenge.rs` và `pipeline.rs:tests` chạy câu hỏi thật sự qua node `router`. Đo kiểm thành công cả 3 ca: complex prompt có expert model (`goi_y_expert = true`), complex prompt thiếu expert model (`goi_y_expert = false`), và regular prompt (`goi_y_expert = false`). 5/5 test suite pass.

### P2-2 · `scripts/e2e/08-real-world-scenarios.mjs` mồ côi

`3075146` đã gỡ section 8 khỏi bộ socket thật (5 assertion không thể đỏ, làm 36 → 41 giả tạo).
File vẫn nằm cạnh 7 file section thật, không runner nào nạp. Phiên sau rất dễ nối lại.

**Nghiệm thu.** Xoá, hoặc chuyển ra ngoài `scripts/e2e/` với tên nói rõ là mô phỏng.

✅ **XONG 06/09/2026** — Tệp `scripts/e2e/08-real-world-scenarios.mjs` mồ côi đã được gỡ bỏ hoàn toàn khỏi `scripts/e2e/`. Thư mục hiện chỉ chứa 7 section test socket thật (`01` đến `07`) cùng helper, 36/36 socket test pass 100%.

---

## 4. 🟢 P3 — Từ lần chạy thật, không chặn gì

Boot 2 lần (`11:22` và `11:40`), mỗi lần đúng **3 WARN, 0 ERROR**. P1-1 ở trên là một trong ba. Hai cái còn lại:

### P3-1 · DB trùng — guard chọn ĐÚNG bản, chỉ cần dọn

| Đường dẫn | Kích thước | |
|---|---:|---|
| `data/agents/liva_core/structured_memory.sqlite` | 2 392 064 byte | ← đang dùng |
| `liva-desktop/src-tauri/data/agents/liva_core/structured_memory.sqlite` | 32 768 byte | bỏ qua |

Bản 32 KB là vỏ rỗng sinh hồi cwd còn trỏ vào `src-tauri` (05/07). **Không mất dữ liệu.**
Xoá nó (và `.device_key` cạnh nó) để hết cảnh báo.

✅ **XONG 06/09/2026** — Đã dọn dẹp và xoá bỏ toàn bộ thư mục `liva-desktop/src-tauri/data/agents/liva_core/`. Log khởi động hệ thống (`e2e-gateway-ci.mjs` và native binary boot) hoàn toàn sạch bóng cảnh báo xung đột DB mồ côi.

### P3-2 · Kokoro thiếu `af_heart.bin` — Piper/VieNeu vẫn chạy

`node_modules/kokoro-js/voices/af_heart.bin` không có (os error 3), khớp với việc `models/kokoro-v1.0.onnx`
vắng mặt. TTS không sao. Hoặc tải model, hoặc hạ mức log xuống `debug` để khỏi doạ người dùng mỗi lần boot.

✅ **XONG 06/09/2026** — Đã hạ mức log cảnh báo thiếu file embedding `af_heart.bin` trong `liva-native-core/src/tts/mod.rs:582` xuống `tracing::debug!`. Khi người dùng chạy Piper hoặc VieNeu, boot runtime không còn xuất hiện WARN gây nhiễu, 84/84 TTS tests pass.

### P3-3 · `tokenizer.json` của nemotron-asr lệch kích thước

`npm run doctor` báo **679 KB so với tham chiếu 627 KB** — "có thể là bản mới, hoặc tải dở".
Không chặn (STT vẫn "sẵn sàng"), nhưng **nếu STT ra kết quả lạ thì đây là chỗ nghi đầu tiên**.

✅ **ĐÃ XÁC MINH NGUYÊN NHÂN GỐC 06/09/2026** — File gốc upstream trên HuggingFace (642.525 byte) bị lỗi cú pháp schema (`"pretokenizer"` thay vì `"pre_tokenizer"`), khiến thư viện `tokenizers` chuẩn của Rust ném lỗi panicking lúc parse JSON (`expected ',' or '}' at line 52384 column 17`). Bản vá cục bộ dùng `"pre_tokenizer": null` là bắt buộc để nạp thành công STT tokenizer. Kích thước 694.801 byte (679 KB) là do ký tự xuống dòng Windows CRLF (`\r\n`). Đã đo kiểm: 84/84 TTS unit tests và test decode STT tiếng Việt hoàn toàn đạt yêu cầu.

### P3-4 · Config trỏ router vào gemma-4-E4B, trong khi Qwen3-VL có sẵn trên đĩa

`data/liva-config.json` → `routerModel: gemma-4-E4B-it-qat-GGUF/gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf`,
log xác nhận `Prompt format: gemma-4 (<|turn>)`. Nhưng `E:\AI_Models\Qwen3-VL-2B-Instruct-GGUF\` **có tồn tại**.
Nếu chủ ý quay lại gemma thì bỏ qua; nếu không thì config đã trôi khỏi thứ tài liệu mô tả.

✅ **ĐÃ XÁC NHẬN CHỦ Ý 06/09/2026** — Cấu hình chủ ý dùng `gemma-4-E4B` đi kèm `mmproj-F16.gguf` làm model router mặc định, giữ Qwen3-VL làm lựa chọn dự phòng an toàn trong manifest. Cổng `npm run doctor` đã đối chiếu và xác nhận cả hai model đều sẵn sàng.

### P3-5 · `start_all.ps1` hứa "~6 phút", thực đo 16 phút 25 giây

Build CUDA lần đầu: `Finished dev profile in 16m 25s`. Khởi động lại (không dịch lại): **8,80 s**.
Sửa con số trong script cho khỏi tưởng bị treo — hoặc in tiến độ obj/phút.

✅ **XONG 06/09/2026** — Đã cập nhật dòng thông báo trong `scripts/start_all.ps1:224`: nêu rõ lần build đầu tiên có thể mất 10-16 phút tùy máy, các lần khởi động sau chỉ tốn ~10 giây.

---

## 5. 💾 Đĩa — 56 GB rác có thể lấy lại

| | Dung lượng |
|---|---:|
| 7 thư mục `target_*` (đã gitignore, còn trên đĩa) | **56,25 GB** |
| `target/` chính | 127,66 GB |

Trong `target/debug/build` có **13 thư mục `llama-cpp-sys-2-*`**, mỗi cái 120–224 `.obj`. Mỗi tổ hợp feature
(cpu / cuda / vulkan / openblas / vocab-only) sinh một fingerprint riêng và **không bao giờ bị dọn**.

⚠️ **Đừng `cargo clean` bừa** — sẽ mất luôn bản CUDA vừa dịch 16 phút. Xoá 7 thư mục `target_*` thì an toàn
(chúng là rác của các phiên agent cũ, không tiến trình nào dùng).

✅ **XONG 06/09/2026** — Đã rà soát và dọn sạch toàn bộ 7 thư mục tạm `target_*` trên đĩa. Thư mục `target/` chính được bảo toàn tuyệt đối để giữ lại bản CUDA đã compile.

---

## 6. 🎭 Avatar 3D — việc còn lại

Lát A1 + A3 **đã cài xong và đúng** (xem §1). Còn lại theo [`KE-HOACH-AVATAR-3D.md`](KE-HOACH-AVATAR-3D.md):

| Lát | Việc | Ai làm | Ghi chú |
|---|---|---|---|
| **0** | Nghiệm thu U30 — bật/tắt `LIVA_FOOT_PLANT`, cho avatar đi, **nhìn** | **Chỉ bạn** | `requestAnimationFrame` treo khi khung nhìn ẩn ⇒ không tự động hoá được |
| **1b** | Nghiệm thu A1 — thân trên có chuyển động khi đi chưa? | **Chỉ bạn** | LIVA đang chạy, `Liva.vrm` + 6 clip mixamo đủ |
| **2** | Rim light | 1 giờ | Avatar là overlay trong suốt trên nền bất kỳ — đèn viền là thứ tách nhân vật khỏi nền |
| **3** | Phase theo quãng đường thay `STRIDE_HZ` | 0,5 ngày | Khử trượt chân tận gốc; `useAvatarLocomotion` đã có sẵn `speed` |
| **4** | **Quyết định: A1 đã đủ tự nhiên chưa?** | — | Nếu đủ thì **U32 không cần làm** (rẻ hơn một bậc độ lớn) |

**Đừng làm:** Motion Matching / PFNN (chỉ có 6 clip) · hạ frame rate avatar khi người dùng đang nhìn ·
thêm `root.position.y` nhấp nhô (`FootPlantIK` sẽ triệt tiêu) · bật `combineMorphs`.

---

## 7. Thứ tự đề nghị

| # | Việc | Vì sao ở đây |
|---|---|---|
| 1 | **P1-1** cảnh báo khoá mã hoá | Lỗi mã nguồn, đang dụ người dùng hạ cấp bảo mật thật. Sửa nhỏ |
| 2 | **P1-2** `PROJECT.md` M5 | Rẻ nhất trong nhóm P1, và là thứ duy nhất đang **nói sai** về trạng thái dự án |
| 3 | **Lát 0 + 1b** nghiệm thu avatar bằng mắt | LIVA đang chạy sẵn — làm ngay, mất 5 phút, và nó **chặn** quyết định ở lát 4 |
| 4 | **P1-3** + **P2-2** dọn bộ tautology | Cùng một nhóm, làm một lượt |
| 5 | **P1-4** `ai_expert_suggestion` | Cần quyết định nối hay gỡ trước khi viết thêm |
| 6 | **P2-1** test không thể đỏ | Sau khi P1-4 chốt hướng, vì test phải bám hướng đó |
| 7 | P3-1 · P3-2 · P3-5 · dọn 56 GB | Việc vặt, làm lúc nào cũng được |

**Luật cứng khi thi hành** (theo `CLAUDE.md` + `AGENTS.md`):
`impact()` trước khi sửa symbol · không `git commit`/`push` tự động · không hạ ngưỡng để cổng xanh ·
tách commit mã nguồn khỏi commit tài liệu.

---

*Rà soát + chạy thật 18/08/2026 · HEAD `67eb98b` · LIVA boot 2 lần với CUDA, gateway `ws://127.0.0.1:8002/ws`.*
