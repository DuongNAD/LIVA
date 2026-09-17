---
title: "Refactor - Context Token Budget Steps 3-4"
tags: [liva/knowledge, liva/refactor, code/graph]
author: "codex"
last_update: "2026-09-17"
---

# Context token budget — Bước 3–4

Người dùng phê duyệt Bước 3–4 của `E:\Project\01_AI_Agents\LIVA\implementation_plan.md` và miễn trừ search_vault/GitNexus/PDG cho phiên. Đọc Vault trực tiếp và dùng Select-String khoanh vùng ảnh hưởng; không chạy indexer.

- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\prompt\dynamic_prompt.rs`: thêm ContextTokenBudget, PromptSelection, BudgetedPrompt, selector/atomic groups và exact compile–measure–evict. Giữ P0/system/current user + suffix; validation IDs/groups; lỗi callback được làm sạch. Constructor PromptBudget không còn sàn vượt context nhỏ hoặc phép nhân overflow.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\src\llm\engine.rs`: check_prompt_fits giữ chữ ký, biên strict và compatibility prompt rỗng; prompt không rỗng đi qua ContextTokenBudget.
- `E:\Project\01_AI_Agents\LIVA\liva-native-core\tests\dynamic_prompt_assembly_tests.rs`: 15 test mới, tổng 26 pass. Test exact-budget dùng callback xác định, không phải tokenizer/model thật.

Kiểm chứng tuần tự offline/locked -j 2, test-threads 2: prompt 26, vision guard 21, runtime stability 3, unit engine guard 5 đều pass; cargo check core all-targets và Clippy core lib + prompt suite với -D warnings đều exit 0. Chưa chạy full workspace/release/CUDA/inference. Future-incompatibility proc-macro-error2 v2.0.1 vẫn còn.

Chưa sửa bốn caller fallback tại lib.rs, agent/graph/pipeline.rs, commands/llm.rs, websocket/dialogue.rs; API exact chưa nối runtime (Bước 5). ChatMessage không có call IDs, grouping chỉ bảo toàn turn chứ không xác thực mọi liên kết tool call.

Báo cáo đầy đủ: `E:\Project\01_AI_Agents\LIVA\docs\03-danh-gia\vision-prompt-budget-acceptance.md`, mục 7. Snapshot/log: `C:\Users\Admin\AppData\Local\Temp\liva-budget-step34-20260917-104954`. Staged diff được giữ nguyên; không commit/remote, không sửa DB/quyền IPC/dependency.
