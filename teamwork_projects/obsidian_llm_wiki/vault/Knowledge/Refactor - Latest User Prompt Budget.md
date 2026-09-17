---
title: "Refactor - Latest User Prompt Budget"
tags: [liva/knowledge, liva/refactor, code/graph]
author: "codex"
last_update: "2026-09-17"
---

# fix(prompt): preserve latest user and bound estimated assembly

- User approved bounded Vault text search in place of unavailable search_vault. No indexer or concurrent compiler used.
- Updated `E:/Project/01_AI_Agents/LIVA/liva-native-core/src/llm/prompt/dynamic_prompt.rs`: reverse user lookup, first-system lookup, latest user P0 in both selectors, checked mandatory-token sum, overflow-safe optional admission, context-aware estimate APIs including reserve, adaptive estimation cushion min(1% n_ctx, 31).
- Existing percentage quotas (40/30 and 55/remainder) retained. Runtime reserve remains 512; context 512 with reserve 512 is invalid. API compatibility wrappers infer a ceiling from quotas; callers requiring an actual n_ctx must use explicit ContextTokenBudget APIs or exact runtime assembly.
- Added `E:/Project/01_AI_Agents/LIVA/liva-native-core/tests/dynamic_prompt_regression_tests.rs`: two latest-user regressions observed failing before repair, passing after. Five final tests cover suffix roles, oversized user errors, 512/1024 boundaries, optional usize::MAX, and context range 0..4096.
- Scoped verification: 37 passed (26 existing prompt + 5 regression + 2 manager entry + 4 stream). rustfmt check and scoped git diff --check passed.
- Existing `E:/Project/01_AI_Agents/LIVA/liva-native-core/src/lib.rs` and `E:/Project/01_AI_Agents/LIVA/liva-native-core/src/agent/graph/pipeline.rs` already call generate_budgeted_completion and propagate errors; no fallback edits needed. This is source inspection, not caller E2E validation. Model fixture LIVA_TEST_MODEL_PATH is not configured.
- Full core test run started separately; result pending. No full-core success claim yet.
- Known remaining limitation: count-based trimming before exact selection in callers may cut across a user turn. This change does not certify end-to-end preservation across that earlier stage.
- Baseline snapshot: `C:/Users/Admin/AppData/Local/Temp/liva-prompt-20260917-185514/dynamic_prompt.rs`. Existing staged changes untouched; no commit/remote operations.
