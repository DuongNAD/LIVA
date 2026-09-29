# TEST_INFRA — LIVA Native Core Reliability Upgrade
# (Vision Context Guard & Dynamic Prompt Budgeting)

## 1. Test Philosophy & Architecture
- **Requirement-Driven & Opaque-Box**: All test assertions are derived directly from `ORIGINAL_REQUEST.md` (R1: Vision Context Guard, R2: Dynamic Prompt Budgeting, R3: Quality Gates & Zero Regression) and `PROJECT.md`.
- **Progressive Testability & Isolation**: Tests run deterministically without requiring external cloud API credentials or live heavy weights during regression gates. Each test is isolated, sets up its own state, and cleans up after execution.
- **Multi-Tier Validation Matrix**:
  - **Tier 1 (Feature Coverage)**: Core happy paths, baseline limits, parameter ceilings, and format assertions.
  - **Tier 2 (Boundary & Corner Cases)**: Context limit edges (3583 vs 3584 tokens), slice ceilings (>7 slices), 4K downsampling, empty buffers, single-token overruns.
  - **Tier 3 (Combinations & Concurrency)**: Mixed text/image payload distributions, chronological sequence order invariants under eviction, concurrent lock contention, and stream cancellation.
  - **Tier 4 (Real-World Workloads & E2E Simulation)**: 100-turn continuous chat simulation, 4K screen capture downsampling, multi-tool chat dialogues exceeding 4096 tokens, and IPC authorization boundaries.

---

## 2. Feature Inventory & Coverage Mapping

| Subsystem | Feature Description | Tier 1 (Core) | Tier 2 (Boundary) | Tier 3 (Pairwise/Combo) | Tier 4 (E2E Workload) |
|---|---|:---:|:---:|:---:|:---:|
| **Subsystem A: Vision Context Guard (R1)** | Pre-tokenization screen resolution clamp (1920x1080) | ✓ | ✓ | ✓ | ✓ |
| | MtmdContextParams token ceiling (`image_max_tokens = 2048`, `image_min = 64`) | ✓ | ✓ | ✓ | ✓ |
| | Post-tokenization `check_prompt_fits` context gate | ✓ | ✓ | ✓ | ✓ |
| | Generation loop bounds (`n_past >= n_ctx`) & KV cache reset | ✓ | ✓ | ✓ | ✓ |
| | Graceful error propagation (no abort/GGML_ASSERT) | ✓ | ✓ | ✓ | ✓ |
| **Subsystem B: Dynamic Prompt Budgeting (R2)** | `PromptBudget::for_dialogue` context calculation | ✓ | ✓ | ✓ | ✓ |
| | `DynamicPromptAssembler::budget_messages` priority eviction (P4 -> P3 -> P2 -> P1) | ✓ | ✓ | ✓ | ✓ |
| | Mandatory P0 persona preservation (cannot be evicted) | ✓ | ✓ | ✓ | ✓ |
| | Strict chronological sequence order restoration post-eviction | ✓ | ✓ | ✓ | ✓ |
| | Multi-turn dialogue compaction under heavy tool schemas | ✓ | ✓ | ✓ | ✓ |

---

## 3. Enumerated Test Scenarios (40 Test Matrix from Spec Miner)

### Subsystem A: Vision Context Guard (20 Scenarios)

#### Tier 1: Feature Coverage (Core Functionality)
1. **`test_vision_guard_allows_safe_token_count`**: Prompts within safe token ceilings (`total_tokens + 512 < n_ctx`) evaluate cleanly without rejection.
2. **`test_vision_guard_rejects_context_overflow`**: Multimodal prompts where `prompt_tokens + 512 >= n_ctx` are intercepted and rejected with a descriptive error before prefill.
3. **`test_vision_slice_ceiling_enforcement`**: Vision inputs configured with slice counts exceeding the safety ceiling (>7 slices) are rejected prior to token explosion.
4. **`test_vision_image_max_tokens_configuration`**: Verifies `MAX_VISION_IMAGE_TOKENS` (2048) and `image_min_tokens` (64) are enforced in `MtmdContextParams`.
5. **`test_vision_error_propagation_not_abort`**: Verifies that guard failures return structured `Err(String)` instead of triggering `GGML_ASSERT` or `abort()`.

#### Tier 2: Boundary & Corner Cases
6. **`test_vision_exact_boundary_n_ctx_minus_reserve`**: Prompt token count at exact limit (`3583` tokens for `n_ctx = 4096`) succeeds (`3583 + 512 = 4095 < 4096`).
7. **`test_vision_one_token_over_boundary`**: Prompt token count at exact boundary (`3584` tokens for `n_ctx = 4096`) fails (`3584 + 512 = 4096 < 4096` is false).
8. **`test_vision_slice_boundary_exactly_7_slices`**: Boundary condition of exactly 7 slices is accepted.
9. **`test_vision_zero_dimension_image`**: Zero-width or zero-height image input yields a controlled validation error without division by zero.
10. **`test_vision_extreme_aspect_ratio`**: Extreme aspect ratios (e.g. 10000x20 or 20x10000) are clamped cleanly without integer overflow.
11. **`test_vision_corrupted_png_buffer`**: Corrupted, truncated, or random image byte buffers return a clean error without native runtime crashes.

#### Tier 3: Combinations & Concurrency
12. **`test_vision_huge_text_small_image_overflow`**: Tests long text prompt (3300 tokens) combined with small image (300 tokens) triggering total context overflow rejection.
13. **`test_vision_small_text_huge_image_overflow`**: Tests short text question (50 tokens) combined with maximum image tokens triggering overflow rejection.
14. **`test_vision_after_text_completion_kv_reset`**: Verifies that standard text generation followed by a vision request clears KV cache and does not leak prefix state.
15. **`test_vision_concurrent_lock_contention`**: Concurrent access to the LLM router mutex serializes requests safely without deadlocks or state corruption.
16. **`test_vision_client_cancellation_during_streaming`**: Streaming callback cancellation halts token generation promptly and releases the engine lock.

#### Tier 4: Real-World Workloads & End-to-End Stress
17. **`test_vision_real_screen_capture_1080p_crop`**: Simulates 1920x1080 desktop frame crop and verifies RGB extraction format and dimensions.
18. **`test_vision_websocket_remote_principal_rejection`**: Verifies that unauthorized principals (`WebSocketRemote`) are denied `vision:ask` execution.
19. **`test_vision_ipc_stdin_local_cli_execution`**: Verifies local CLI / IPC principal authorization and parameter validation.
20. **`test_vision_repeated_rapid_fire_calls`**: 10 consecutive vision queries in rapid succession verify clean state resets and zero memory leaks.

---

### Subsystem B: Dynamic Prompt Assembler (20 Scenarios)

#### Tier 1: Feature Coverage (Core Functionality)
21. **`test_assembler_budget_from_context_window`**: `PromptBudget::for_dialogue` computes proper system, tool, and response quotas (e.g. 40% system, 30% tools).
22. **`test_assembler_assemble_prompt_with_skills_and_tools`**: Prompt compilation includes base persona, active skills, and compact tool schemas.
23. **`test_assembler_assemble_messages_priority_selection`**: Evaluates prioritized slice selection when total tokens exceed budget quotas.
24. **`test_assembler_compact_tool_schema_formatting`**: Compact tool formatting emits single-line schema with `*` for required fields.
25. **`test_assembler_compile_budgeted_prompt_end_to_end`**: End-to-end prompt compilation produces syntactically valid model template outputs.

#### Tier 2: Boundary & Corner Cases
26. **`test_assembler_zero_system_budget`**: Zero system token budget yields `PromptAssemblyError::InvalidConfiguration`.
27. **`test_assembler_empty_slices_input`**: Empty messages input returns empty vector or `PromptAssemblyError::EmptyPrompt`.
28. **`test_assembler_mandatory_p0_exceeds_budget`**: P0 System Core slice exceeding total budget returns `BudgetExceeded` without evicting P0.
29. **`test_assembler_exact_budget_match`**: Total slice tokens matching budget exactly retains 100% of slices.
30. **`test_assembler_single_token_overflow_eviction`**: Single token over budget evicts the lowest priority slice.
31. **`test_assembler_unicode_vietnamese_token_estimation`**: Vietnamese text with combining diacritics and emojis is estimated without panic or slicing error.

#### Tier 3: Combinations & Invariants
32. **`test_assembler_strict_priority_eviction_hierarchy`**: Strict hierarchy enforcement: P4 evicted before P3, P3 before P2, P2 before P1; P0 never evicted.
33. **`test_assembler_chronological_sequence_preservation_under_eviction`**: Evicting older turns strictly preserves chronological order of remaining messages.
34. **`test_assembler_skill_priority_sorting`**: Higher priority skills are retained over lower priority skills under constrained budget.
35. **`test_assembler_tool_quota_truncation_without_partial_syntax`**: Tool catalog truncation omits excess tools without corrupting schema lines.
36. **`test_assembler_syntax_preservation_chatml_vs_gemma`**: ChatML (`<|im_start|>`) and Gemma (`<start_of_turn>`) syntax preserved without delimiter bleed.

#### Tier 4: Real-World Workloads & Integration
37. **`test_assembler_integration_chat_completion_path`**: Simulated `chat:completion` path with 30-turn history and 7 tools stays under `n_ctx - 512`.
38. **`test_assembler_integration_dialogue_handler_path`**: Voice dialogue simulation with recalled memory facts packs into P3 and stays within budget.
39. **`test_assembler_integration_agent_graph_pipeline`**: Agent graph multi-step execution maintains bounded context without naive history trimming.
40. **`test_assembler_100_turn_continuous_chat_stress`**: 100 consecutive turns maintain flat steady-state prompt length within budget.

---

## 4. Test Runners & Verification Commands

```powershell
# Run Vision Context Guard test suite
cargo test --test vision_context_guard_tests

# Run Dynamic Prompt Budgeting integration test suite
cargo test --test dynamic_prompt_integration_tests

# Run existing assembly unit test suite
cargo test --test dynamic_prompt_assembly_tests

# Run full native core test suite
cargo test --no-fail-fast -p liva-native-core

# Run Gateway E2E CI verification
node scripts/e2e-gateway-ci.mjs
```

---

## 5. Addendum 17/09/2026 � exact-budget runtime path and opt-in suites

- The estimate-based rows above (`PromptBudget::for_dialogue`, `budget_messages`) remain for
  compatibility/unit tests only. The runtime guard is the exact path: `ContextTokenBudget` +
  `DynamicPromptAssembler::compile_exact_budgeted_prompt` (template-compiled, tokenizer-measured,
  atomic-group eviction) behind `LlamaRouterManager::generate_budgeted_completion`; the final
  `check_prompt_fits` guard is unchanged. Vision measures the real `chunks.total_tokens()` after
  compilation; persona, image and the current question are mandatory.
- New model-free suite: `tests/completion_stream_tests.rs` (4 tests � heartbeat cancellation,
  wire-shape preservation, error envelope; the closed-channel heartbeat regression was verified
  RED before the fix).
- Opt-in runtime suites: `tests/prompt_budget_runtime.rs` (text path � ran GREEN twice in release
  against gemma-4-E2B on CPU) and `tests/vision_budget_runtime.rs` (release-only; BLOCKED until a
  matching VL GGUF + mmproj fixture exists on disk). Both fail loudly (exit 101 with a
  configuration message) when run with `--ignored` and missing fixtures � per spec, never a
  silent pass.
- Evidence and commands: `docs/03-danh-gia/vision-prompt-budget-acceptance.md` sections 8-9.
