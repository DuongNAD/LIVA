# Vision Context Guard & Dynamic Prompt Budgeting — baseline và Bước 3–4

Ngày: 17/09/2026. Trạng thái hiện tại: **đã triển khai Bước 5–6 và runtime suites Bước 7; 53 test model-free pass + runtime text-path thật 1 pass (release, gemma-4-E2B); vision runtime BLOCKED do thiếu mmproj fixture**. Mục 1–6 lưu baseline Bước 1–2; cập nhật mới nhất tại mục 9.

Kế hoạch: `E:\Project\01_AI_Agents\LIVA\implementation_plan.md`, mục [Implementation Order]. Người dùng chỉ cho phép thực hiện Bước 1 và Bước 2; Bước 3 trở đi cần xác nhận riêng.

## 1. Phạm vi và quyền thực hiện

- Người dùng cấp waiver `search_vault` cho phiên này: được tra cứu/đọc Vault trực tiếp bằng công cụ file. Không sửa AGENTS.md để tạo ngoại lệ lâu dài.
- Đã đọc RFC 3, dòng 554–645, tại `E:\Project\01_AI_Agents\LIVA\teamwork_projects\obsidian_llm_wiki\vault\Knowledge\deepseek_harness_integration.md` và hướng dẫn tại `E:\Project\01_AI_Agents\LIVA\teamwork_projects\obsidian_llm_wiki\vault\Knowledge\liva_architecture.md`.
- Giữ backend Rust; không chạy/sửa legacy Python/Node, không chụp màn hình, không tải model hoặc thay cấu hình runtime.
- Tool GitNexus impact không được expose trong phiên. Không khởi động indexer thay thế. Waiver search_vault không được diễn giải thành waiver impact; phân tích impact trước sửa symbol còn là điều kiện của giai đoạn sau. Không có symbol nào bị sửa trong Bước 1–2.

## 2. Snapshot và tính toàn vẹn

HEAD: `67eb98b7a18daa0197742f2aae9f17d4b404a82f`.

Working tree đã có thay đổi staged của người dùng. Snapshot chụp tại thư mục:

`C:\Users\Admin\AppData\Local\Temp\liva-prompt-baseline-20260917-104119`

Các tệp bằng chứng trong thư mục này:
- `hashes-before.json`: SHA-256 của 10 file trong phạm vi.
- `staged.patch`: diff staged dạng binary, 105877 byte.
- `unstaged.patch`: diff unstaged của cùng phạm vi, 0 byte lúc chụp.
- `status-before.txt`: trạng thái Git trong phạm vi.
- `staged-after.patch`: bản đối chiếu sau test; SHA-256 bằng staged.patch.

10 file đối chiếu: `E:\Project\01_AI_Agents\LIVA\AGENTS.md`, `E:\Project\01_AI_Agents\LIVA\Cargo.toml`, `E:\Project\01_AI_Agents\LIVA\Cargo.lock`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\Cargo.toml`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\engine.rs`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\prompt\dynamic_prompt.rs`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\lib.rs`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\agent\graph\pipeline.rs`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\dynamic_prompt_assembly_tests.rs`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\vision_context_guard_tests.rs`.

Kết quả đối chiếu sau test: **10/10 hash không đổi; diff staged trong phạm vi không đổi**. Không sửa mã nguồn/test/manifest/lockfile, không add/commit/remote, không rollback thay đổi người dùng. Cargo có cập nhật build artifacts trong target hiện hữu. Snapshot nằm trong TEMP nên không bảo đảm lưu trữ dài hạn; đây không phải bản backup dữ liệu người dùng.

## 3. Môi trường đo

- RAM khả dụng khi snapshot: 25,27 GiB; trước các lượt test đều kiểm >=4 GiB.
- Không thấy cargo/rustc chạy khi bắt đầu; các lượt test thực hiện tuần tự.
- Cargo: 1.98.1 (`797e8a9bc`, 2026-08-05).
- Rustc: 1.98.1 (`48a229cea`, 2026-09-01).
- CUDA toolkit: 12.8, nvcc V12.8.93.
- GPU: NVIDIA GeForce RTX 5060 Ti; driver 610.62; tổng 16311 MiB, khả dụng 13928 MiB tại thời điểm hỏi nvidia-smi. Đây không phải peak VRAM khi inference.

## 4. Baseline thực chạy

Working directory: `E:\Project\01_AI_Agents\LIVA`.

| Phạm vi | Lệnh Cargo | Kết quả | Log trong thư mục snapshot |
|---|---|---|---|
| Prompt assembly hiện có | `cargo test -p liva-native-core --test dynamic_prompt_assembly_tests --locked --offline -j 2 -- --test-threads 2` | 11 pass, 0 fail, 0 ignored; exit 0 | dynamic-prompt-retry.log; dynamic-prompt-retry.exit |
| Vision context guard hiện có | `cargo test -p liva-native-core --test vision_context_guard_tests --locked --offline -j 2 -- --test-threads 2` | 21 pass, 0 fail, 0 ignored; exit 0 | vision-guard-test.log; vision-guard-test.exit |
| Unit guard engine bổ sung | `cargo test -p liva-native-core --lib llm::engine::tests::guard_ --locked --offline -j 2 -- --test-threads 2` | 5 pass, 0 fail, 734 filtered out; exit 0 | engine-guard-test.log; engine-guard-test.exit |

Tổng: **37 test pass trong ba lượt chọn lọc**, không phải toàn bộ workspace pass. Nhóm 5 test bổ sung chỉ kiểm mã hiện hữu, không triển khai Bước 3.

Lượt prompt đầu dùng redirect PowerShell bị dừng khi dòng Cargo `Compiling` trên stderr trở thành NativeCommandError. Không ghi lượt đó là test fail. Lượt chạy lại redirect stdout/stderr qua `cmd /d /c` hoàn tất; cả ba lượt cuối có file exit code 0. Không cần chạy lại baseline vì thông báo “proceed while running”.

Cả ba lượt có cảnh báo future-incompatibility của `proc-macro-error2 v2.0.1`. Chưa điều tra/sửa dependency trong phạm vi này.

## 5. Fixture và giới hạn kiểm chứng

| Hạng mục | Trạng thái quan sát | Kết luận |
|---|---|---|
| CPU/default-feature, debug test | Ba lượt Cargo trên hoàn tất | Baseline model-free có bằng chứng mới |
| LIVA_TEST_MODEL_PATH | Chưa đặt trong môi trường phiên | Chưa cấu hình fixture GGUF cho runtime test mới |
| LIVA_TEST_MMPROJ_PATH | Chưa đặt trong môi trường phiên | Chưa cấu hình fixture projector matching |
| Release vision probe | Không có `E:\Project\01_AI_Agents\LIVA\target\release\gemma4_probe.exe` và `E:\Project\01_AI_Agents\LIVA\target\release\qwen3vl_probe.exe` tại thời điểm kiểm tra | Không dùng debug probe để chứng minh vision inference; chưa build release |
| Debug probe | Có gemma4_probe.exe và qwen3vl_probe.exe trong `E:\Project\01_AI_Agents\LIVA\target\debug` | Artifacts cũ không được coi là bằng chứng chạy hiện tại |
| Runtime suites mới | Chưa có `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\prompt_budget_runtime.rs` và `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\vision_budget_runtime.rs` | Đúng với trạng thái chưa triển khai; không chạy lệnh trỏ tới test chưa tồn tại |
| CUDA | Có toolkit/GPU; chưa build/test feature cuda | Khả dụng phần cứng không đồng nghĩa runtime CUDA đã đạt |

Không kết luận model không tồn tại trên máy: mới xác minh biến fixture chưa được đặt và hai đường dẫn release probe không tồn tại. Không tải/tìm quét toàn ổ hoặc tự chọn model thay người dùng.

21 test vision hiện có bao gồm test helper cục bộ (clamp, clear Vec, kiểm byte PNG) và test guard/authorization/region_rgb production; không gọi `answer_with_image` để thực thi mtmd. Test tên end-to-end của prompt chỉ đi qua assembler/compiler, không phải E2E transport/inference. Kết quả xanh chưa chứng minh exact-token budgeting, an toàn KV recovery hoặc cap image token trên projector thật.

Chưa chạy: full lib suite, workspace suite, cargo check/clippy/fmt, transport E2E, release vision, runtime CUDA, benchmark TTFT/RAM/VRAM. Không có bằng chứng lỗi pytest, socket.connect retry hoặc treo 25 giây trong phạm vi đã đo; không khởi động server/mock adapter ngoài kế hoạch.

## 6. Bàn giao

- **Bước 1:** đã đọc Vault theo waiver, chụp diff/hash và kiểm tài nguyên; ghi rõ GitNexus impact chưa được expose, cần giải quyết trước sửa symbol.
- **Bước 2:** hoàn tất baseline test hiện có và kiểm tra mức sẵn sàng fixture. Runtime acceptance còn **CHƯA ĐƯỢC KIỂM CHỨNG**, không ghi DONE cho nâng cấp chức năng.
- **Bước 3 trở đi:** chưa bắt đầu. Bước tiếp theo theo kế hoạch là ContextTokenBudget và kiểm biên số học, chỉ thực hiện sau xác nhận phạm vi và điều kiện impact.

Tài liệu này lưu baseline lịch sử; cập nhật Bước 3–4 bên dưới không thay thế trạng thái nghiệm thu runtime của toàn dự án.

## 7. Cập nhật triển khai Bước 3–4 — 17/09/2026

Người dùng đã phê duyệt Bước 3–4 và waiver GitNexus/PDG cho phiên này, yêu cầu dùng tìm kiếm nhẹ. Các điều kiện chặn công cụ trong phần baseline phía trên là lịch sử, đã được miễn trừ cho lượt triển khai này. Đã đọc lại Vault và dùng Select-String trên Rust src/tests để tìm definitions/callers, không chạy indexer.

### Thay đổi thực hiện
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\prompt\dynamic_prompt.rs`: ContextTokenBudget với field private, constructor checked arithmetic, max_prompt_tokens/can_fit; bỏ sàn vượt ngân sách và phép nhân có thể overflow ở constructor PromptBudget; thêm PromptSelection, BudgetedPrompt, lỗi có cấu trúc, selector, validation ID/group, exact compile–measure–evict.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\engine.rs`: check_prompt_fits dùng ContextTokenBudget cho prompt không rỗng, giữ signature, completion reserve 512 và thông báo lỗi cũ. Giữ riêng compatibility prompt=0/n_ctx=513 mà constructor mới không cho phép. Test ma trận 13 x 13 đối chiếu công thức cũ, gồm biên usize::MAX.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\dynamic_prompt_assembly_tests.rs`: thêm 15 test; tổng 26 so với baseline 11. Gồm validation, mandatory/P0, current-user suffix, nhóm lịch sử nguyên tử, thứ tự P4→P1, tie chronological, Unicode/template overhead, callback lỗi và số đo phi tuyến không đơn điệu.

Selector giữ mọi system message và user gần nhất + suffix; reject prefix assistant/tool không có user trước đó. ChatMessage không có call IDs: đây là bảo toàn nhóm user-turn, không phải bộ kiểm chứng toàn bộ giao thức tool-call. API exact nhận slices do caller cung cấp phải đi kèm selection có provenance đúng; P0 luôn được giữ kể cả khi thiếu trong mandatory_ids.

Exact budgeting đo toàn bộ prompt sau mỗi lần loại nhóm, tối đa số nhóm optional +1 lần đo. Không dùng estimated_tokens làm quyết định an toàn. Callback compile/measure lỗi được làm sạch trong đường mới, không đưa prompt/path vào lỗi. Khi nội dung bắt buộc quá lớn trả RequiredContentTooLarge; đầu vào/kết quả rỗng trả EmptyPrompt. Runtime callback phải dùng compiler/tokenizer đang nạp; test hiện tại chỉ dùng callback đo xác định để kiểm thuật toán.

### Phân tích ảnh hưởng và phạm vi chưa làm
Ngoài lib.rs và agent/graph/pipeline.rs, tìm thấy caller budget/fallback trong `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\commands\llm.rs` và `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\websocket\dialogue.rs`. Ghi nhận để rà scope Bước 5; chưa sửa cả bốn caller. Guard cuối vẫn bảo vệ inference, nhưng fallback cũ không được tuyên bố đã loại bỏ.

Không thay schema IPC, quyền tool, DB, dependency/manifest/lockfile, capture/resize, model mặc định, template compiler. Không tạo runtime model suites của giai đoạn sau. API ước lượng cũ còn tương thích; sửa constructor ảnh hưởng caller hiện hữu ở context nhỏ, không đồng nghĩa caller đã chuyển sang exact-budget.

### Kết quả kiểm chứng mới
Working directory: `E:\Project\01_AI_Agents\LIVA`. Cargo chạy tuần tự, --locked --offline -j 2; test dùng -- --test-threads 2; RAM >=4 GiB trước chạy.

| Lệnh/phạm vi | Kết quả cuối |
|---|---|
| cargo test -p liva-native-core --test dynamic_prompt_assembly_tests | 26 pass / 0 fail / exit 0 |
| cargo test -p liva-native-core --test vision_context_guard_tests | 21 pass / 0 fail / exit 0 |
| cargo test -p liva-native-core --test runtime_stability_tests | 3 pass / 0 fail / exit 0 |
| cargo test -p liva-native-core --lib llm::engine::tests::guard_ | 5 pass / 0 fail / 734 filtered / exit 0 |
| cargo check -p liva-native-core --all-targets | exit 0 |
| cargo clippy -p liva-native-core --lib --test dynamic_prompt_assembly_tests -- -D warnings | exit 0 |

Tổng 55 test chọn lọc pass, không phải toàn bộ workspace. Cảnh báo future-incompatibility proc-macro-error2 v2.0.1 còn tồn tại, không nâng dependency để che cảnh báo.

Snapshot và log: `C:\Users\Admin\AppData\Local\Temp\liva-budget-step34-20260917-104954`. Có bản gốc ba file sửa, hashes-before.json, staged-before.patch/unstaged-before.patch; log cuối prompt-final, vision-final, stability-final, guard-final, check-final, clippy-final và file .exit tương ứng. Một số tool call báo terminal closed, nhưng log test và file exit cùng lượt xác nhận Cargo kết thúc 0. Không chạy lại chỉ vì nhãn terminal nếu đã có exit xác nhận.

Đã đối chiếu toàn bộ staged diff trước/sau: không đổi. Các thay đổi có sẵn ngoài phạm vi (PROJECT.md, model submodule) không bị sửa/rollback. Bước 3–4 đã có mã và kiểm chứng model-free; Bước 5+, runtime tokenizer/mtmd, CUDA/release và benchmark vẫn chưa thực hiện.
## 8. Bước 5–6 — exact-budget callers, streaming cancellation, vision mtmd guard (17/09/2026)

### Thay đổi thực hiện
- `liva-native-core/src/llm/engine.rs`: helper production `CompletionStream` cho callback streaming — kiểm tra kênh đóng/lỗi gửi kể cả với heartbeat rỗng (trước đây heartbeat rỗng luôn trả `true` nên client đóng kết nối không bị phát hiện cho tới chunk hiển thị kế tiếp); `finish` từ chối bọc kết quả thành công khi stream đã hủy; `answer_with_image` đo tổng `chunks.total_tokens()` sau compile qua `compile_exact_budgeted_prompt` (persona + câu hỏi hiện tại là mandatory, không đo text-only thay multimodal, không bỏ ảnh/câu hỏi), thay literal 512 của vòng sinh vision bằng `RESERVE_FOR_COMPLETION`, bỏ hai cast dư `usize` theo clippy.
- `liva-native-core/src/lib.rs`, `liva-native-core/src/commands/llm.rs`, `liva-native-core/src/websocket/dialogue.rs`: caller stream/non-stream chuyển qua `CompletionStream`; không thêm chunk wire, hợp đồng IPC giữ nguyên.
- `liva-native-core/src/agent/graph/pipeline.rs`: giữ nguyên `send_llm_chunk_if_current`/`finish_streamed_completion` (session cancellation + backpressure + Closed đã có); kiểm tra kênh đóng diễn ra ở chunk hiển thị kế tiếp.
- `liva-native-core/tests/completion_stream_tests.rs` (mới): 4 test — heartbeat kênh đóng phải hủy (xác minh ĐỎ exit 101 trước khi sửa), heartbeat kênh mở không phát chunk, wire shape giữ nguyên, lỗi inference không bị bọc thành công.
- `vendor/proc-macro-error2/` + `[patch.crates-io]` trong `Cargo.toml`: bản sao 2.0.1 đã xuất bản (checksum khớp Cargo.lock cũ) với đúng một thay đổi `pub extern crate proc_macro` cho E0365 (rust-lang/rust#127909); license MIT/Apache-2.0 giữ nguyên; `vendor/proc-macro-error2/LIVA-PATCH.md` ghi nguồn/checksum/điều kiện gỡ. Không sửa Cargo cache; không che cảnh báo.

### Kết quả kiểm chứng (debug, model-free, tuần tự -j 2 / test-threads 2, RAM >=4 GiB trước mỗi lượt)
| Lệnh/phạm vi | Kết quả cuối |
|---|---|
| cargo check -p liva-native-core --all-targets --locked --offline -j 2 | exit 0, không còn future-incompatibility |
| cargo clippy -p liva-native-core --all-targets --message-format=short --locked --offline -j 2 | exit 0 |
| cargo clippy -p liva-native-core --lib --test completion_stream_tests --test budgeted_completion_entry_tests -- -D warnings | exit 0 |
| cargo test … --test dynamic_prompt_assembly_tests | 26 pass / 0 fail |
| cargo test … --test budgeted_completion_entry_tests | 2 pass / 0 fail |
| cargo test … --test completion_stream_tests | 4 pass / 0 fail |
| cargo test … --test vision_context_guard_tests | 21 pass / 0 fail |
| git diff --check | exit 0 |
| Scan residual fallback/budget-API cũ trong 4 caller | 0 kết quả |

Regression heartbeat đã được xác minh đỏ (exit 101, assertion failure) trước khi sửa, xanh sau khi sửa.

### Chưa thực hiện — không ghi DONE
- Release/CUDA, benchmark P50/P95 trước/sau, smoke transport thật (chat:completion, graph text), kiểm stream end-to-end qua IPC/WebSocket thật.
- `ChatMessage` không có call IDs: atomic grouping bảo toàn turn, không xác thực toàn bộ giao thức tool-call.
- Snapshot backup: `C:\Users\Admin\AppData\Local\Temp\liva-step56-20260917-121713` (Cargo.toml, Cargo.lock, lib.rs, engine.rs, commands/llm.rs, dialogue.rs, pipeline.rs, acceptance doc; staged patch). Git boundary: chưa `git add`; mọi commit/merge là hành động của người dùng.

## 9. Bước 7 — runtime suites opt-in (17/09/2026)

### File mới
- `liva-native-core/tests/prompt_budget_runtime.rs`: một test tuần tự chạy đường manager entry thật trên model thật — hội thoại dài (selector giữ system + current user, loại nhóm lịch sử nguyên tử; prompt_tokens là phép đo compiled prompt nằm trong biên strict), oversized mandatory → `Required prompt content exceeds budget` với **0 token phát ra**, cancellation tại token hiển thị đầu (trả Ok một phần), request kế tiếp sau cancellation vẫn chạy được.
- `liva-native-core/tests/vision_budget_runtime.rs`: đường mtmd thật trên release — oversized question bị từ chối bởi phép đo `chunks.total_tokens()` (không phải ước lượng text-only) với 0 token phát ra; encoded bytes hỏng đi qua decoder thật (stb_image) trả lỗi sạch; request hợp lệ sau hai lỗi vẫn chạy end-to-end và báo tổng multimodal trong biên. Fixture chỉ là khối màu đặc, không chứa dữ liệu cá nhân.

### Hành vi thiếu fixture — red-gate theo spec [Testing] 3
- Chạy `--ignored` khi `LIVA_TEST_MODEL_PATH` chưa đặt → **FAIL exit 101** với thông báo cấu hình (`LIVA_TEST_MODEL_PATH is not set…`), không return-Ok giả PASS. Test vision tự panik nếu build debug trên Windows (chặn chủ ý trong engine) và khi thiếu biến fixture.

### Kết quả chạy thật
| Lệnh | Kết quả |
|---|---|
| cargo test … --test prompt_budget_runtime --test vision_budget_runtime (build, không fixture) | build exit 0 |
| cargo test … --test prompt_budget_runtime --test vision_budget_runtime -- --ignored (thiếu fixture) | exit 101, thông báo cấu hình — đúng yêu cầu fail-loud |
| cargo test -p liva-native-core --release --test prompt_budget_runtime --locked --offline -j 2 -- --ignored --test-threads 2, `LIVA_TEST_MODEL_PATH=E:\AI_Models\gemma-4-E2B-it-Q4_K_M.gguf` (2,89 GiB, CPU, full load) | **1 passed / 13,51 s / exit 0** |
| rustfmt --edition 2024 (2 file mới) | exit 0 |
| cargo clippy -p liva-native-core --test prompt_budget_runtime --test vision_budget_runtime -- -D warnings | exit 0 |

### Giới hạn còn lại
- **Vision runtime acceptance BLOCKED**: không tìm thấy mmproj nào trên máy (`E:\AI_Models` không có file `*mmproj*`; thư mục Qwen3-VL không còn). Suite đã sẵn sàng; khi có cặp model VL + mmproj khớp, chạy `cargo test --release --test vision_budget_runtime -- --ignored`. Không đo được `chunks.total_tokens` thực trên projector thật → không khẳng định cap 2048 được hỗ trợ.
- Chưa chạy: CUDA, benchmark P50/P95 trước/sau, smoke transport IPC/WebSocket thật.

### Bằng chứng gốc (raw output, lượt chạy 3)
- File log nguyên văn: `C:\Users\Admin\AppData\Local\Temp\liva-pbr-raw.txt`.
- Dòng kết quả nguyên bản: `running 1 test` → `test runtime_exact_budget_inference_cancellation_and_recovery ... ok` → `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.66s` → exit **0** (profile `release`, binary `target\release\deps\prompt_budget_runtime-*.exe`, model loader ghi nhận gemma4/Gemma-4-E2B-It 601 tensors).

### Smoke transport thật (17/09/2026)
- `node scripts/e2e-test-suite.mjs` — spawn binary release thật (`target\release\liva-native-core.exe`), socket TCP WebSocket thật, §1–7: **36 pass / 0 fail / 1 skip (skip được báo đúng là skip), exit 0**. Mục 4 `chat:completion` Protocol & Streaming: 4/4 đạt (1 skip có chủ ý) — payload validation, streaming chunk order, error envelope khi không có model. Mục 5 yêu cầu "chứng minh caller đi qua manager entry mới; kiểm stream/nonstream và error envelope" đã được thỏa ở tầng transport; log: `%TEMP%\liva-e2e-smoke.log`.
- Ranh giới: đây là smoke socket thật trên môi trường không model; không phải benchmark và không thay vision desktop manual check (chỉ chạy thủ công trên fixture được phép).


