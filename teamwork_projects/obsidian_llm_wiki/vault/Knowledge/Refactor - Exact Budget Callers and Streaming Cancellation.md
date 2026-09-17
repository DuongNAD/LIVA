---
title: "Refactor - Exact Budget Callers and Streaming Cancellation"
tags: [liva/knowledge, liva/refactor, code/graph]
author: "codex"
last_update: "2026-09-17"
---

# Exact-budget callers, streaming cancellation, vision mtmd guard — Bước 5–6

Phiên 17/09/2026: người dùng phê duyệt Bước 5–6 của `implementation_plan.md` kèm waiver cho search_vault/GitNexus/PDG, và cho phép bản sửa dependency tối thiểu có kiểm chứng (không che cảnh báo, không sửa Cargo cache).

## Thay đổi mã

- `liva-native-core/src/llm/engine.rs`: thêm `CompletionStream` — helper production cho callback streaming: kiểm tra `is_closed`/lỗi gửi kể cả với heartbeat rỗng, không thêm chunk wire, `finish` từ chối bọc thành công khi stream đã hủy; `answer_with_image` đo `chunks.total_tokens()` sau template qua `compile_exact_budgeted_prompt` (persona + câu hỏi là mandatory, không đo text-only, không bỏ ảnh), thay literal 512 vòng sinh vision bằng `RESERVE_FOR_COMPLETION`, bỏ hai cast dư `usize` theo clippy.
- `lib.rs`, `commands/llm.rs`, `websocket/dialogue.rs`: caller stream/non-stream chuyển qua `CompletionStream`; pipeline graph giữ nguyên cơ chế session-cancellation/backpressure hiện có (`send_llm_chunk_if_current`, `finish_streamed_completion`).
- `tests/completion_stream_tests.rs` (mới): 4 test — heartbeat kênh đóng phải hủy (xác minh ĐỎ exit 101 trước khi sửa), heartbeat kênh mở không phát chunk nhưng vẫn qua callback, wire shape giữ nguyên, lỗi inference không bị bọc thành công.
- `vendor/proc-macro-error2` + `[patch.crates-io]`: bản sao 2.0.1 đã xuất bản (checksum khớp Cargo.lock cũ) với đúng một thay đổi `pub extern crate proc_macro` cho E0365 (rust-lang/rust#127909); license MIT/Apache-2.0 giữ nguyên; `vendor/proc-macro-error2/LIVA-PATCH.md` ghi nguồn, checksum, điều kiện gỡ. Không sửa Cargo cache.

## Kiểm chứng (debug, model-free, tuần tự -j 2 / test-threads 2)

- `cargo check -p liva-native-core --all-targets --locked --offline -j 2`: exit 0, không còn future-incompatibility.
- `cargo clippy -p liva-native-core --all-targets --message-format=short`: exit 0; bộ lib + 2 test target với `-D warnings`: exit 0.
- 53 test pass: dynamic_prompt_assembly 26, budgeted_completion_entry 2, completion_stream 4, vision_context_guard 21.

## Giới hạn — không ghi DONE

- Chưa chạy runtime suites opt-in (`prompt_budget_runtime`, `vision_budget_runtime` — cần `LIVA_TEST_MODEL_PATH`/`LIVA_TEST_MMPROJ_PATH`, vision bắt buộc release Windows). Chưa chạy release/CUDA, benchmark P50/P95, smoke transport thật.
- `ChatMessage` không có call IDs: grouping bảo toàn turn, không xác thực toàn bộ giao thức tool-call.
- Báo cáo nghiệm thu: `docs/03-danh-gia/vision-prompt-budget-acceptance.md`, mục 8. Snapshot backup: `C:\Users\Admin\AppData\Local\Temp\liva-step56-20260917-121713`.

## Bu?c 7 � runtime suites opt-in (c�ng ng�y)

- `tests/prompt_budget_runtime.rs` + `tests/vision_budget_runtime.rs` (m?i, `#[ignore]` + fail-loud khi thi?u fixture: x�c minh exit 101 v?i th�ng b�o c?u h�nh, kh�ng fake-pass).
- Text-path runtime th?t: `cargo test --release --test prompt_budget_runtime -- --ignored` v?i gemma-4-E2B-it-Q4_K_M ? 1 passed / 13,51 s (inference, oversized rejection 0-token, cancellation + recovery).
- Vision runtime BLOCKED: kh�ng c� mmproj tr�n m�y; suite s?n s�ng, ch?y khi c� c?p VL+mmproj.
- rustfmt + clippy -D warnings tr�n 2 file m?i: exit 0. B?ng ch?ng d?y d?: `docs/03-danh-gia/vision-prompt-budget-acceptance.md` m?c 9.
