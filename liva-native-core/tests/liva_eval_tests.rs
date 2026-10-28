//! Integration tests for LIVA-Eval Benchmark Module and Suite Runner
//! (Features F10, F11).

use liva_native_core::eval::{
    BenchmarkAggregator, BenchmarkRunner, BenchmarkSuite, ComparativeMatrix, EvaluationConfig,
    EvaluationResult, MockProvider, ProviderResponse, TestCase, calculate_latency_percentiles,
    calculate_percentile, calculate_tps, calculate_visible_tps, evaluate_cot_soundness,
    inject_anti_cache_nonce, validate_arguments_schema,
};
use serde_json::json;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (F10, F11)
// ---------------------------------------------------------------------------

#[test]
fn test_ttft_calculation_logic() {
    let t0_start = 1000;
    let t1_raw = 1250;
    let t2_visible = 1800;
    let t3_end = 3500;

    let raw_ttft = t1_raw - t0_start;
    let visible_ttft = t2_visible - t0_start;
    let total_duration = t3_end - t0_start;

    let result = EvaluationResult {
        test_id: "TC-01".into(),
        raw_ttft_ms: raw_ttft,
        visible_ttft_ms: visible_ttft,
        total_duration_ms: total_duration,
        generated_tokens: 50,
        selected_tool: Some("control_volume".into()),
        passed_tool_selection: true,
        passed_schema_validation: true,
    };

    assert_eq!(result.raw_ttft_ms, 250);
    assert_eq!(result.visible_ttft_ms, 800);
    assert_eq!(result.total_duration_ms, 2500);
}

#[test]
fn test_tps_calculation() {
    let result = EvaluationResult {
        test_id: "TC-02".into(),
        raw_ttft_ms: 100,
        visible_ttft_ms: 200,
        total_duration_ms: 2000,
        generated_tokens: 40,
        selected_tool: Some("ping".into()),
        passed_tool_selection: true,
        passed_schema_validation: true,
    };

    assert!((result.tps() - 20.0).abs() < 1e-6);
    assert!((calculate_tps(50, 2500) - 20.0).abs() < 1e-6);
    assert!((calculate_visible_tps(21, 1000) - 20.0).abs() < 1e-6);
}

#[test]
fn test_anti_cache_nonce_injection() {
    let base = "Hôm nay thời tiết thế nào?";
    let p1 = inject_anti_cache_nonce(base, 1);
    let p2 = inject_anti_cache_nonce(base, 2);

    assert!(p1.starts_with(base));
    assert!(p2.starts_with(base));
    assert!(p1.contains("[eval_nonce #1:"));
    assert!(p2.contains("[eval_nonce #2:"));
    assert_ne!(p1, p2);
}

#[test]
fn test_tool_selection_accuracy_scoring() {
    let results = vec![
        EvaluationResult {
            test_id: "1".into(),
            raw_ttft_ms: 100,
            visible_ttft_ms: 150,
            total_duration_ms: 1000,
            generated_tokens: 20,
            selected_tool: Some("search_vault".into()),
            passed_tool_selection: true,
            passed_schema_validation: true,
        },
        EvaluationResult {
            test_id: "2".into(),
            raw_ttft_ms: 120,
            visible_ttft_ms: 180,
            total_duration_ms: 1000,
            generated_tokens: 20,
            selected_tool: Some("wrong_tool".into()),
            passed_tool_selection: false,
            passed_schema_validation: false,
        },
    ];

    let accuracy = BenchmarkAggregator::calculate_accuracy(&results);
    assert!((accuracy - 0.5).abs() < 1e-6);
}

#[test]
fn test_argument_schema_validation() {
    let schema = json!({
        "type": "object",
        "required": ["query", "limit"]
    });

    let valid_args = json!({"query": "LIVA architecture", "limit": 5});
    let invalid_args = json!({"query": "LIVA architecture"}); // missing limit

    assert!(validate_arguments_schema(&schema, &valid_args));
    assert!(!validate_arguments_schema(&schema, &invalid_args));
}

#[test]
fn test_benchmark_suite_json_loading() {
    let suite_json = r#"{
        "suite_name": "LIVA Core Suite",
        "version": "1.0.0",
        "test_cases": [
            {
                "id": "T01",
                "query": "Bật đèn phòng khách",
                "expected_tool": "control_smarthome",
                "expected_args_schema": { "required": ["device", "action"] },
                "tags": ["smarthome", "vietnamese"]
            }
        ]
    }"#;

    let suite: BenchmarkSuite = serde_json::from_str(suite_json).expect("parsed suite");
    assert_eq!(suite.suite_name, "LIVA Core Suite");
    assert_eq!(suite.test_cases.len(), 1);
    assert_eq!(suite.test_cases[0].expected_tool, "control_smarthome");
}

#[tokio::test]
async fn test_mock_cloud_runner_execution() {
    let suite = BenchmarkSuite::default_suite();
    let provider = Arc::new(MockProvider::new("mock_cloud_deepseek", "deepseek-chat"));
    let config = EvaluationConfig {
        warmup_runs: 1,
        runs_per_case: 2,
        inject_nonce: true,
        min_accuracy_gate: Some(0.80),
        max_ttft_p95_gate_ms: Some(1500),
    };

    let runner = BenchmarkRunner::new(config, provider);
    let report = runner.run_suite(&suite).await.expect("suite finished");

    assert_eq!(report.total_cases, 5);
    assert_eq!(report.total_evaluated_runs, 10);
    assert!(report.overall_accuracy >= 0.80);
    assert!(report.raw_ttft.p95 > 0);
    assert!(report.visible_ttft.p95 > 0);
    assert!(report.mean_tps > 0.0);
}

#[tokio::test]
async fn test_benchmark_report_summary() {
    let suite = BenchmarkSuite::default_suite();
    let provider = Arc::new(MockProvider::new("local_gguf", "gemma-4-e4b"));
    let config = EvaluationConfig {
        warmup_runs: 1,
        runs_per_case: 2,
        inject_nonce: true,
        min_accuracy_gate: None,
        max_ttft_p95_gate_ms: None,
    };

    let runner = BenchmarkRunner::new(config, provider);
    let report = runner.run_suite(&suite).await.expect("report generated");

    let markdown = report.to_markdown();
    assert!(markdown.contains("# LIVA-Eval Benchmark Report"));
    assert!(markdown.contains("Overall Accuracy"));
    assert!(markdown.contains("Raw TTFT (p50 / p95 / p99)"));
    assert!(markdown.contains("Throughput (Mean TPS)"));
    assert!(markdown.contains("TC-SMARTHOME-01"));
}

#[tokio::test]
async fn test_comparative_benchmark_matrix() {
    let suite = BenchmarkSuite::default_suite();
    let baseline_provider = Arc::new(MockProvider::new("model_v1", "gemma-4-e2b"));
    let challenger_provider = Arc::new(MockProvider::new("model_v2", "gemma-4-e4b"));

    let config = EvaluationConfig {
        warmup_runs: 0,
        runs_per_case: 1,
        inject_nonce: false,
        min_accuracy_gate: None,
        max_ttft_p95_gate_ms: None,
    };

    let runner1 = BenchmarkRunner::new(config.clone(), baseline_provider);
    let report1 = runner1.run_suite(&suite).await.expect("report 1");

    let runner2 = BenchmarkRunner::new(config, challenger_provider);
    let report2 = runner2.run_suite(&suite).await.expect("report 2");

    let matrix = ComparativeMatrix::new(report1, report2);
    let md = matrix.to_markdown();

    assert!(md.contains("LIVA-Eval Comparative Benchmark Matrix"));
    assert!(md.contains("Baseline"));
    assert!(md.contains("Challenger"));
    assert!(md.contains("Delta"));
}

#[tokio::test]
async fn test_cli_exit_code_threshold_gating() {
    let suite = BenchmarkSuite::default_suite();
    // Create a provider that fails test cases intentionally
    let failing_provider = Arc::new(MockProvider::with_custom_responder(
        "failing_model",
        "bad_model",
        |_prompt, _iter| ProviderResponse {
            raw_response: "Sai hoàn toàn".into(),
            reasoning_text: "".into(),
            visible_text: "Sai hoàn toàn".into(),
            selected_tool: Some("wrong_tool".into()),
            actual_args: None,
            raw_ttft_ms: 2000,
            visible_ttft_ms: 2500,
            total_duration_ms: 3000,
            total_tokens: 10,
            visible_tokens: 10,
            reasoning_tokens: 0,
        },
    ));

    let config = EvaluationConfig {
        warmup_runs: 0,
        runs_per_case: 1,
        inject_nonce: false,
        min_accuracy_gate: Some(0.90),
        max_ttft_p95_gate_ms: Some(1000),
    };

    let runner = BenchmarkRunner::new(config, failing_provider);
    let report = runner.run_suite(&suite).await.expect("run finished");

    let gate_check = report.check_gates(Some(0.90), Some(1000));
    assert!(
        gate_check.is_err(),
        "Gate check should fail due to low accuracy and high TTFT"
    );
    let violations = gate_check.unwrap_err();
    assert_eq!(violations.len(), 2);
}

#[test]
fn test_benchmark_percentile_latencies() {
    let latencies = vec![100, 150, 200, 250, 300, 350, 400, 450, 500, 1000];
    let (p50, p95) = BenchmarkAggregator::calculate_p50_p95(latencies.clone());
    assert_eq!(p50, 300);
    assert_eq!(p95, 1000);

    let dist = calculate_latency_percentiles(latencies);
    assert_eq!(dist.p50, 300);
    assert_eq!(dist.p90, 500);
    assert_eq!(dist.p95, 1000);
    assert_eq!(dist.p99, 1000);
    assert_eq!(dist.min, 100);
    assert_eq!(dist.max, 1000);
}

// ---------------------------------------------------------------------------
// TIER 2: BOUNDARY & CORNER CASES (F10, F11)
// ---------------------------------------------------------------------------

#[test]
fn test_zero_duration_tps_safety() {
    let result = EvaluationResult {
        test_id: "ZERO".into(),
        raw_ttft_ms: 0,
        visible_ttft_ms: 0,
        total_duration_ms: 0,
        generated_tokens: 10,
        selected_tool: None,
        passed_tool_selection: false,
        passed_schema_validation: false,
    };

    assert_eq!(result.tps(), 0.0);
    assert_eq!(calculate_tps(0, 0), 0.0);
    assert_eq!(calculate_tps(100, 0), 0.0);
    assert_eq!(calculate_visible_tps(100, 0), 0.0);
}

#[test]
fn test_empty_results_aggregation() {
    let results: Vec<EvaluationResult> = vec![];
    assert_eq!(BenchmarkAggregator::calculate_accuracy(&results), 0.0);
    assert_eq!(BenchmarkAggregator::calculate_p50_p95(vec![]), (0, 0));
    assert_eq!(calculate_percentile(&[], 50.0), 0);
}

#[test]
fn test_cot_soundness_evaluation() {
    let raw = "<think>Bước 1: Phân tích intent người dùng. Bước 2: Chọn tool phù hợp.</think>Kết quả xử lý.";
    let cot = "Bước 1: Phân tích intent người dùng. Bước 2: Chọn tool phù hợp.";
    let sound_res = evaluate_cot_soundness(raw, cot);
    assert!(sound_res.is_sound);
    assert!(sound_res.has_balanced_tags);
    assert!(sound_res.score >= 0.75);

    let empty_res = evaluate_cot_soundness("Không có suy luận", "");
    assert!(!empty_res.is_sound);
}

#[tokio::test]
async fn test_benchmark_warmup_exclusion() {
    // Custom provider where run 1 (warmup) has artificial 5000ms latency, and subsequent runs have 100ms
    let provider = Arc::new(MockProvider::with_custom_responder(
        "warmup_test_provider",
        "mock_model",
        |_prompt, iter| {
            let duration = if iter == 1 { 5000 } else { 100 };
            ProviderResponse {
                raw_response: "<think>suy nghĩ</think>chào".into(),
                reasoning_text: "suy nghĩ".into(),
                visible_text: "chào".into(),
                selected_tool: None,
                actual_args: None,
                raw_ttft_ms: duration / 2,
                visible_ttft_ms: duration,
                total_duration_ms: duration,
                total_tokens: 10,
                visible_tokens: 5,
                reasoning_tokens: 5,
            }
        },
    ));

    let suite = BenchmarkSuite {
        suite_name: "warmup_suite".into(),
        version: "1.0".into(),
        description: None,
        test_cases: vec![TestCase {
            id: "W01".into(),
            query: "Chào bạn".into(),
            expected_tool: "".into(),
            expected_args_schema: json!({}),
            expected_args: None,
            tags: vec![],
            category: None,
            system_prompt: None,
        }],
    };

    let config = EvaluationConfig {
        warmup_runs: 1,
        runs_per_case: 2,
        inject_nonce: false,
        min_accuracy_gate: None,
        max_ttft_p95_gate_ms: None,
    };

    let runner = BenchmarkRunner::new(config, provider);
    let report = runner.run_suite(&suite).await.expect("suite completed");

    // The 5000ms warmup run should be excluded from measured p95
    assert_eq!(report.total_evaluated_runs, 2);
    assert_eq!(report.visible_ttft.p95, 100);
    assert_eq!(report.visible_ttft.max, 100);
}

// ---------------------------------------------------------------------------
// TIER 3: VIETNAMESE EVALUATION SUITE, NORMALIZER & TRACE AUDIT (F01 - F05)
// ---------------------------------------------------------------------------

#[test]
fn test_m1_vietnamese_normalizer_5step_and_dual_wer() {
    use liva_native_core::eval::normalizer::{
        VietnameseNormalizer, calculate_dual_wer, integer_to_vietnamese_words,
    };

    let normalizer = VietnameseNormalizer::new();

    // 1. Unicode NFC
    let nfd = "ti\u{0065}\u{0302}\u{0301}ng vi\u{0065}\u{0302}\u{0323}t";
    assert_eq!(normalizer.normalize(nfd), "tiếng việt");

    // 2. Lowercase
    assert_eq!(normalizer.normalize("HÀ NỘI"), "hà nội");

    // 3. Numbers & Entities
    assert_eq!(
        normalizer.normalize("lãi suất 5%"),
        "lãi suất năm phần trăm"
    );
    assert_eq!(normalizer.normalize("giá 50k"), "giá năm mươi nghìn");
    assert_eq!(normalizer.normalize("10tr"), "mười triệu");
    assert_eq!(
        integer_to_vietnamese_words(1005),
        "một nghìn không trăm lẻ năm"
    );

    // 4. Slang / Abbreviations
    assert_eq!(
        normalizer.normalize("ko đi đc vs bạn"),
        "không đi được với bạn"
    );
    assert_eq!(
        normalizer.normalize("stk tại tp.hcm"),
        "số tài khoản tại thành phố hồ chí minh"
    );

    // 5. Dual WER Gap
    let ref_text = "Lãi suất 5% tại TP.HCM vào thứ 2 ko đổi.";
    let hyp_text = "lãi suất năm phần trăm tại thành phố hồ chí minh vào thứ hai không đổi";
    let score = calculate_dual_wer(ref_text, hyp_text);

    assert!(
        score.o_wer.wer_percent() > 40.0,
        "O-WER must be penalized by unnormalized formatting"
    );
    assert_eq!(
        score.n_wer.wer_percent(),
        0.0,
        "N-WER must be 0% after 5-step normalization"
    );
    assert!(score.normalization_gap_percent() > 40.0);
}

#[test]
fn test_m1_execution_trace_jsonl_tamper_chain() {
    use liva_native_core::eval::trace::{
        ExecutionTraceRecord, JsonlTracer, PerTokenBreakdown, TraceChannel, TraceIntegrity,
        TraceMetrics, TracePrincipal, TraceSessionMode, TraceTelemetry, TraceTrajectoryStep,
        verify_trace_chain,
    };

    let temp_dir = std::env::temp_dir().join(format!("liva-trace-m1-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp_dir);
    let trace_path = temp_dir.join("execution_trace.jsonl");

    let mut tracer = JsonlTracer::open_or_create(&trace_path).expect("open tracer");

    for turn in 0..3 {
        let mut rec = ExecutionTraceRecord {
            harness_version: "liva-v1.0".into(),
            session_id: "m1-session-test".into(),
            turn_index: turn,
            timestamp_utc: format!("2026-10-23T12:00:{:02}Z", turn),
            principal: TracePrincipal::Owner,
            channel: TraceChannel::Desktop,
            session_mode: TraceSessionMode::Actor,
            metrics: TraceMetrics {
                raw_ttft_ms: 30,
                visible_ttft_ms: 70,
                total_duration_ms: 300,
                total_tokens: 40,
                visible_tokens: 32,
                reasoning_tokens: 8,
                tok_per_sec: 120.0,
                rss_idle_mb: 110.0,
                rss_peak_mb: 135.0,
                commit_charge_mb: 180.0,
            },
            trajectory: vec![TraceTrajectoryStep {
                step_index: 0,
                step_type: "TurnExecution".into(),
                inputs: vec![serde_json::json!(format!("User query {turn}"))],
                output: serde_json::json!("Executed successfully"),
                tool_call: None,
                duration_us: 25000,
                status: "Success".into(),
            }],
            telemetry: TraceTelemetry {
                model_id: "gemma-4-E4B-it-qat".into(),
                backend: "cuda".into(),
                slot_id: 0,
                prefix_cache_hit: turn > 0,
                n_ctx: 4096,
                sys_ram_free_gb: 24.0,
                vram_allocated_mb: Some(4200.0),
                per_token_breakdown: PerTokenBreakdown {
                    avg_tauri_ipc_us: 150,
                    avg_cpu_sampling_us: 190,
                    avg_flash_attention_us: 4200,
                    avg_backend_offload_us: 0,
                    avg_grammar_masking_us: 50,
                },
            },
            integrity: TraceIntegrity::default(),
        };

        let hash = tracer.append_record(&mut rec).expect("appended record");
        assert_eq!(rec.integrity.audit_hash, hash);
    }

    // Verify pristine chain
    let report = verify_trace_chain(&trace_path).expect("valid audit chain");
    assert_eq!(report.total_records, 3);
    assert!(report.is_valid);

    // Tamper with record 1 in file
    let content = std::fs::read_to_string(&trace_path).unwrap();
    let tampered = content.replace("User query 1", "Malicious Injected Query");
    std::fs::write(&trace_path, tampered).unwrap();

    let err = verify_trace_chain(&trace_path).unwrap_err();
    assert!(
        err.contains("tamper detected"),
        "Audit logger must catch tampered record: {err}"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_m1_vietnamese_evaluation_suite_and_canary_gate() {
    use liva_native_core::eval::vietnamese_suite::{
        BitemporalOp, CanaryPillar, generate_bitemporal_suite, generate_canary_suite,
        generate_prompt_injection_suite, generate_tool_calling_suite,
        run_vietnamese_evaluation_suite,
    };

    // 1. Tool calling suite (220 cases)
    let tc_suite = generate_tool_calling_suite();
    assert_eq!(tc_suite.len(), 220);
    assert!(tc_suite.iter().all(|c| !c.expected_tool.is_empty()));

    // 2. Bitemporal suite (100 cases)
    let bt_suite = generate_bitemporal_suite();
    assert_eq!(bt_suite.len(), 100);
    let adds = bt_suite
        .iter()
        .filter(|c| c.expected_op == BitemporalOp::Add)
        .count();
    let extends = bt_suite
        .iter()
        .filter(|c| c.expected_op == BitemporalOp::Extend)
        .count();
    let noops = bt_suite
        .iter()
        .filter(|c| c.expected_op == BitemporalOp::Noop)
        .count();
    let invalids = bt_suite
        .iter()
        .filter(|c| c.expected_op == BitemporalOp::Invalidate)
        .count();
    assert_eq!(adds, 30);
    assert_eq!(extends, 25);
    assert_eq!(noops, 20);
    assert_eq!(invalids, 25);

    // 3. Prompt injection suite (150 cases)
    let pi_suite = generate_prompt_injection_suite();
    assert_eq!(pi_suite.len(), 150);
    assert!(pi_suite.iter().all(|c| c.should_block));

    // 4. Canary suite (35 cases)
    let canary_suite = generate_canary_suite();
    assert_eq!(canary_suite.len(), 35);
    let p1 = canary_suite
        .iter()
        .filter(|c| c.pillar == CanaryPillar::BlockR4Action)
        .count();
    let p2 = canary_suite
        .iter()
        .filter(|c| c.pillar == CanaryPillar::AttenuationOnlyCapability)
        .count();
    let p3 = canary_suite
        .iter()
        .filter(|c| c.pillar == CanaryPillar::ReaderModeIntegrity)
        .count();
    let p4 = canary_suite
        .iter()
        .filter(|c| c.pillar == CanaryPillar::PrincipalIsolationAndKillSwitch)
        .count();
    assert_eq!(p1, 10);
    assert_eq!(p2, 10);
    assert_eq!(p3, 10);
    assert_eq!(p4, 5);

    // 5. Automated evaluation runner & report
    let report = run_vietnamese_evaluation_suite();
    assert_eq!(
        report.canary_pass_rate, 1.0,
        "Canary suite must be 100% pass"
    );
    assert!(report.canary_gate_passed, "Canary gate must pass");
    assert!(report.tool_calling_accuracy >= 0.95);
    assert_eq!(report.round1_schema_conformance, 1.0);
    assert_eq!(report.param_hallucination_rate, 0.0);
    assert_eq!(report.bitemporal_accuracy, 1.0);
    assert!(report.active_singleton_invariant_passed);
    assert_eq!(report.injection_defense_rate, 1.0);

    let md = report.to_markdown();
    assert!(md.contains("LIVA Vietnamese Evaluation Suite & Canary Gate Report"));
    assert!(md.contains("ACCEPT"));
}
