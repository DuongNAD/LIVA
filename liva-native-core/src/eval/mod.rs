//! LIVA-Eval: Automated Evaluation & Benchmark Harness (Features F10, F11).
//!
//! Provides comprehensive benchmarking tools for:
//! - Raw TTFT vs Visible TTFT measurement
//! - TPS throughput computation with zero-duration protection
//! - Tool call precision, recall, and F1 scoring
//! - Argument schema conformance validation
//! - Chain-of-Thought (CoT) reasoning soundness verification
//! - Latency percentile aggregation (p50, p90, p95, p99)
//! - Multi-provider benchmark execution (Local GGUF, Cloud API, Mock)
//! - Multi-model comparative delta matrices and CI gating

pub mod dataset;
pub mod metrics;
pub mod runner;

pub use dataset::{BenchmarkSuite, TestCase, inject_anti_cache_nonce, inject_vietnamese_nonce};
pub use metrics::{
    CoTSoundnessResult, LatencyDistribution, TimingMetrics, ToolCallMetrics,
    calculate_latency_percentiles, calculate_percentile, calculate_tps, calculate_visible_tps,
    evaluate_cot_soundness, validate_arguments_schema,
};
pub use runner::{
    BenchmarkReport, BenchmarkRunner, CloudApiProvider, ComparativeMatrix, EvaluationConfig,
    EvaluationProvider, EvaluationRun, MockProvider, ProviderResponse, TestCaseSummary,
};

use serde::{Deserialize, Serialize};

/// Lightweight single-evaluation result struct for backward-compatible benchmark aggregations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvaluationResult {
    pub test_id: String,
    pub raw_ttft_ms: u64,
    pub visible_ttft_ms: u64,
    pub total_duration_ms: u64,
    pub generated_tokens: usize,
    pub selected_tool: Option<String>,
    pub passed_tool_selection: bool,
    pub passed_schema_validation: bool,
}

impl EvaluationResult {
    pub fn tps(&self) -> f64 {
        calculate_tps(self.generated_tokens, self.total_duration_ms)
    }
}

/// Helper aggregator for static benchmark computations.
pub struct BenchmarkAggregator;

impl BenchmarkAggregator {
    /// Calculate (p50, p95) from latencies.
    pub fn calculate_p50_p95(latencies: Vec<u64>) -> (u64, u64) {
        let dist = calculate_latency_percentiles(latencies);
        (dist.p50, dist.p95)
    }

    /// Calculate accuracy ratio across evaluation results.
    pub fn calculate_accuracy(results: &[EvaluationResult]) -> f64 {
        if results.is_empty() {
            return 0.0;
        }
        let passed = results
            .iter()
            .filter(|r| r.passed_tool_selection && r.passed_schema_validation)
            .count();
        passed as f64 / results.len() as f64
    }

    /// Inject anti-cache nonce into prompt.
    pub fn inject_anti_cache_nonce(prompt: &str, iteration: usize) -> String {
        inject_anti_cache_nonce(prompt, iteration)
    }

    /// Validate args schema against expected JSON schema.
    pub fn validate_args_schema(
        expected_schema: &serde_json::Value,
        actual_args: &serde_json::Value,
    ) -> bool {
        validate_arguments_schema(expected_schema, actual_args)
    }
}
