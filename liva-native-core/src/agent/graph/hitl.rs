use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Human-In-The-Loop (HITL) approval context required for high-risk / destructive operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApprovalContext {
    pub action_id: String,
    pub tool_name: String,
    pub arguments: Value,
    pub reason: String,
    pub timeout_secs: u64,
    #[serde(default)]
    pub created_at: i64,
}

impl ApprovalContext {
    pub fn new(
        action_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments: Value,
        reason: impl Into<String>,
        timeout_secs: u64,
    ) -> Self {
        let created_at = Utc::now().timestamp_millis();
        Self {
            action_id: action_id.into(),
            tool_name: tool_name.into(),
            arguments,
            reason: reason.into(),
            timeout_secs,
            created_at,
        }
    }

    /// Check whether the approval request has exceeded its deadline.
    pub fn is_expired(&self, current_time_ms: i64) -> bool {
        let timeout_ms = (self.timeout_secs as i64) * 1000;
        current_time_ms > self.created_at + timeout_ms
    }

    /// Check expiration against current system UTC clock.
    pub fn is_expired_now(&self) -> bool {
        self.is_expired(Utc::now().timestamp_millis())
    }
}

/// The decision provided by the user via Tauri IPC or external approval hook.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum ApprovalDecision {
    Approved {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        modified_args: Option<Value>,
    },
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    TimedOut,
}

/// Lifecycle status for checkpoints and graph execution state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CheckpointStatus {
    Active,
    Suspended,
    Completed,
    Failed,
}

impl CheckpointStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Suspended => "SUSPENDED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "SUSPENDED" => Self::Suspended,
            "COMPLETED" => Self::Completed,
            "FAILED" => Self::Failed,
            _ => Self::Active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_approval_context_creation_and_expiration() {
        let ctx = ApprovalContext::new(
            "act-123",
            "execute_bash",
            json!({"command": "rm -rf /tmp/test"}),
            "Destructive command execution requires confirmation",
            30,
        );

        assert_eq!(ctx.action_id, "act-123");
        assert_eq!(ctx.tool_name, "execute_bash");
        assert!(!ctx.is_expired(ctx.created_at + 10_000));
        assert!(ctx.is_expired(ctx.created_at + 31_000));
    }

    #[test]
    fn test_approval_decision_serialization() {
        let approved = ApprovalDecision::Approved {
            modified_args: Some(json!({"command": "rm -rf /tmp/safe"})),
        };
        let serialized = serde_json::to_string(&approved).expect("serialize");
        let deserialized: ApprovalDecision = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(approved, deserialized);

        let rejected = ApprovalDecision::Rejected {
            reason: Some("Operation denied by admin".to_string()),
        };
        let serialized_rej = serde_json::to_string(&rejected).expect("serialize");
        let deserialized_rej: ApprovalDecision =
            serde_json::from_str(&serialized_rej).expect("deserialize");
        assert_eq!(rejected, deserialized_rej);
    }

    #[test]
    fn test_checkpoint_status_conversions() {
        assert_eq!(CheckpointStatus::Active.as_str(), "ACTIVE");
        assert_eq!(CheckpointStatus::Suspended.as_str(), "SUSPENDED");
        assert_eq!(CheckpointStatus::Completed.as_str(), "COMPLETED");
        assert_eq!(CheckpointStatus::Failed.as_str(), "FAILED");

        assert_eq!(CheckpointStatus::from_str("ACTIVE"), CheckpointStatus::Active);
        assert_eq!(CheckpointStatus::from_str("suspended"), CheckpointStatus::Suspended);
        assert_eq!(CheckpointStatus::from_str("COMPLETED"), CheckpointStatus::Completed);
        assert_eq!(CheckpointStatus::from_str("failed"), CheckpointStatus::Failed);
        assert_eq!(CheckpointStatus::from_str("unknown"), CheckpointStatus::Active);
    }
}
