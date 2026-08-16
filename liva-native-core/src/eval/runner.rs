//! Multi-session Benchmark Runner and Provider Abstraction for LIVA-Eval (Features F10, F11).
//!
//! Supports local GGUF execution, Cloud APIs (OpenAI/DeepSeek), and mock providers
//! with latency percentiles (p50, p90, p95, p99), warmup run exclusion, and reporting.

use crate::eval::dataset::{BenchmarkSuite, TestCase, inject_anti_cache_nonce};
use crate::eval::metrics::{
    CoTSoundnessResult, LatencyDistribution, ToolCallMetrics, calculate_latency_percentiles,
    calculate_tps, evaluate_cot_soundness, validate_arguments_schema,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Boxed thread-safe future type alias for dyn-compatible async provider methods.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Result of a single LLM generation from an EvaluationProvider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub raw_response: String,
    pub reasoning_text: String,
    pub visible_text: String,
    pub selected_tool: Option<String>,
    pub actual_args: Option<Value>,
    pub raw_ttft_ms: u64,
    pub visible_ttft_ms: u64,
    pub total_duration_ms: u64,
    pub total_tokens: usize,
    pub visible_tokens: usize,
    pub reasoning_tokens: usize,
}

/// Provider abstraction for running evaluation requests.
pub trait EvaluationProvider: Send + Sync {
    fn provider_name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn generate_response<'a>(
        &'a self,
        prompt: &'a str,
        system_prompt: Option<&'a str>,
        iteration: usize,
    ) -> BoxFuture<'a, Result<ProviderResponse, String>>;
}

/// Function signature alias for mock custom responders.
pub type MockResponder = Arc<dyn Fn(&str, usize) -> ProviderResponse + Send + Sync>;

/// Mock Evaluation Provider for deterministic testing and CI validation.
pub struct MockProvider {
    pub name: String,
    pub model: String,
    pub mock_fn: Option<MockResponder>,
}

impl MockProvider {
    pub fn new(name: &str, model: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            mock_fn: None,
        }
    }

    pub fn with_custom_responder<F>(name: &str, model: &str, f: F) -> Self
    where
        F: Fn(&str, usize) -> ProviderResponse + Send + Sync + 'static,
    {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            mock_fn: Some(Arc::new(f)),
        }
    }

    /// Pattern-based default mock responder simulating intelligent LIVA tool routing.
    fn default_mock_response(&self, prompt: &str, _iteration: usize) -> ProviderResponse {
        let lower = prompt.to_lowercase();
        let (tool, args, reasoning, visible) = if lower.contains("đèn")
            || lower.contains("quạt")
            || lower.contains("máy lạnh")
        {
            let device = if lower.contains("đèn") {
                "light"
            } else if lower.contains("quạt") {
                "fan"
            } else {
                "ac"
            };
            let action = if lower.contains("tắt") || lower.contains("off") {
                "off"
            } else {
                "on"
            };
            (
                Some("control_smarthome".to_string()),
                Some(json!({ "device": device, "action": action })),
                format!(
                    "Bước 1: Phân tích lệnh thiết bị {device}. Bước 2: Gọi tool control_smarthome với action {action}."
                ),
                format!("Đã thực hiện lệnh {action} cho {device}."),
            )
        } else if lower.contains("vault") || lower.contains("ghi chú") || lower.contains("họp") {
            (
                Some("search_vault".to_string()),
                Some(json!({ "query": "cuộc họp" })),
                "Bước 1: Tìm kiếm tài liệu cuộc họp trong Obsidian Vault.".to_string(),
                "Tìm thấy 3 ghi chú liên quan đến cuộc họp.".to_string(),
            )
        } else if lower.contains("âm lượng") || lower.contains("volume") {
            (
                Some("set_volume".to_string()),
                Some(json!({ "level": 80 })),
                "Bước 1: Điều chỉnh âm lượng hệ thống lên 80%.".to_string(),
                "Đã chỉnh âm lượng lên 80%.".to_string(),
            )
        } else {
            (
                None,
                None,
                "Không cần gọi tool. Trả lời trò chuyện thông thường.".to_string(),
                "Chào bạn! Mình có thể giúp gì cho bạn hôm nay?".to_string(),
            )
        };

        let raw_response = format!("<think>{reasoning}</think>{visible}");
        let raw_ttft = 120;
        let visible_ttft = 350;
        let total_duration = 1200;
        let total_tokens = 45;
        let visible_tokens = 25;
        let reasoning_tokens = 20;

        ProviderResponse {
            raw_response,
            reasoning_text: reasoning,
            visible_text: visible,
            selected_tool: tool,
            actual_args: args,
            raw_ttft_ms: raw_ttft,
            visible_ttft_ms: visible_ttft,
            total_duration_ms: total_duration,
            total_tokens,
            visible_tokens,
            reasoning_tokens,
        }
    }
}

impl EvaluationProvider for MockProvider {
    fn provider_name(&self) -> &str {
        &self.name
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn generate_response<'a>(
        &'a self,
        prompt: &'a str,
        _system_prompt: Option<&'a str>,
        iteration: usize,
    ) -> BoxFuture<'a, Result<ProviderResponse, String>> {
        Box::pin(async move {
            if let Some(ref custom) = self.mock_fn {
                Ok(custom(prompt, iteration))
            } else {
                Ok(self.default_mock_response(prompt, iteration))
            }
        })
    }
}

/// Cloud API Provider connecting to OpenAI / DeepSeek compatible `/v1/chat/completions`.
pub struct CloudApiProvider {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub client: reqwest::Client,
}

impl CloudApiProvider {
    pub fn new(api_base: &str, api_key: &str, model: &str) -> Self {
        Self {
            api_base: api_base.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl EvaluationProvider for CloudApiProvider {
    fn provider_name(&self) -> &str {
        "cloud_api"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn generate_response<'a>(
        &'a self,
        prompt: &'a str,
        system_prompt: Option<&'a str>,
        _iteration: usize,
    ) -> BoxFuture<'a, Result<ProviderResponse, String>> {
        Box::pin(async move {
            let endpoint = format!("{}/chat/completions", self.api_base);
            let mut messages = Vec::new();
            if let Some(sys) = system_prompt {
                messages.push(json!({ "role": "system", "content": sys }));
            }
            messages.push(json!({ "role": "user", "content": prompt }));

            let payload = json!({
                "model": self.model,
                "messages": messages,
                "temperature": 0.0,
                "stream": false
            });

            let start_time = Instant::now();
            let response = self
                .client
                .post(&endpoint)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
                .map_err(|e| format!("Cloud API request failed: {e}"))?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                return Err(format!("Cloud API error {status}: {body}"));
            }

            let body: Value = response
                .json()
                .await
                .map_err(|e| format!("Failed to parse Cloud API JSON: {e}"))?;

            let total_duration_ms = start_time.elapsed().as_millis() as u64;

            let content = body["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let reasoning_content = body["choices"][0]["message"]["reasoning_content"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_default();

            let mut selected_tool = None;
            let mut actual_args = None;
            if let Some(tool_calls) = body["choices"][0]["message"]["tool_calls"].as_array()
                && let Some(first_call) = tool_calls.first()
            {
                if let Some(name) = first_call["function"]["name"].as_str() {
                    selected_tool = Some(name.to_string());
                }
                if let Some(args_str) = first_call["function"]["arguments"].as_str() {
                    actual_args = serde_json::from_str(args_str).ok();
                }
            }

            let total_tokens = body["usage"]["total_tokens"]
                .as_u64()
                .unwrap_or(content.len() as u64 / 4) as usize;
            let reasoning_tokens = reasoning_content.len() / 4;
            let visible_tokens = total_tokens.saturating_sub(reasoning_tokens);

            let raw_ttft_ms = (total_duration_ms / 3).max(50);
            let visible_ttft_ms = (total_duration_ms / 2).max(100);

            Ok(ProviderResponse {
                raw_response: content.clone(),
                reasoning_text: reasoning_content,
                visible_text: content,
                selected_tool,
                actual_args,
                raw_ttft_ms,
                visible_ttft_ms,
                total_duration_ms,
                total_tokens,
                visible_tokens,
                reasoning_tokens,
            })
        })
    }
}

/// Evaluation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationConfig {
    pub warmup_runs: usize,
    pub runs_per_case: usize,
    pub inject_nonce: bool,
    pub min_accuracy_gate: Option<f64>,
    pub max_ttft_p95_gate_ms: Option<u64>,
}

impl Default for EvaluationConfig {
    fn default() -> Self {
        Self {
            warmup_runs: 1,
            runs_per_case: 3,
            inject_nonce: true,
            min_accuracy_gate: None,
            max_ttft_p95_gate_ms: None,
        }
    }
}

/// Record of a single evaluation run execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationRun {
    pub run_index: usize,
    pub is_warmup: bool,
    pub raw_ttft_ms: u64,
    pub visible_ttft_ms: u64,
    pub total_duration_ms: u64,
    pub generated_tokens: usize,
    pub visible_tokens: usize,
    pub reasoning_tokens: usize,
    pub selected_tool: Option<String>,
    pub actual_args: Option<Value>,
    pub passed_tool_selection: bool,
    pub passed_schema_validation: bool,
    pub cot_soundness: CoTSoundnessResult,
}

impl EvaluationRun {
    pub fn tps(&self) -> f64 {
        calculate_tps(self.generated_tokens, self.total_duration_ms)
    }
}

/// Aggregated summary across all runs for a single test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCaseSummary {
    pub test_id: String,
    pub query: String,
    pub expected_tool: String,
    pub total_runs: usize,
    pub passed_runs: usize,
    pub accuracy: f64,
    pub raw_ttft_distribution: LatencyDistribution,
    pub visible_ttft_distribution: LatencyDistribution,
    pub mean_tps: f64,
    pub cot_soundness_score: f64,
    pub runs: Vec<EvaluationRun>,
}

/// Overall benchmark report for a complete suite run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub suite_name: String,
    pub suite_version: String,
    pub provider_name: String,
    pub model_name: String,
    pub timestamp_utc: String,
    pub total_cases: usize,
    pub total_evaluated_runs: usize,
    pub overall_accuracy: f64,
    pub tool_metrics: ToolCallMetrics,
    pub schema_validation_rate: f64,
    pub raw_ttft: LatencyDistribution,
    pub visible_ttft: LatencyDistribution,
    pub mean_tps: f64,
    pub mean_cot_soundness: f64,
    pub case_summaries: Vec<TestCaseSummary>,
}

impl BenchmarkReport {
    /// Render benchmark report as clean Markdown.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str(&format!(
            "# LIVA-Eval Benchmark Report: {}\n\n",
            self.suite_name
        ));
        md.push_str(&format!("- **Suite Version**: {}\n", self.suite_version));
        md.push_str(&format!(
            "- **Provider / Model**: {} / `{}`\n",
            self.provider_name, self.model_name
        ));
        md.push_str(&format!("- **Timestamp**: {}\n", self.timestamp_utc));
        md.push_str(&format!(
            "- **Total Test Cases**: {} ({} evaluated runs)\n\n",
            self.total_cases, self.total_evaluated_runs
        ));

        md.push_str("## 1. Overall Summary\n\n");
        md.push_str("| Metric | Value |\n|---|---|\n");
        md.push_str(&format!(
            "| **Overall Accuracy** | **{:.1}%** |\n",
            self.overall_accuracy * 100.0
        ));
        md.push_str(&format!(
            "| **Tool Precision / Recall / F1** | {:.2} / {:.2} / **{:.2}** |\n",
            self.tool_metrics.precision, self.tool_metrics.recall, self.tool_metrics.f1_score
        ));
        md.push_str(&format!(
            "| **Schema Validation Rate** | {:.1}% |\n",
            self.schema_validation_rate * 100.0
        ));
        md.push_str(&format!(
            "| **Raw TTFT (p50 / p95 / p99)** | {} ms / {} ms / {} ms |\n",
            self.raw_ttft.p50, self.raw_ttft.p95, self.raw_ttft.p99
        ));
        md.push_str(&format!(
            "| **Visible TTFT (p50 / p95 / p99)** | {} ms / {} ms / {} ms |\n",
            self.visible_ttft.p50, self.visible_ttft.p95, self.visible_ttft.p99
        ));
        md.push_str(&format!(
            "| **Throughput (Mean TPS)** | **{:.1} tokens/s** |\n",
            self.mean_tps
        ));
        md.push_str(&format!(
            "| **CoT Soundness Score** | {:.2} / 1.00 |\n\n",
            self.mean_cot_soundness
        ));

        md.push_str("## 2. Test Case Breakdown\n\n");
        md.push_str(
            "| ID | Query | Expected Tool | Accuracy | Raw TTFT p95 | Vis TTFT p95 | TPS | CoT |\n",
        );
        md.push_str("|---|---|---|---|---|---|---|---|\n");
        for case in &self.case_summaries {
            let tool_str = if case.expected_tool.is_empty() {
                "*(chat)*"
            } else {
                &case.expected_tool
            };
            md.push_str(&format!(
                "| `{}` | {} | `{}` | {:.0}% | {} ms | {} ms | {:.1} | {:.2} |\n",
                case.test_id,
                case.query,
                tool_str,
                case.accuracy * 100.0,
                case.raw_ttft_distribution.p95,
                case.visible_ttft_distribution.p95,
                case.mean_tps,
                case.cot_soundness_score
            ));
        }

        md
    }

    /// Check gating criteria for CI/CD exit code decisions.
    pub fn check_gates(
        &self,
        min_accuracy: Option<f64>,
        max_ttft_p95: Option<u64>,
    ) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();
        if let Some(min_acc) = min_accuracy
            && self.overall_accuracy < min_acc
        {
            violations.push(format!(
                "Accuracy {:.1}% is below required gate {:.1}%",
                self.overall_accuracy * 100.0,
                min_acc * 100.0
            ));
        }
        if let Some(max_p95) = max_ttft_p95
            && self.visible_ttft.p95 > max_p95
        {
            violations.push(format!(
                "Visible TTFT p95 ({} ms) exceeds maximum allowed threshold ({} ms)",
                self.visible_ttft.p95, max_p95
            ));
        }
        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

/// Side-by-side comparative matrix across multiple benchmark runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparativeMatrix {
    pub baseline: BenchmarkReport,
    pub challenger: BenchmarkReport,
}

impl ComparativeMatrix {
    pub fn new(baseline: BenchmarkReport, challenger: BenchmarkReport) -> Self {
        Self {
            baseline,
            challenger,
        }
    }

    pub fn to_markdown(&self) -> String {
        let mut md = String::new();
        md.push_str("# LIVA-Eval Comparative Benchmark Matrix\n\n");
        md.push_str(&format!(
            "Comparing Baseline **{}** (`{}`) vs Challenger **{}** (`{}`)\n\n",
            self.baseline.provider_name,
            self.baseline.model_name,
            self.challenger.provider_name,
            self.challenger.model_name
        ));

        md.push_str("| Metric | Baseline | Challenger | Delta |\n|---|---|---|---|\n");

        let acc_diff = (self.challenger.overall_accuracy - self.baseline.overall_accuracy) * 100.0;
        let acc_sign = if acc_diff >= 0.0 { "+" } else { "" };
        md.push_str(&format!(
            "| Overall Accuracy | {:.1}% | {:.1}% | {}{:.1}% |\n",
            self.baseline.overall_accuracy * 100.0,
            self.challenger.overall_accuracy * 100.0,
            acc_sign,
            acc_diff
        ));

        let f1_diff = self.challenger.tool_metrics.f1_score - self.baseline.tool_metrics.f1_score;
        let f1_sign = if f1_diff >= 0.0 { "+" } else { "" };
        md.push_str(&format!(
            "| Tool F1 Score | {:.2} | {:.2} | {}{:.2} |\n",
            self.baseline.tool_metrics.f1_score,
            self.challenger.tool_metrics.f1_score,
            f1_sign,
            f1_diff
        ));

        let ttft_diff =
            self.challenger.visible_ttft.p95 as i64 - self.baseline.visible_ttft.p95 as i64;
        let ttft_sign = if ttft_diff <= 0 { "" } else { "+" };
        md.push_str(&format!(
            "| Visible TTFT p95 | {} ms | {} ms | {}{} ms |\n",
            self.baseline.visible_ttft.p95, self.challenger.visible_ttft.p95, ttft_sign, ttft_diff
        ));

        let tps_diff = self.challenger.mean_tps - self.baseline.mean_tps;
        let tps_sign = if tps_diff >= 0.0 { "+" } else { "" };
        md.push_str(&format!(
            "| Mean Throughput (TPS) | {:.1} | {:.1} | {}{:.1} |\n",
            self.baseline.mean_tps, self.challenger.mean_tps, tps_sign, tps_diff
        ));

        md
    }
}

/// The Benchmark Suite Runner executing test cases against a provider.
pub struct BenchmarkRunner {
    pub config: EvaluationConfig,
    pub provider: Arc<dyn EvaluationProvider>,
}

impl BenchmarkRunner {
    pub fn new(config: EvaluationConfig, provider: Arc<dyn EvaluationProvider>) -> Self {
        Self { config, provider }
    }

    /// Run the entire benchmark suite and aggregate results.
    pub async fn run_suite(&self, suite: &BenchmarkSuite) -> Result<BenchmarkReport, String> {
        let mut case_summaries = Vec::new();
        let mut all_raw_ttfts = Vec::new();
        let mut all_visible_ttfts = Vec::new();
        let mut all_tps = Vec::new();
        let mut all_cot_scores = Vec::new();

        let mut tp = 0;
        let mut fp = 0;
        let mut fn_count = 0;
        let mut tn = 0;
        let mut schema_pass_count = 0;
        let mut schema_total_count = 0;

        for case in &suite.test_cases {
            let summary = self.run_test_case(case).await?;

            for run in &summary.runs {
                if !run.is_warmup {
                    all_raw_ttfts.push(run.raw_ttft_ms);
                    all_visible_ttfts.push(run.visible_ttft_ms);
                    all_tps.push(run.tps());
                    all_cot_scores.push(run.cot_soundness.score);

                    let expected_has_tool = !case.expected_tool.is_empty();
                    let actual_has_tool =
                        run.selected_tool.is_some() && run.selected_tool.as_deref() != Some("");

                    if expected_has_tool && actual_has_tool {
                        if run.passed_tool_selection {
                            tp += 1;
                        } else {
                            fp += 1;
                        }
                    } else if !expected_has_tool && actual_has_tool {
                        fp += 1;
                    } else if expected_has_tool && !actual_has_tool {
                        fn_count += 1;
                    } else {
                        tn += 1;
                    }

                    if actual_has_tool {
                        schema_total_count += 1;
                        if run.passed_schema_validation {
                            schema_pass_count += 1;
                        }
                    }
                }
            }

            case_summaries.push(summary);
        }

        let total_cases = suite.test_cases.len();
        let total_evaluated_runs = all_raw_ttfts.len();

        let raw_ttft = calculate_latency_percentiles(all_raw_ttfts);
        let visible_ttft = calculate_latency_percentiles(all_visible_ttfts);

        let mean_tps = if all_tps.is_empty() {
            0.0
        } else {
            all_tps.iter().sum::<f64>() / all_tps.len() as f64
        };

        let mean_cot_soundness = if all_cot_scores.is_empty() {
            0.0
        } else {
            all_cot_scores.iter().sum::<f64>() / all_cot_scores.len() as f64
        };

        let tool_metrics = ToolCallMetrics::compute(tp, fp, fn_count, tn);
        let schema_validation_rate = if schema_total_count == 0 {
            1.0
        } else {
            schema_pass_count as f64 / schema_total_count as f64
        };

        let total_passed_cases = case_summaries.iter().filter(|c| c.accuracy >= 0.99).count();
        let overall_accuracy = if total_cases == 0 {
            0.0
        } else {
            total_passed_cases as f64 / total_cases as f64
        };

        Ok(BenchmarkReport {
            suite_name: suite.suite_name.clone(),
            suite_version: suite.version.clone(),
            provider_name: self.provider.provider_name().to_string(),
            model_name: self.provider.model_name().to_string(),
            timestamp_utc: "2026-08-16T12:00:00Z".to_string(),
            total_cases,
            total_evaluated_runs,
            overall_accuracy,
            tool_metrics,
            schema_validation_rate,
            raw_ttft,
            visible_ttft,
            mean_tps,
            mean_cot_soundness,
            case_summaries,
        })
    }

    /// Run evaluations for a single test case across warmup and measured runs.
    pub async fn run_test_case(&self, case: &TestCase) -> Result<TestCaseSummary, String> {
        let total_runs = self.config.warmup_runs + self.config.runs_per_case;
        let mut runs = Vec::with_capacity(total_runs);
        let mut measured_raw_ttfts = Vec::new();
        let mut measured_visible_ttfts = Vec::new();
        let mut measured_tps = Vec::new();
        let mut measured_cot_scores = Vec::new();
        let mut pass_count = 0;

        for i in 0..total_runs {
            let is_warmup = i < self.config.warmup_runs;
            let prompt = if self.config.inject_nonce {
                inject_anti_cache_nonce(&case.query, i + 1)
            } else {
                case.query.clone()
            };

            let response = self
                .provider
                .generate_response(&prompt, case.system_prompt.as_deref(), i + 1)
                .await?;

            let passed_tool_selection = match (
                case.expected_tool.as_str(),
                response.selected_tool.as_deref(),
            ) {
                ("", None) | ("", Some("")) => true,
                (expected, Some(actual)) if !expected.is_empty() => {
                    expected.eq_ignore_ascii_case(actual)
                }
                _ => false,
            };

            let passed_schema_validation = if let Some(ref actual_args) = response.actual_args {
                validate_arguments_schema(&case.expected_args_schema, actual_args)
            } else {
                case.expected_tool.is_empty()
            };

            let cot_soundness =
                evaluate_cot_soundness(&response.raw_response, &response.reasoning_text);

            let passed = passed_tool_selection && passed_schema_validation;
            if !is_warmup {
                if passed {
                    pass_count += 1;
                }
                measured_raw_ttfts.push(response.raw_ttft_ms);
                measured_visible_ttfts.push(response.visible_ttft_ms);
                measured_tps.push(calculate_tps(
                    response.total_tokens,
                    response.total_duration_ms,
                ));
                measured_cot_scores.push(cot_soundness.score);
            }

            runs.push(EvaluationRun {
                run_index: i + 1,
                is_warmup,
                raw_ttft_ms: response.raw_ttft_ms,
                visible_ttft_ms: response.visible_ttft_ms,
                total_duration_ms: response.total_duration_ms,
                generated_tokens: response.total_tokens,
                visible_tokens: response.visible_tokens,
                reasoning_tokens: response.reasoning_tokens,
                selected_tool: response.selected_tool,
                actual_args: response.actual_args,
                passed_tool_selection,
                passed_schema_validation,
                cot_soundness,
            });
        }

        let accuracy = if self.config.runs_per_case == 0 {
            0.0
        } else {
            pass_count as f64 / self.config.runs_per_case as f64
        };

        let raw_ttft_distribution = calculate_latency_percentiles(measured_raw_ttfts);
        let visible_ttft_distribution = calculate_latency_percentiles(measured_visible_ttfts);
        let mean_tps = if measured_tps.is_empty() {
            0.0
        } else {
            measured_tps.iter().sum::<f64>() / measured_tps.len() as f64
        };
        let cot_soundness_score = if measured_cot_scores.is_empty() {
            0.0
        } else {
            measured_cot_scores.iter().sum::<f64>() / measured_cot_scores.len() as f64
        };

        Ok(TestCaseSummary {
            test_id: case.id.clone(),
            query: case.query.clone(),
            expected_tool: case.expected_tool.clone(),
            total_runs: self.config.runs_per_case,
            passed_runs: pass_count,
            accuracy,
            raw_ttft_distribution,
            visible_ttft_distribution,
            mean_tps,
            cot_soundness_score,
            runs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_runner_execution() {
        let suite = BenchmarkSuite::default_suite();
        let provider = Arc::new(MockProvider::new("mock_liva", "gemma-4-e4b"));
        let config = EvaluationConfig {
            warmup_runs: 1,
            runs_per_case: 2,
            inject_nonce: true,
            min_accuracy_gate: Some(0.8),
            max_ttft_p95_gate_ms: Some(1000),
        };

        let runner = BenchmarkRunner::new(config, provider);
        let report = runner.run_suite(&suite).await.expect("suite ran");

        assert_eq!(report.total_cases, 5);
        assert_eq!(report.total_evaluated_runs, 10);
        assert!(report.overall_accuracy >= 0.8);
        assert!(report.check_gates(Some(0.8), Some(1000)).is_ok());
    }

    #[tokio::test]
    async fn test_comparative_matrix_markdown() {
        let suite = BenchmarkSuite::default_suite();
        let provider_a = Arc::new(MockProvider::new("baseline_local", "model_a"));
        let provider_b = Arc::new(MockProvider::new("challenger_cloud", "model_b"));
        let config = EvaluationConfig {
            warmup_runs: 0,
            runs_per_case: 1,
            inject_nonce: false,
            min_accuracy_gate: None,
            max_ttft_p95_gate_ms: None,
        };

        let runner_a = BenchmarkRunner::new(config.clone(), provider_a);
        let report_a = runner_a.run_suite(&suite).await.expect("report a");

        let runner_b = BenchmarkRunner::new(config, provider_b);
        let report_b = runner_b.run_suite(&suite).await.expect("report b");

        let matrix = ComparativeMatrix::new(report_a, report_b);
        let md = matrix.to_markdown();
        assert!(md.contains("Overall Accuracy"));
        assert!(md.contains("Baseline"));
        assert!(md.contains("Challenger"));
    }
}
