//! Rule-of-Two Session Mode Governance (Feature F19).

use crate::cognitive::policy::RiskTier;
pub use liva_core_types::{RuleOfTwoVertices, SecurityError, SessionMode};

pub struct SessionModeGovernor;

impl SessionModeGovernor {
    /// Phân giải các đỉnh kích hoạt từ chế độ phiên.
    pub fn get_vertices(mode: SessionMode) -> RuleOfTwoVertices {
        match mode {
            SessionMode::Reader => RuleOfTwoVertices {
                untrusted_input: true,
                sensitive_data: true,
                external_mutation: false,
            },
            SessionMode::Actor => RuleOfTwoVertices {
                untrusted_input: false,
                sensitive_data: true,
                external_mutation: true,
            },
            SessionMode::Browser => RuleOfTwoVertices {
                untrusted_input: true,
                sensitive_data: false,
                external_mutation: true,
            },
        }
    }

    /// Kiểm tra tính hợp lệ của việc gọi công cụ theo chế độ phiên hiện tại.
    pub fn check_tool_call(
        mode: SessionMode,
        tool_name: &str,
        risk_tier: RiskTier,
        accesses_vault: bool,
    ) -> Result<(), SecurityError> {
        match mode {
            SessionMode::Reader => {
                // Reader cấm mọi công cụ có tính đột biến (Reversible, ExternalSideEffect, PhysicalOrIrreversible, Forbidden)
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
                        "Lệnh hoặc công cụ '{tool_name}' (Bậc rủi ro: {risk_tier:?}) yêu cầu đột biến ngoài, bị cấm tuyệt đối trong phiên Reader"
                    )));
                }
            }
            SessionMode::Browser => {
                // Browser cấm mọi truy cập vào kho cá nhân hoặc bộ nhớ nhạy cảm
                let is_vault_or_sensitive = accesses_vault
                    || tool_name.contains("vault")
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
                        "Công cụ '{tool_name}' truy cập dữ liệu nhạy cảm/kho cá nhân, bị cấm tuyệt đối trong phiên Browser"
                    )));
                }
            }
            SessionMode::Actor => {
                // Actor cho phép đột biến và truy cập vault (nếu là Owner)
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_mode_governor_rule_of_two_and_enforcement() {
        // Invariant: No session mode ever activates all three vertices
        for mode in [SessionMode::Reader, SessionMode::Actor, SessionMode::Browser] {
            let v = SessionModeGovernor::get_vertices(mode);
            assert!(v.is_valid());
            assert!(v.validate().is_ok());
        }

        // Reader Mode: blocks mutations
        assert!(matches!(
            SessionModeGovernor::check_tool_call(
                SessionMode::Reader,
                "send_telegram_msg",
                RiskTier::R3,
                false
            ),
            Err(SecurityError::MutationForbiddenInReaderMode(_))
        ));
        assert!(SessionModeGovernor::check_tool_call(
            SessionMode::Reader,
            "read_file",
            RiskTier::R0,
            false
        )
        .is_ok());

        // Browser Mode: blocks vault / memory / keyring / sensitive personal tools
        assert!(matches!(
            SessionModeGovernor::check_tool_call(
                SessionMode::Browser,
                "search_vault",
                RiskTier::R0,
                true
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            SessionModeGovernor::check_tool_call(
                SessionMode::Browser,
                "read_markdown",
                RiskTier::R0,
                false
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            SessionModeGovernor::check_tool_call(
                SessionMode::Browser,
                "get_user_profile",
                RiskTier::R0,
                false
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            SessionModeGovernor::check_tool_call(
                SessionMode::Browser,
                "contacts:list",
                RiskTier::R0,
                false
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(matches!(
            SessionModeGovernor::check_tool_call(
                SessionMode::Browser,
                "get_tasks",
                RiskTier::R0,
                false
            ),
            Err(SecurityError::VaultAccessForbiddenInBrowserMode(_))
        ));
        assert!(SessionModeGovernor::check_tool_call(
            SessionMode::Browser,
            "web_fetch",
            RiskTier::R1,
            false
        )
        .is_ok());

        // Actor Mode: permits authorized calls
        assert!(SessionModeGovernor::check_tool_call(
            SessionMode::Actor,
            "send_message",
            RiskTier::R3,
            false
        )
        .is_ok());
    }
}
