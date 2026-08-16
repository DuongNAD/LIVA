//! Mathematical metrics and evaluation calculations for LIVA-Eval (Feature F10).
//!
//! Provides calculations for:
//! - Time To First Token (Raw TTFT vs Visible TTFT)
//! - Tokens Per Second (TPS) with zero-duration protection
//! - Tool Call Accuracy (Precision, Recall, F1 Score)
//! - JSON Argument Schema Validation
//! - Chain-of-Thought (CoT) Soundness

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// Percentile metric calculation using the nearest-rank method.
pub fn calculate_percentile(sorted_data: &[u64], percentile: f64) -> u64 {
    if sorted_data.is_empty() {
        return 0;
    }
    if percentile <= 0.0 {
        return sorted_data[0];
    }
    if percentile >= 100.0 {
        return sorted_data[sorted_data.len() - 1];
    }
    let n = sorted_data.len();
    let rank = ((percentile / 100.0) * n as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    sorted_data[idx]
}

/// Calculate p50, p90, p95, p99 percentiles from a list of latencies in ms.
pub fn calculate_latency_percentiles(mut latencies: Vec<u64>) -> LatencyDistribution {
    if latencies.is_empty() {
        return LatencyDistribution::default();
    }
    latencies.sort_unstable();

    let count = latencies.len();
    let min = latencies[0];
    let max = latencies[count - 1];
    let sum: u64 = latencies.iter().sum();
    let mean = sum as f64 / count as f64;

    let variance: f64 = latencies
        .iter()
        .map(|&v| {
            let diff = v as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / count as f64;
    let std_dev = variance.sqrt();

    let p50 = calculate_percentile(&latencies, 50.0);
    let p90 = calculate_percentile(&latencies, 90.0);
    let p95 = calculate_percentile(&latencies, 95.0);
    let p99 = calculate_percentile(&latencies, 99.0);

    LatencyDistribution {
        p50,
        p90,
        p95,
        p99,
        min,
        max,
        mean,
        std_dev,
        count,
    }
}

/// Summary of latency distribution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LatencyDistribution {
    pub p50: u64,
    pub p90: u64,
    pub p95: u64,
    pub p99: u64,
    pub min: u64,
    pub max: u64,
    pub mean: f64,
    pub std_dev: f64,
    pub count: usize,
}

/// Calculate throughput in Tokens Per Second (TPS).
///
/// Guarded against zero or negative duration: returns `0.0` if duration_ms is 0.
pub fn calculate_tps(token_count: usize, duration_ms: u64) -> f64 {
    if duration_ms == 0 || token_count == 0 {
        return 0.0;
    }
    let duration_sec = duration_ms as f64 / 1000.0;
    token_count as f64 / duration_sec
}

/// Calculate visible generation throughput excluding the initial token latency.
pub fn calculate_visible_tps(visible_tokens: usize, generation_duration_ms: u64) -> f64 {
    if generation_duration_ms == 0 || visible_tokens <= 1 {
        return 0.0;
    }
    let token_count = visible_tokens - 1;
    let duration_sec = generation_duration_ms as f64 / 1000.0;
    token_count as f64 / duration_sec
}

/// Tool Call classification metrics (Precision, Recall, F1, Accuracy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ToolCallMetrics {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
    pub true_negatives: usize,
    pub total: usize,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub accuracy: f64,
}

impl ToolCallMetrics {
    pub fn compute(
        true_positives: usize,
        false_positives: usize,
        false_negatives: usize,
        true_negatives: usize,
    ) -> Self {
        let total = true_positives + false_positives + false_negatives + true_negatives;

        let precision = if true_positives + false_positives == 0 {
            if true_positives + false_negatives == 0 {
                1.0
            } else {
                0.0
            }
        } else {
            true_positives as f64 / (true_positives + false_positives) as f64
        };

        let recall = if true_positives + false_negatives == 0 {
            1.0
        } else {
            true_positives as f64 / (true_positives + false_negatives) as f64
        };

        let f1_score = if precision + recall < 1e-9 {
            0.0
        } else {
            2.0 * (precision * recall) / (precision + recall)
        };

        let accuracy = if total == 0 {
            0.0
        } else {
            (true_positives + true_negatives) as f64 / total as f64
        };

        Self {
            true_positives,
            false_positives,
            false_negatives,
            true_negatives,
            total,
            precision,
            recall,
            f1_score,
            accuracy,
        }
    }
}

/// JSON Schema Argument Validator.
///
/// Validates whether the actual tool call arguments conform to the expected JSON schema.
pub fn validate_arguments_schema(expected_schema: &Value, actual_args: &Value) -> bool {
    validate_json_value(expected_schema, actual_args)
}

fn validate_json_value(schema: &Value, val: &Value) -> bool {
    let Some(schema_obj) = schema.as_object() else {
        return true;
    };

    // 1. Check type constraint if specified
    if let Some(type_val) = schema_obj.get("type").and_then(|t| t.as_str()) {
        let type_match = match type_val {
            "object" => val.is_object(),
            "array" => val.is_array(),
            "string" => val.is_string(),
            "number" => val.is_number(),
            "integer" => val.is_i64() || val.is_u64(),
            "boolean" => val.is_boolean(),
            "null" => val.is_null(),
            _ => true,
        };
        if !type_match {
            return false;
        }
    }

    // 2. Check enum constraint
    if let Some(enum_vals) = schema_obj.get("enum").and_then(|e| e.as_array())
        && !enum_vals.contains(val)
    {
        return false;
    }

    // 3. Object-specific validations
    if let Some(val_obj) = val.as_object() {
        // Check required fields
        if let Some(required) = schema_obj.get("required").and_then(|r| r.as_array()) {
            for req in required {
                if let Some(field_name) = req.as_str()
                    && !val_obj.contains_key(field_name)
                {
                    return false;
                }
            }
        }

        // Validate properties recursively if present
        if let Some(props) = schema_obj.get("properties").and_then(|p| p.as_object()) {
            for (key, prop_schema) in props {
                if let Some(field_val) = val_obj.get(key)
                    && !validate_json_value(prop_schema, field_val)
                {
                    return false;
                }
            }
        }
    }

    // 4. Array-specific validations
    if let Some(val_arr) = val.as_array()
        && let Some(items_schema) = schema_obj.get("items")
    {
        for item in val_arr {
            if !validate_json_value(items_schema, item) {
                return false;
            }
        }
    }

    true
}

/// Evaluation of Chain-of-Thought (CoT) / reasoning soundness.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CoTSoundnessResult {
    pub is_sound: bool,
    pub score: f64,
    pub has_balanced_tags: bool,
    pub reasoning_token_count: usize,
    pub step_indicators_found: usize,
    pub issues: Vec<String>,
}

/// Evaluate the soundness and structural integrity of CoT reasoning trace.
pub fn evaluate_cot_soundness(raw_response: &str, reasoning_text: &str) -> CoTSoundnessResult {
    let mut issues = Vec::new();
    let mut score_points = 0.0;
    const MAX_POINTS: f64 = 4.0;

    // 1. Tag balance check in raw response
    let open_think_count = raw_response.matches("<think>").count()
        + raw_response.matches("<thought>").count()
        + raw_response.matches("<analysis>").count()
        + raw_response.matches("<|channel|>analysis").count();

    let close_think_count = raw_response.matches("</think>").count()
        + raw_response.matches("</thought>").count()
        + raw_response.matches("</analysis>").count()
        + raw_response.matches("<|channel|>final").count();

    let has_balanced_tags = open_think_count == close_think_count;
    if has_balanced_tags {
        score_points += 1.0;
    } else {
        issues.push(format!(
            "Unbalanced reasoning tags: {open_think_count} open vs {close_think_count} close"
        ));
    }

    // 2. Non-empty reasoning check
    let trimmed_reasoning = reasoning_text.trim();
    let reasoning_token_estimate =
        (trimmed_reasoning.len() / 4).max(if trimmed_reasoning.is_empty() { 0 } else { 1 });
    if !trimmed_reasoning.is_empty() {
        score_points += 1.0;
    } else {
        issues.push("Reasoning trace is empty".to_string());
    }

    // 3. Step deliberation indicators check
    let step_indicators = [
        "bước",
        "step",
        "đầu tiên",
        "first",
        "tiếp theo",
        "next",
        "phân tích",
        "analyze",
        "bởi vì",
        "because",
        "do đó",
        "therefore",
        "suy nghĩ",
        "cần gọi",
        "tool",
        "tham số",
        "giải thích",
    ];
    let lower_reasoning = trimmed_reasoning.to_lowercase();
    let step_count = step_indicators
        .iter()
        .filter(|&&ind| lower_reasoning.contains(ind))
        .count();

    if step_count >= 1 {
        score_points += 1.0;
    }
    if step_count >= 2 {
        score_points += 1.0;
    }

    let final_score = (score_points / MAX_POINTS).clamp(0.0, 1.0);
    let is_sound = has_balanced_tags && !trimmed_reasoning.is_empty() && final_score >= 0.5;

    CoTSoundnessResult {
        is_sound,
        score: final_score,
        has_balanced_tags,
        reasoning_token_count: reasoning_token_estimate,
        step_indicators_found: step_count,
        issues,
    }
}

/// Timing metrics for a single generation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingMetrics {
    pub raw_ttft: Duration,
    pub visible_ttft: Option<Duration>,
    pub total_duration: Duration,
    pub generated_tokens: usize,
    pub visible_tokens: usize,
    pub reasoning_tokens: usize,
}

impl TimingMetrics {
    pub fn raw_ttft_ms(&self) -> u64 {
        self.raw_ttft.as_millis() as u64
    }

    pub fn visible_ttft_ms(&self) -> u64 {
        self.visible_ttft.unwrap_or(self.total_duration).as_millis() as u64
    }

    pub fn total_duration_ms(&self) -> u64 {
        self.total_duration.as_millis() as u64
    }

    pub fn tps(&self) -> f64 {
        calculate_tps(self.generated_tokens, self.total_duration_ms())
    }

    pub fn visible_tps(&self) -> f64 {
        calculate_visible_tps(self.visible_tokens, self.total_duration_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_percentile_calculation_robustness() {
        let empty: Vec<u64> = vec![];
        assert_eq!(calculate_percentile(&empty, 50.0), 0);

        let single = vec![42];
        assert_eq!(calculate_percentile(&single, 50.0), 42);
        assert_eq!(calculate_percentile(&single, 95.0), 42);

        let data = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(calculate_percentile(&data, 50.0), 50);
        assert_eq!(calculate_percentile(&data, 90.0), 90);
        assert_eq!(calculate_percentile(&data, 95.0), 100);
        assert_eq!(calculate_percentile(&data, 99.0), 100);
    }

    #[test]
    fn test_latency_distribution_statistics() {
        let latencies = vec![100, 200, 300, 400, 500];
        let dist = calculate_latency_percentiles(latencies);
        assert_eq!(dist.min, 100);
        assert_eq!(dist.max, 500);
        assert_eq!(dist.p50, 300);
        assert_eq!(dist.count, 5);
        assert!((dist.mean - 300.0).abs() < 1e-6);
    }

    #[test]
    fn test_zero_duration_tps_protection() {
        assert_eq!(calculate_tps(100, 0), 0.0);
        assert_eq!(calculate_visible_tps(100, 0), 0.0);
        assert_eq!(calculate_tps(0, 1000), 0.0);
        assert_eq!(calculate_visible_tps(1, 1000), 0.0);
    }

    #[test]
    fn test_tool_call_metrics_precision_recall_f1() {
        // 8 TP, 2 FP, 0 FN, 0 TN
        let metrics = ToolCallMetrics::compute(8, 2, 0, 0);
        assert!((metrics.precision - 0.8).abs() < 1e-6);
        assert!((metrics.recall - 1.0).abs() < 1e-6);
        let expected_f1 = 2.0 * (0.8 * 1.0) / (0.8 + 1.0);
        assert!((metrics.f1_score - expected_f1).abs() < 1e-6);
    }

    #[test]
    fn test_argument_schema_validation_types_and_enums() {
        let schema = json!({
            "type": "object",
            "required": ["action", "level"],
            "properties": {
                "action": { "type": "string", "enum": ["set", "mute"] },
                "level": { "type": "integer" }
            }
        });

        assert!(validate_arguments_schema(
            &schema,
            &json!({ "action": "set", "level": 50 })
        ));
        assert!(!validate_arguments_schema(
            &schema,
            &json!({ "action": "invalid", "level": 50 })
        ));
        assert!(!validate_arguments_schema(
            &schema,
            &json!({ "action": "set" })
        )); // missing level
        assert!(!validate_arguments_schema(
            &schema,
            &json!({ "action": "set", "level": "fifty" })
        )); // wrong type
    }

    #[test]
    fn test_cot_soundness_evaluation() {
        let raw = "<think>Đầu tiên phân tích câu hỏi người dùng, sau đó chọn tool phù hợp.</think>Chào bạn!";
        let cot = "Đầu tiên phân tích câu hỏi người dùng, sau đó chọn tool phù hợp.";
        let res = evaluate_cot_soundness(raw, cot);
        assert!(res.is_sound);
        assert!(res.has_balanced_tags);
        assert!(res.score >= 0.75);

        let broken_raw = "<think>Chưa đóng thẻ suy luận";
        let res_broken = evaluate_cot_soundness(broken_raw, "Chưa đóng thẻ suy luận");
        assert!(!res_broken.has_balanced_tags);
        assert!(!res_broken.is_sound);
    }
}
