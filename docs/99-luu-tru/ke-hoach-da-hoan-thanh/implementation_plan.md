# Implementation Plan

[Overview]
Hoàn thiện Vision Context Guard & Dynamic Prompt Budgeting bằng ngân sách token thực tế, bảo toàn nội dung bắt buộc và kiểm thử đi qua mã production trước khi nghiệm thu.

## Phạm vi và trạng thái
- Ngày khảo sát: 17/09/2026. HEAD quan sát: `67eb98b`; working tree có nhiều thay đổi staged/unstaged, kể cả các file thuộc kế hoạch. HEAD không đại diện toàn bộ mã đã khảo sát. Không reset, unstage, ghi đè hoặc commit thay đổi của người dùng.
- Đây là kế hoạch đề xuất, không phải báo cáo đã triển khai. Chưa chạy compiler, test hoặc benchmark trong phiên lập kế hoạch. Các số 942 test, P90 <480ms và kết quả ngày 06/09 trong tài liệu cũ không phải bằng chứng nghiệm thu hiện tại.
- Chỉ thay backend Rust và test/tài liệu liên quan. Không thay Node/Python legacy, DB/WAL, phân quyền, model mặc định, avatar, voice latency hay cấu hình UI.
- Giữ nguyên hợp đồng IPC/WebSocket và `CompletionOutput`. Không tự tăng context window, giảm completion reserve, tải model hay thay model để làm test xanh.

## Bằng chứng từ mã hiện tại
1. `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\engine.rs`, `answer_with_image`, dòng 574–580: `image_min_tokens=-1`, `image_max_tokens=2048`; dòng 641–647: `chunks.total_tokens()` được kiểm bởi `check_prompt_fits` TRƯỚC `eval_chunks`. Vì vậy giả thuyết “vision chưa có guard” không còn đúng với working tree này. Chưa xác minh trên model thật rằng giới hạn 2048 được projector thực thi.
2. Cùng file, `check_prompt_fits`: điều kiện hiện hành là `prompt_tokens + 512 < n_ctx`; tổng bằng n_ctx phải bị từ chối. Giữ nguyên biên này.
3. `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\prompt\dynamic_prompt.rs`: `PromptSlice::new` dùng byte/4 +4; `assemble_messages` cộng ước lượng, không đo template/tokenizer thực. `reserve_response_tokens` không tham gia trực tiếp kiểm giới hạn tổng của hàm này.
4. `budget_chat_messages` bảo vệ system đầu tiên bằng P0 nhưng chỉ xếp user cuối vào P1 và vẫn có thể loại nó. Nếu message cuối là assistant/tool, user mới nhất thậm chí không được nhận diện đặc biệt.
5. `PromptBudget::for_dialogue` và `from_context_window` dùng mức sàn 512/256 hoặc 256/128; với context nhỏ, mức sàn có thể vượt phần còn lại.
6. `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\lib.rs`, `handle_chat_completion_scoped`, dòng 583–590 và `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\agent\graph\pipeline.rs`, `build_pipeline_graph`, dòng 460–475: `.unwrap_or_else(|_| messages.clone())` bỏ qua thất bại budgeting. Việc lấy n_ctx, compile và inference xảy ra ở các lần giữ lock khác nhau.
7. `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\vision_context_guard_tests.rs` tự định nghĩa clamp/slice limit; test KV reset chỉ clear Vec cục bộ; test corrupted PNG chỉ kiểm byte. Những test đó không chứng minh engine xử lý tương ứng.
8. `E:\Project\01_AI_Agents\LIVA\PROJECT.md` vẫn ghi nhiều milestone DONE, trái với mô tả trong tài liệu tồn đọng. Phải tách lịch sử khỏi nghiệm thu mới, không đổi tất cả milestone không liên quan.

## Nguyên tắc giải pháp
Giữ guard cuối tại engine cho tất cả caller. Bổ sung bước dựng prompt có đo chính xác sau template: tokenizer đang nạp cho text; `mtmd.tokenize(...).total_tokens()` cho vision. Ước lượng chỉ dùng xếp hạng/tiền xử lý, không quyết định an toàn. Khi không thể giữ system bắt buộc + câu hỏi hiện tại + ảnh + phần trả lời, trả lỗi có cấu trúc thay vì âm thầm bỏ câu hỏi/ảnh hoặc dùng lại prompt quá dài.

Không hứa tự giảm độ phân giải hay số image token trong đợt đầu: không có bằng chứng rằng mọi projector hỗ trợ cùng chính sách. Giữ cap 2048 hiện có, coi đó là gợi ý upstream; kiểm tổng mtmd thực là thẩm quyền cuối. Tối ưu resize thích nghi chỉ mở thành đợt riêng sau benchmark chất lượng OCR/vision.

## Điều kiện công cụ
Đã tìm và đọc trực tiếp RFC 3 trong `E:\Project\01_AI_Agents\LIVA\teamwork_projects\obsidian_llm_wiki\vault\Knowledge\deepseek_harness_integration.md`. Đây là tài liệu định hướng, không phải chứng cứ runtime. Tool `search_vault` và các tool impact/PDG không được cung cấp trong phiên; `rg` cũng không có trên PATH, nên khảo sát dùng đọc file và PowerShell Select-String có phạm vi. Trước sửa mã, phải có `search_vault` theo AGENTS.md hoặc xin chấp thuận thay thế rõ ràng; không giả nhận đã gọi tool. Không khởi động indexer nặng để bù thiếu công cụ.

[Types]
Các kiểu mới bên dưới đặt tại `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\prompt\dynamic_prompt.rs`, chỉ nội bộ Rust, không thêm schema IPC:

1. `ContextTokenBudget { n_ctx: usize, reserve_response_tokens: usize }`, derive Debug/Clone/Copy/PartialEq/Eq. Constructor trả Result; n_ctx và reserve phải >0, phải còn ít nhất một token prompt. `max_prompt_tokens = n_ctx.checked_sub(reserve).and_then(|n| n.checked_sub(1))`. Không dùng phép cộng có thể overflow. Runtime dùng reserve 512 hiện có.
2. `BudgetedPrompt { prompt: String, prompt_tokens: usize, dropped_slice_ids: Vec<String> }`, derive Debug/Clone. `prompt_tokens` là số đo toàn bộ prompt đã compile; với vision bao gồm ảnh, không gắn nhãn text-only.
3. `PromptSelection { mandatory_ids: Vec<String>, atomic_groups: Vec<Vec<String>> }`, derive Debug/Clone/Default. ID phải tồn tại và duy nhất. Mỗi slice thuộc tối đa một nhóm; nhóm chứa slice mandatory thì cả nhóm mandatory. Dùng nhóm để giữ nguyên các turn lịch sử, không giữ tool result mồ côi. Không thay layout `PromptSlice` hiện có để tránh phá struct literals và serde.
4. Mở rộng `PromptAssemblyError`: `InvalidSelection(String)`, `TokenizationError(String)`, `RequiredContentTooLarge { required_tokens: usize, max_prompt_tokens: usize }`. Giữ các variant cũ; cập nhật Display và mọi match exhaustive. Lỗi không chứa prompt, ảnh, facts hoặc đường dẫn model nhạy cảm.
5. Giữ `PromptBudget`, `SlicePriority`, `SkillDefinition` và `CompletionOutput` tương thích nguồn. Các API ước lượng cũ được ghi rõ không cung cấp bảo đảm exact-token. Không thêm enum priority mang tính bắt buộc: tính mandatory độc lập với thứ tự ưu tiên.

[Files]
## File mới
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\prompt_budget_runtime.rs`: tokenizer thật và đường manager; fixture model opt-in.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\vision_budget_runtime.rs`: regression release/model thật cho mtmd, guard và phục hồi request kế tiếp.
- `E:\Project\01_AI_Agents\LIVA\docs\03-danh-gia\vision-prompt-budget-acceptance.md`: bằng chứng mới, câu lệnh, model/build fingerprint, kết quả đo và phần chưa chạy.

## File sửa
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\prompt\dynamic_prompt.rs`: kiểu/API exact-budget, chọn nhóm nguyên tử, validation, bỏ mức sàn vượt ngân sách, unit test thuần.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\engine.rs`: budget chung trong guard; entry cho messages; budget vision bằng mtmd; thay literal 512 ở giới hạn sinh vision bằng RESERVE_FOR_COMPLETION; log tổng hợp.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\lib.rs`: handle_chat_completion_scoped dùng entry mới dưới cùng manager lock, bỏ fallback bỏ qua budgeting; lấy model_id tương ứng request trong cùng critical section.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\agent\graph\pipeline.rs`: closure LLM trong build_pipeline_graph; giữ session cancellation, backpressure và persistence; ghi model_id đúng model đang inference.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\dynamic_prompt_assembly_tests.rs`: giữ test API cũ, thêm test exact-budget có tên phản ánh mức kiểm chứng.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\vision_context_guard_tests.rs`: giữ unit test check_prompt_fits/nen_sinh_tiep; bỏ helper/test tự xác nhận clamp, slice, Vec/KV và PNG byte; chuyển yêu cầu hành vi thật sang vision_budget_runtime.rs.
- `E:\Project\01_AI_Agents\LIVA\PROJECT.md`, `E:\Project\01_AI_Agents\LIVA\TEST_READY.md`, `E:\Project\01_AI_Agents\LIVA\TEST_INFRA.md`: thêm trạng thái/bằng chứng phạm vi này, bảo toàn cảnh báo thu hồi test cũ.

## File giữ nguyên nhưng phải regression
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\websocket.rs`: caller answer_with_image giữ signature/event.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\bin\gemma4_probe.rs`, `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\bin\qwen3vl_probe.rs`: caller vision trực tiếp phải compile.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\vision\capture.rs`: giữ crop/ROI, không resize dựa trên giả định.
Không xóa/move file production; không sửa manifest/config/lockfile trong phạm vi cơ sở.

[Functions]
Đường dẫn đầy đủ của từng file bên dưới đã khai báo tại [Files].

## API mới — dynamic_prompt.rs
- `ContextTokenBudget::new(n_ctx: usize, reserve_response_tokens: usize) -> Result<Self, PromptAssemblyError>`: validate và kiểm số học.
- `ContextTokenBudget::max_prompt_tokens(&self) -> usize`: giới hạn strict đã validate.
- `DynamicPromptAssembler::select_chat_messages(messages: &[ChatMessage]) -> Result<(Vec<PromptSlice>, PromptSelection), PromptAssemblyError>`: mọi system message không rõ nguồn là mandatory, không tự hạ quyền nội dung. Giữ user gần nhất và toàn bộ suffix sau nó mandatory. Nhóm lịch sử trước đó theo user-turn, không tách call/result. Không có user trả EmptyPrompt. Memory chỉ optional khi có slice/provenance rõ, không đoán theo nội dung string.
- `DynamicPromptAssembler::compile_exact_budgeted_prompt<C, M>(slices: &[PromptSlice], selection: &PromptSelection, budget: &ContextTokenBudget, compile: C, measure: M) -> Result<BudgetedPrompt, PromptAssemblyError> where C: Fn(&[ChatMessage]) -> Result<String, String>, M: FnMut(&str) -> Result<usize, String>`: validate IDs/groups; compile chronological; đo toàn prompt; quá trần thì loại nhóm optional P4 → P3 → P2 → P1, cùng priority loại cũ trước. Nhóm nhiều priority lấy mức bảo vệ cao nhất. Đo lại sau mỗi lần loại, không giả định token cộng tuyến tính. Tối đa số nhóm optional +1 lần đo; hết nhóm trả RequiredContentTooLarge. Callback production dùng compiler/tokenizer thật; callback deterministic chỉ test thuật toán.

## API mới — engine.rs
- `LlamaRouterManager::generate_budgeted_completion<F>(&mut self, messages: &[super::ChatMessage], temperature: f32, top_p: f32, token_callback: F) -> Result<CompletionOutput, String> where F: FnMut(&str) -> bool`: đọc model/n_ctx dưới lock đang giữ; selector + compiler hiện hành + model.str_to_token(prompt, AddBos::Always); gọi generate_completion. Guard cuối còn nguyên. Ban đầu chấp nhận tokenize lại một lần để tránh thay KV/cache; chỉ tối ưu bỏ trùng sau benchmark.

## API sửa, không đổi signature ngoài
- `check_prompt_fits` (engine.rs): dùng ContextTokenBudget với RESERVE_FOR_COMPLETION, giữ biên strict và lỗi chẩn đoán tương thích; test overflow/zero.
- `LlamaRouterManager::answer_with_image` (engine.rs): persona + user chứa marker ảnh là mandatory; closure compile_prompt và mtmd.tokenize đo chính xác; bitmap sống xuyên tokenization. Chưa có optional slices thì quá ngân sách trả lỗi, không xóa persona/câu hỏi/ảnh. Tokenize bản cuối để lấy chunks cho eval, kiểm tổng lần cuối trước eval; không lấy token text thay tổng multimodal. Giữ Windows debug guard, callback cancellation, last_tokens.clear và KV reset. Request sau lỗi không tái sử dụng prefix vision không hợp lệ.
- `PromptBudget::from_context_window`, `for_dialogue` (dynamic_prompt.rs): bỏ mức sàn vượt available, tránh overflow; context không hợp lệ dẫn budget 0 và lỗi ở consumer. API exact-budget mới là đường runtime chính.
- `handle_chat_completion_scoped` (lib.rs): bỏ compile/budget ngoài lock và fallback; move messages vào spawn_blocking, gọi generate_budgeted_completion cho stream/nonstream. Lỗi qua nhánh lỗi hiện có, không tạo success giả.
- `build_pipeline_graph` (pipeline.rs): closure text gọi generate_budgeted_completion; giữ checks session trước/sau lock và callback timeout. Vision closure giữ answer_with_image, hưởng guard mới tự động.
Không xóa API public trong đợt này. Không thay compiler/template format hoặc mở rộng quyền tool. API assemble_prompt/assemble_messages/compile_budgeted_prompt/budget_chat_messages cũ còn phục vụ compatibility và unit test nhưng không được coi là guard exact-token.

[Classes]
Rust không có class theo nghĩa OOP. Mở rộng impl `DynamicPromptAssembler`, `PromptBudget`, `LlamaRouterManager` như [Functions]; thêm struct `ContextTokenBudget`, `BudgetedPrompt`, `PromptSelection` như [Types]. Không inheritance, service nền hoặc trait framework. Giữ `LlamaEngine` và `VisionImage` tương thích.

[Dependencies]
Không thêm package/nâng version. Dùng llama-cpp-2 đã khai báo 0.1.154 với feature mtmd, std, serde, tracing, Tokio hiện có. Không tự sửa Cargo.lock. Các API str_to_token, MtmdInputText, tokenize, total_tokens, eval_chunks đã có call site production nên không giả định thêm API upstream. Fixture chỉ dùng model/mmproj local được cho phép; test không tải mạng/ghi DB thật. Không cần thay codecs hoặc cấu hình image_min_tokens cho đợt này.

[Testing]
## 1. Preflight và baseline
- Ghi SHA-256 các file thuộc phạm vi, git diff staged/unstaged, cấu hình build/model (không đọc secret), RAM khả dụng và tiến trình cargo/rustc hiện hữu. Nếu file đổi trong khi đo, hủy kết luận baseline và đồng bộ lại với người đang làm.
- RAM khả dụng >=4 GiB trước test; nếu thiếu, dừng và ghi BLOCKED, không tự dừng tiến trình người dùng. Không build/test đồng thời; không cargo clean hoặc thêm target directory.
- Ghi rõ Windows vision debug bị chặn chủ ý trong engine: test debug không thể chứng minh mtmd thực thi.

## 2. Unit test bắt buộc
Trong `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\dynamic_prompt_assembly_tests.rs` và module test của dynamic_prompt.rs:
- Biên n_ctx=0, reserve=0, n_ctx<=reserve+1, usize::MAX; không panic/overflow. Với 4096/512: 3583 nhận, 3584 từ chối.
- Byte/4 đánh giá thấp tiếng Việt/emoji/template: hàm exact vẫn loại đúng theo callback đo; ghi rõ đây là unit test không phải tokenizer thật.
- User dài hơn ngân sách bắt buộc trả lỗi, không sinh câu trả lời cho system-only; user gần nhất được giữ cả khi suffix assistant/tool xuất hiện.
- System constraints không mất; nhóm lịch sử atomic; ID trùng/không tồn tại/nhóm chồng nhau bị từ chối; thứ tự ổn định và không ghép tool result mồ côi.
- Sai số estimated_tokens cố ý, template overhead, tokenizer callback lỗi, compile lỗi, không có user, đầu vào rỗng, không còn optional groups.
- Giới hạn số lần compile/measure <= optional-group-count+1; xóa slice làm token không giảm vẫn kết thúc hữu hạn.

## 3. Runtime tests có model thật
Trong `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\prompt_budget_runtime.rs`:
- Dùng model local, kiểm compile bằng template đang nạp + str_to_token thật. Hội thoại dài phải giữ nguyên current user/system, số token báo cáo bằng phép đo độc lập trên prompt cuối.
- Gọi generate_budgeted_completion và kiểm prompt_tokens, nội dung đầu ra không rỗng, cancellation và request kế tiếp. Test tokenizer-only không được ghi là inference.
- Model swap rồi request dùng đúng n_ctx/tokenizer của model đang giữ lock; không đổi global template từ các test chạy song song. Gom ca model thật vào một test tuần tự hoặc dùng mutex dùng chung, không spawn model workers.

Trong `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\vision_budget_runtime.rs`:
- Release Windows; RGB và PNG sinh từ fixture không chứa dữ liệu cá nhân. Dùng model/mmproj local matching, không tự chụp màn hình người dùng.
- Prompt quá dài gọi answer_with_image thật phải trả lỗi ngân sách, không phát token; request hợp lệ sau đó vẫn chạy. Giữ kiểm thứ tự guard trước eval trong review; runtime error đơn lẻ không chứng minh native eval chưa từng chạy.
- Ca text ngắn + ảnh thường; text dài + ảnh; input thiếu mmproj; corrupted encoded bytes gọi decoder thật; cancellation rồi text/vision tiếp theo. RGB invalid chỉ thêm khi đã xác minh binding trả lỗi trước FFI, không chủ động gọi native bằng buffer có thể gây unsafe access.
- Ghi chunks.total_tokens thực; không dùng hằng 2048/7 slices/64 min để thay số đo. Không khẳng định projector cap được hỗ trợ nếu chưa đo.
- Test cần model đánh dấu ignore rõ ràng. Khi gọi explicit --ignored mà thiếu fixture phải fail với thông báo cấu hình, không return Ok và giả PASS. Chỉ acceptance đã chạy đủ mới được ghi DONE.

## 4. Lệnh validation sau triển khai
Chạy từng lệnh riêng, chỉ chuyển bước sau khi lệnh trước kết thúc. Đây là lệnh dự kiến, chưa được chạy trong phiên lập kế hoạch:

```powershell
Set-Location 'E:\Project\01_AI_Agents\LIVA'
Get-CimInstance Win32_OperatingSystem | Select-Object @{N='FreeGiB';E={[math]::Round($_.FreePhysicalMemory / 1MB, 2)}}
Get-Process cargo,rustc -ErrorAction SilentlyContinue
cargo fmt --all -- --check
cargo check -p liva-native-core --all-targets --locked --offline -j 2
cargo test -p liva-native-core --test dynamic_prompt_assembly_tests --locked --offline -j 2 -- --test-threads 2
cargo test -p liva-native-core --test vision_context_guard_tests --locked --offline -j 2 -- --test-threads 2
cargo test -p liva-native-core --lib --locked --offline -j 2 -- --test-threads 2
cargo test -p liva-native-core --release --test prompt_budget_runtime --locked --offline -j 2 -- --ignored --test-threads 2
cargo test -p liva-native-core --release --test vision_budget_runtime --locked --offline -j 2 -- --ignored --test-threads 2
cargo clippy -p liva-native-core --all-targets --locked --offline -j 2 -- -D warnings
cargo test --workspace --locked --offline -j 2 -- --test-threads 2
git --no-pager diff --check
```
Runtime tests mới đọc fixture từ `LIVA_TEST_MODEL_PATH`, `LIVA_TEST_MMPROJ_PATH`, chỉ tồn tại trong test, không thay cấu hình ứng dụng. Offline thiếu dependency thì ghi BLOCKED, không fetch ngầm. CUDA là lượt kiểm riêng với `--features cuda` nếu môi trường hiện có hỗ trợ, không build CUDA và CPU đồng thời.

## 5. Hiệu năng và tiêu chí chấp nhận
- Đo cùng model/context/build/hardware và tập prompt trước/sau: 5 warm-up, 30 mẫu tuần tự cho prompt ngắn/dài, tiếng Việt và vision fixture; cold projector load ghi riêng. Ghi thời gian assembly, số lần tokenize, prompt tokens, TTFT và RAM/VRAM đỉnh nếu đo được. Không log prompt, ảnh, khóa hoặc facts.
- Báo P50/P95 kèm số mẫu; baseline chỉ có ý nghĩa khi cùng cấu hình. Mức tăng P95 assembly/TTFT >10% cần điều tra và quyết định rõ, không tự tăng ngưỡng hay tuyên bố đạt SLA voice/vision cũ.
- Bắt buộc: không inference khi vượt ngân sách; không mất nội dung mandatory; không fallback bỏ qua lỗi; stream kết thúc/hủy đúng hợp đồng; request hợp lệ sau lỗi chạy được; không thay quyền IPC/tool hoặc dependency.
- Smoke test entry chat:completion và graph text trên môi trường test cô lập để chứng minh caller đi qua manager entry mới; kiểm stream/nonstream và error envelope hiện có. Socket vision có chụp desktop chỉ kiểm thủ công trên màn hình fixture được cho phép, không tự chạy trên desktop chứa dữ liệu cá nhân. Chưa chạy transport thật thì ghi thiếu coverage, không gọi unit test là E2E.

[Implementation Order]
1. Xác nhận người dùng cho phép triển khai; giải quyết điều kiện search_vault/impact hoặc chấp thuận thay thế. Chụp diff/hash scoped và kiểm RAM/process; không chạm staging của người dùng.
2. Ghi baseline các test hiện có trong phạm vi và trạng thái fixture CPU/release/CUDA. Nếu fixture thiếu, tách phần model-free khỏi nghiệm thu runtime.
3. Thêm ContextTokenBudget và test biên số học; dùng nó trong check_prompt_fits mà không thay semantics strict.
4. Thêm selector, atomic groups, exact compile/measure và unit tests; giữ API ước lượng cũ tương thích, sửa mức sàn budget.
5. Thêm generate_budgeted_completion; nối handle_chat_completion_scoped và graph closure; bỏ fallback, lấy model identity trong cùng lock. Regression stream/cancellation trước sang vision.
6. Hoàn thiện answer_with_image dùng tổng mtmd thực và reserve chung; giữ cap/projector/capture hiện hành. Không thêm adaptive resize trong đợt này.
7. Thay test vision tự xác nhận bằng test production phù hợp; thêm runtime suites opt-in và xác nhận lỗi không bị biến thành PASS/SKIP ngầm.
8. Chạy validation tuần tự, runtime release và benchmark trước/sau. Thiếu model/hardware/công cụ ghi BLOCKED; lỗi ngoài phạm vi không sửa tùy tiện.
9. Cập nhật acceptance report, PROJECT.md, TEST_READY.md, TEST_INFRA.md chỉ theo bằng chứng mới; đọc lại mọi file sửa, kiểm diff giới hạn phạm vi, bàn giao phần còn thiếu.

Rollback: sửa ngược đúng các hunk tạo trong phiên theo baseline đã lưu, không git reset/revert cả file vì có thay đổi của người dùng. Không commit/push/fetch/merge hoặc thay nhánh. Không ghi DONE trước khi các ca runtime bắt buộc thực sự được chạy và đạt.
