use serde::{Deserialize, Serialize};

/// High-level status of a tool execution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ObservationStatus {
    #[default]
    Success,
    Failure,
    Unknown,
    DryRun,
}

/// A verified physical or stateful side-effect recorded during execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SideEffectRecord {
    /// The target resource modified (e.g. "vault/daily.md", "lights:living_room", "telegram_outbox").
    pub target_resource: String,
    /// Type of effect (e.g. "file_appended", "device_toggled", "message_queued").
    pub effect_type: String,
    /// Whether the side effect was independently verified via read-back or status confirmation.
    pub verified: bool,
}

/// Execution metrics and executor provenance for audit trails.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditTrace {
    /// Execution wall-clock time in milliseconds.
    pub execution_duration_ms: u64,
    /// Principal or caller that executed the tool (e.g. "liva_core", "local_user", "telegram_bridge").
    pub executor_principal: String,
    /// Optional exit code or HTTP status code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i32>,
    /// Optional memory allocation delta in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_delta_bytes: Option<i64>,
}

impl Default for AuditTrace {
    fn default() -> Self {
        Self {
            execution_duration_ms: 0,
            executor_principal: "liva_core".to_string(),
            status_code: None,
            memory_delta_bytes: None,
        }
    }
}

/// The canonical result and verified observation of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolObservation {
    /// Correlation ID matching the originating ActionProposal.
    pub action_id: String,
    /// The tool or command that was executed.
    pub tool_id: String,
    /// Boolean success flag.
    pub success: bool,
    /// Detailed status classification.
    pub status: ObservationStatus,
    /// Textual output sanitized against prompt injection tags, control chars, and secrets.
    pub output_sanitized: String,
    /// Error message if execution failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Whether this failure is transient and can be retried safely.
    pub retryable: bool,
    /// Optional structured state delta resulting from the operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_diff: Option<serde_json::Value>,
    /// Verified real-world side effects.
    pub real_side_effects: Vec<SideEffectRecord>,
    /// Audit and performance metrics.
    pub audit_trace: AuditTrace,
    /// Completion timestamp in epoch milliseconds.
    pub timestamp_ms: i64,
}

use regex::Regex;
use std::sync::LazyLock;

pub const MAX_OBSERVATION_BYTES: usize = 256 * 1024; // 256KB

static RE_SYSTEM_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<\s*system(?:\s+[^>]*)?>").expect("valid regex"));
static RE_SYSTEM_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<\s*/\s*system\s*>").expect("valid regex"));
static RE_THINK_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<\s*think(?:\s+[^>]*)?>").expect("valid regex"));
static RE_THINK_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<\s*/\s*think\s*>").expect("valid regex"));
static RE_INST_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[\s*INST\s*\]").expect("valid regex"));
static RE_INST_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[\s*/\s*INST\s*\]").expect("valid regex"));

impl ToolObservation {
    /// Sanitizes raw tool output string by removing prompt injection delimiters, control chars, and null bytes,
    /// and truncates output exceeding 256KB.
    pub fn sanitize_output(raw: &str) -> String {
        let (truncated_raw, was_truncated) = if raw.len() > MAX_OBSERVATION_BYTES {
            let mut cut = MAX_OBSERVATION_BYTES;
            while cut > 0 && !raw.is_char_boundary(cut) {
                cut -= 1;
            }
            (&raw[..cut], true)
        } else {
            (raw, false)
        };

        let mut clean = String::with_capacity(truncated_raw.len() + 64);
        for c in truncated_raw.chars() {
            if c == '\0' {
                continue; // strip null bytes
            }
            if c.is_control() && c != '\n' && c != '\r' && c != '\t' {
                continue; // strip non-whitespace control characters
            }
            clean.push(c);
        }

        // Neutralize system instruction injection tags & multi-model delimiters
        let s1 = RE_SYSTEM_OPEN.replace_all(&clean, "[system]");
        let s2 = RE_SYSTEM_CLOSE.replace_all(&s1, "[/system]");
        let s3 = RE_THINK_OPEN.replace_all(&s2, "[think]");
        let s4 = RE_THINK_CLOSE.replace_all(&s3, "[/think]");
        let s5 = RE_INST_OPEN.replace_all(&s4, "[inst]");
        let s6 = RE_INST_CLOSE.replace_all(&s5, "[/inst]");

        let mut final_str = s6
            .replace("<|im_start|>", "[im_start]")
            .replace("<|im_end|>", "[im_end]")
            .replace("<|start_header_id|>", "[start_header_id]")
            .replace("<|end_header_id|>", "[end_header_id]")
            .replace("<|eot_id|>", "[eot_id]")
            .replace("<start_of_turn>", "[start_of_turn]")
            .replace("<end_of_turn>", "[end_of_turn]")
            .replace("<|system|>", "[system]")
            .replace("<|user|>", "[user]")
            .replace("<|assistant|>", "[assistant]")
            .replace("<|endoftext|>", "[endoftext]");

        if was_truncated {
            final_str.push_str("\n[TRUNCATED: output exceeded max bytes]");
        }

        final_str
    }

    /// Constructs a successful ToolObservation.
    pub fn success(
        action_id: impl Into<String>,
        tool_id: impl Into<String>,
        raw_output: &str,
        duration_ms: u64,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        Self {
            action_id: action_id.into(),
            tool_id: tool_id.into(),
            success: true,
            status: ObservationStatus::Success,
            output_sanitized: Self::sanitize_output(raw_output),
            error: None,
            retryable: false,
            state_diff: None,
            real_side_effects: Vec::new(),
            audit_trace: AuditTrace {
                execution_duration_ms: duration_ms,
                executor_principal: "liva_core".to_string(),
                status_code: Some(0),
                memory_delta_bytes: None,
            },
            timestamp_ms: now_ms,
        }
    }

    /// Constructs a failed ToolObservation.
    pub fn failure(
        action_id: impl Into<String>,
        tool_id: impl Into<String>,
        error_msg: impl Into<String>,
        retryable: bool,
        duration_ms: u64,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let err_s = error_msg.into();
        Self {
            action_id: action_id.into(),
            tool_id: tool_id.into(),
            success: false,
            status: ObservationStatus::Failure,
            output_sanitized: String::new(),
            error: Some(err_s),
            retryable,
            state_diff: None,
            real_side_effects: Vec::new(),
            audit_trace: AuditTrace {
                execution_duration_ms: duration_ms,
                executor_principal: "liva_core".to_string(),
                status_code: Some(1),
                memory_delta_bytes: None,
            },
            timestamp_ms: now_ms,
        }
    }

    /// Fluent method to record a verified side effect.
    pub fn with_side_effect(
        mut self,
        target_resource: impl Into<String>,
        effect_type: impl Into<String>,
        verified: bool,
    ) -> Self {
        self.real_side_effects.push(SideEffectRecord {
            target_resource: target_resource.into(),
            effect_type: effect_type.into(),
            verified,
        });
        self
    }

    /// Fluent method to attach state diff.
    pub fn with_state_diff(mut self, diff: serde_json::Value) -> Self {
        self.state_diff = Some(diff);
        self
    }
}
