//! Executor Policy Gate: Enforces sink sensitivity, Rule-of-Two, and Taint restrictions (Feature F20).

use crate::authorization::taint::{DataOrigin, DeclassificationReceipt};
use crate::cognitive::policy::{PolicyEngine, RiskTier};
use liva_core_types::{SecurityError, SessionMode};

pub struct ExecutorPolicy;

impl ExecutorPolicy {
    /// Kiểm tra toàn diện trước khi cho phép một công cụ chạm tới Sink thực thi.
    pub fn enforce_sink_call(
        session_mode: SessionMode,
        tool_name: &str,
        arguments_json: &serde_json::Value,
        is_argument_tainted: bool,
        _origin: Option<&DataOrigin>,
        receipt: Option<&DeclassificationReceipt>,
    ) -> Result<(), SecurityError> {
        // 1. Phân loại độ nhạy của Sink dựa vào PolicyEngine
        let risk_tier = PolicyEngine::classify_tool(tool_name);

        // 2. Kiểm tra chế độ phiên Rule-of-Two (F19)
        if session_mode == SessionMode::Reader {
            let is_mutation = risk_tier >= RiskTier::Reversible
                || tool_name.starts_with("send_")
                || tool_name.starts_with("write_")
                || tool_name.starts_with("delete_")
                || tool_name.starts_with("control_")
                || tool_name.contains("mutate")
                || tool_name.contains("webhook")
                || tool_name.contains("powershell")
                || tool_name.contains("shell")
                || tool_name.contains("POST");
            if is_mutation {
                return Err(SecurityError::MutationForbiddenInReaderMode(format!(
                    "ExecutorPolicy: Công cụ '{tool_name}' có rủi ro '{risk_tier:?}' (đột biến ngoài), bị cấm tuyệt đối trong phiên Reader"
                )));
            }
        }

        if session_mode == SessionMode::Browser {
            let is_vault_or_sensitive = tool_name.contains("vault")
                || tool_name.contains("memory")
                || tool_name.contains("markdown")
                || tool_name.starts_with("read_markdown")
                || tool_name.contains("get_fact")
                || tool_name.contains("credential")
                || tool_name.contains("keyring")
                || tool_name.contains("pkm")
                || tool_name.contains("profile")
                || tool_name.contains("contact")
                || tool_name.contains("get_tasks");
            if is_vault_or_sensitive {
                return Err(SecurityError::VaultAccessForbiddenInBrowserMode(format!(
                    "ExecutorPolicy: Công cụ '{tool_name}' truy cập kho dữ liệu cá nhân hoặc dữ liệu nhạy cảm, bị cấm trong phiên Browser"
                )));
            }
        }

        // 3. Kiểm tra rủi ro R4 (Cấm tuyệt đối đối với agent)
        if risk_tier == RiskTier::R4 {
            return Err(SecurityError::ForbiddenR4Operation(format!(
                "ExecutorPolicy: Công cụ '{tool_name}' thuộc bậc rủi ro R4 (cấm tuyệt đối đối với agent)"
            )));
        }

        // 4. Kiểm tra dữ liệu ô nhiễm (F20)
        if is_argument_tainted {
            let is_sink_mutation = risk_tier >= RiskTier::Reversible
                || tool_name.starts_with("send_")
                || tool_name.starts_with("write_")
                || tool_name.contains("mutate")
                || tool_name.contains("exec");

            if is_sink_mutation {
                // Phải có receipt hợp lệ và ràng buộc mã băm đối số
                let receipt_valid = match receipt {
                    Some(rc) => {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        if now > rc.expires_at_ms {
                            return Err(SecurityError::InvalidDeclassificationReceipt(format!(
                                "ExecutorPolicy: Declassification receipt expired at {}, current time is {}",
                                rc.expires_at_ms, now
                            )));
                        }
                        let computed_hash =
                            crate::cognitive::policy::compute_canonical_args_hash(arguments_json);
                        if rc.payload_blake3_hash != computed_hash {
                            return Err(SecurityError::InvalidDeclassificationReceipt(format!(
                                "Declassification receipt payload hash mismatch: expected {}, got {}",
                                rc.payload_blake3_hash, computed_hash
                            )));
                        }
                        true
                    }
                    None => false,
                };

                if !receipt_valid {
                    return Err(SecurityError::TaintedDataSinkViolation(format!(
                        "ExecutorPolicy: Dữ liệu ô nhiễm chưa được giải mật qua biên nhận hợp lệ bị cấm đưa vào sink đột biến '{tool_name}'"
                    )));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_executor_policy_enforcement() {
        let empty_args = json!({});

        // 1. R4 operation is unconditionally forbidden
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Actor,
                "cedar_policy/modify",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::ForbiddenR4Operation(_))
        ));

        // 2. Reader mode forbids mutations
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Reader,
                "send_email",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::MutationForbiddenInReaderMode(_))
        ));

        // 3. Browser mode forbids vault and sensitive personal tools
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Browser,
                "search_vault",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Browser,
                "read_markdown",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Browser,
                "get_user_profile",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Browser,
                "contacts:list",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Browser,
                "get_tasks",
                &empty_args,
                false,
                None,
                None
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));

        // 4. Tainted argument without receipt to mutating sink fails
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Actor,
                "write_file",
                &empty_args,
                true,
                None,
                None
            ),
            Err(SecurityError::TaintedDataSinkViolation(_))
        ));

        // 5a. Tainted argument with valid unexpired matching receipt succeeds
        let valid_receipt = DeclassificationReceipt {
            receipt_id: "rc_test".into(),
            authorized_by: "Owner".into(),
            payload_blake3_hash: crate::cognitive::policy::compute_canonical_args_hash(&empty_args),
            issued_at_ms: 1000,
            expires_at_ms: u64::MAX,
        };
        assert!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Actor,
                "write_file",
                &empty_args,
                true,
                None,
                Some(&valid_receipt)
            )
            .is_ok()
        );

        // 5b. Tainted argument with hash mismatch fails
        let mismatched_receipt = DeclassificationReceipt {
            receipt_id: "rc_mismatch".into(),
            authorized_by: "Owner".into(),
            payload_blake3_hash: "wrong_hash".into(),
            issued_at_ms: 1000,
            expires_at_ms: u64::MAX,
        };
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Actor,
                "write_file",
                &empty_args,
                true,
                None,
                Some(&mismatched_receipt)
            ),
            Err(SecurityError::InvalidDeclassificationReceipt(_))
        ));

        // 5c. Tainted argument with expired receipt fails
        let expired_receipt = DeclassificationReceipt {
            receipt_id: "rc_expired".into(),
            authorized_by: "Owner".into(),
            payload_blake3_hash: crate::cognitive::policy::compute_canonical_args_hash(&empty_args),
            issued_at_ms: 1000,
            expires_at_ms: 1001,
        };
        assert!(matches!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Actor,
                "write_file",
                &empty_args,
                true,
                None,
                Some(&expired_receipt)
            ),
            Err(SecurityError::InvalidDeclassificationReceipt(_))
        ));

        // 6. Safe read operation succeeds
        assert!(
            ExecutorPolicy::enforce_sink_call(
                SessionMode::Actor,
                "get_time",
                &empty_args,
                false,
                None,
                None
            )
            .is_ok()
        );
    }
}
