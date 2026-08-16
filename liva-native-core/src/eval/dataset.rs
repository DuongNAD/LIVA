//! Benchmark Suite and Dataset Parser for LIVA-Eval (Features F10, F11).
//!
//! Provides dataset parsing, JSON schema validation, test case filtering,
//! and anti-cache nonce injection (`[eval_nonce #N]` / `[phiên #N]`).

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;
use uuid::Uuid;

/// An individual test case within a benchmark suite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestCase {
    pub id: String,
    pub query: String,
    #[serde(default, deserialize_with = "deserialize_expected_tool")]
    pub expected_tool: String,
    #[serde(default, deserialize_with = "deserialize_args_schema")]
    pub expected_args_schema: Value,
    #[serde(default)]
    pub expected_args: Option<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn deserialize_expected_tool<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    if let Some(s) = val.as_str() {
        return Ok(s.to_string());
    }
    if let Some(tool_name) = val
        .as_object()
        .and_then(|obj| obj.get("tool_name"))
        .and_then(|v| v.as_str())
    {
        return Ok(tool_name.to_string());
    }
    Ok(String::new())
}

fn deserialize_args_schema<'de, D>(deserializer: D) -> Result<Value, D::Error>
where
    D: Deserializer<'de>,
{
    let val = Value::deserialize(deserializer)?;
    if val.is_null() {
        Ok(serde_json::json!({}))
    } else {
        Ok(val)
    }
}

/// A collection of benchmark test cases forming an evaluation suite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub suite_name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub test_cases: Vec<TestCase>,
}

impl BenchmarkSuite {
    /// Parse a benchmark suite from JSON string.
    pub fn from_json_str(json_str: &str) -> Result<Self, String> {
        let suite: Self = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to parse benchmark suite JSON: {e}"))?;
        suite.validate()?;
        Ok(suite)
    }

    /// Load a benchmark suite from a JSON file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path_ref = path.as_ref();
        let content = std::fs::read_to_string(path_ref)
            .map_err(|e| format!("Failed to read suite file {:?}: {e}", path_ref))?;
        Self::from_json_str(&content)
    }

    /// Serialize the benchmark suite to formatted JSON.
    pub fn to_json_str(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize suite to JSON: {e}"))
    }

    /// Validate the benchmark suite integrity.
    pub fn validate(&self) -> Result<(), String> {
        if self.suite_name.trim().is_empty() {
            return Err("Benchmark suite name cannot be empty".to_string());
        }
        if self.test_cases.is_empty() {
            return Err("Benchmark suite contains 0 test cases".to_string());
        }

        let mut seen_ids = HashSet::new();
        for case in &self.test_cases {
            if case.id.trim().is_empty() {
                return Err("Test case ID cannot be empty".to_string());
            }
            if !seen_ids.insert(&case.id) {
                return Err(format!("Duplicate test case ID found: '{}'", case.id));
            }
            if case.query.trim().is_empty() {
                return Err(format!("Test case '{}' has an empty query", case.id));
            }
        }
        Ok(())
    }

    /// Filter test cases by tag matching.
    pub fn filter_by_tags(&self, required_tags: &[&str]) -> Self {
        if required_tags.is_empty() {
            return self.clone();
        }
        let filtered_cases: Vec<TestCase> = self
            .test_cases
            .iter()
            .filter(|case| {
                required_tags
                    .iter()
                    .any(|&tag| case.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            })
            .cloned()
            .collect();

        Self {
            suite_name: format!("{}_filtered", self.suite_name),
            version: self.version.clone(),
            description: self.description.clone(),
            test_cases: filtered_cases,
        }
    }

    /// Filter test cases by category.
    pub fn filter_by_category(&self, category: &str) -> Self {
        let filtered_cases: Vec<TestCase> = self
            .test_cases
            .iter()
            .filter(|case| {
                case.category
                    .as_deref()
                    .map(|c| c.eq_ignore_ascii_case(category))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        Self {
            suite_name: format!("{}_{category}", self.suite_name),
            version: self.version.clone(),
            description: self.description.clone(),
            test_cases: filtered_cases,
        }
    }

    /// Get a test case by its ID.
    pub fn get_case(&self, id: &str) -> Option<&TestCase> {
        self.test_cases.iter().find(|c| c.id == id)
    }

    /// Create default authoritative LIVA core benchmark suite.
    pub fn default_suite() -> Self {
        Self {
            suite_name: "liva_core_v1".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Authoritative LIVA Core Evaluation Benchmark Suite".to_string()),
            test_cases: vec![
                TestCase {
                    id: "TC-SMARTHOME-01".to_string(),
                    query: "Bật đèn phòng khách giúp mình".to_string(),
                    expected_tool: "control_smarthome".to_string(),
                    expected_args_schema: serde_json::json!({
                        "type": "object",
                        "required": ["device", "action"],
                        "properties": {
                            "device": { "type": "string" },
                            "action": { "type": "string" }
                        }
                    }),
                    expected_args: Some(serde_json::json!({ "device": "light", "action": "on" })),
                    tags: vec!["smarthome".to_string(), "vietnamese".to_string()],
                    category: Some("smarthome".to_string()),
                    system_prompt: None,
                },
                TestCase {
                    id: "TC-SMARTHOME-02".to_string(),
                    query: "Tắt quạt trần đi".to_string(),
                    expected_tool: "control_smarthome".to_string(),
                    expected_args_schema: serde_json::json!({
                        "type": "object",
                        "required": ["device", "action"]
                    }),
                    expected_args: Some(serde_json::json!({ "device": "fan", "action": "off" })),
                    tags: vec!["smarthome".to_string(), "vietnamese".to_string()],
                    category: Some("smarthome".to_string()),
                    system_prompt: None,
                },
                TestCase {
                    id: "TC-VAULT-01".to_string(),
                    query: "Tìm trong ghi chú vault xem cuộc họp hôm qua có nội dung gì"
                        .to_string(),
                    expected_tool: "search_vault".to_string(),
                    expected_args_schema: serde_json::json!({
                        "type": "object",
                        "required": ["query"]
                    }),
                    expected_args: Some(serde_json::json!({ "query": "cuộc họp" })),
                    tags: vec!["vault".to_string(), "rag".to_string()],
                    category: Some("vault".to_string()),
                    system_prompt: None,
                },
                TestCase {
                    id: "TC-OS-01".to_string(),
                    query: "Tăng âm lượng máy tính lên 80%".to_string(),
                    expected_tool: "set_volume".to_string(),
                    expected_args_schema: serde_json::json!({
                        "type": "object",
                        "required": ["level"]
                    }),
                    expected_args: Some(serde_json::json!({ "level": 80 })),
                    tags: vec!["os_control".to_string()],
                    category: Some("os_control".to_string()),
                    system_prompt: None,
                },
                TestCase {
                    id: "TC-CHAT-01".to_string(),
                    query: "Hôm nay thời tiết thế nào nhỉ?".to_string(),
                    expected_tool: "".to_string(),
                    expected_args_schema: serde_json::json!({}),
                    expected_args: None,
                    tags: vec!["conversation".to_string()],
                    category: Some("chat".to_string()),
                    system_prompt: None,
                },
            ],
        }
    }
}

/// Inject an anti-cache nonce into the prompt to bypass prefix caching during benchmarks.
pub fn inject_anti_cache_nonce(prompt: &str, iteration: usize) -> String {
    let nonce = Uuid::new_v4().to_string();
    format!("{prompt}\n\n[eval_nonce #{iteration}: {nonce}]")
}

/// Inject a Vietnamese anti-cache nonce into the prompt.
pub fn inject_vietnamese_nonce(prompt: &str, iteration: usize) -> String {
    let nonce = Uuid::new_v4().to_string();
    format!("[phiên #{iteration}: {nonce}] {prompt}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_benchmark_suite_serialization_roundtrip() {
        let default_suite = BenchmarkSuite::default_suite();
        let json_str = default_suite.to_json_str().expect("serialized");
        let parsed = BenchmarkSuite::from_json_str(&json_str).expect("deserialized");

        assert_eq!(default_suite.suite_name, parsed.suite_name);
        assert_eq!(default_suite.test_cases.len(), parsed.test_cases.len());
        assert_eq!(default_suite.test_cases[0].id, parsed.test_cases[0].id);
    }

    #[test]
    fn test_suite_validation_errors() {
        let empty_suite = BenchmarkSuite {
            suite_name: "".into(),
            version: "1.0".into(),
            description: None,
            test_cases: vec![],
        };
        assert!(empty_suite.validate().is_err());

        let duplicate_id_suite = BenchmarkSuite {
            suite_name: "suite".into(),
            version: "1.0".into(),
            description: None,
            test_cases: vec![
                TestCase {
                    id: "dup".into(),
                    query: "query 1".into(),
                    expected_tool: "".into(),
                    expected_args_schema: json!({}),
                    expected_args: None,
                    tags: vec![],
                    category: None,
                    system_prompt: None,
                },
                TestCase {
                    id: "dup".into(),
                    query: "query 2".into(),
                    expected_tool: "".into(),
                    expected_args_schema: json!({}),
                    expected_args: None,
                    tags: vec![],
                    category: None,
                    system_prompt: None,
                },
            ],
        };
        assert!(duplicate_id_suite.validate().is_err());
    }

    #[test]
    fn test_suite_tag_and_category_filtering() {
        let suite = BenchmarkSuite::default_suite();
        let smarthome_suite = suite.filter_by_tags(&["smarthome"]);
        assert_eq!(smarthome_suite.test_cases.len(), 2);

        let vault_suite = suite.filter_by_category("vault");
        assert_eq!(vault_suite.test_cases.len(), 1);
        assert_eq!(vault_suite.test_cases[0].id, "TC-VAULT-01");
    }

    #[test]
    fn test_anti_cache_nonce_uniqueness() {
        let prompt = "Kiểm tra hệ thống";
        let n1 = inject_anti_cache_nonce(prompt, 1);
        let n2 = inject_anti_cache_nonce(prompt, 2);

        assert!(n1.starts_with(prompt));
        assert!(n2.starts_with(prompt));
        assert!(n1.contains("[eval_nonce #1:"));
        assert!(n2.contains("[eval_nonce #2:"));
        assert_ne!(n1, n2);
    }
}
