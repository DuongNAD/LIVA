use super::proposal::ActionProposal;
use serde::{Deserialize, Serialize};
use std::fmt;

/// 4-Tier Static Risk Classification Hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// Pure state-reading actions with zero mutation or side-effects.
    ReadOnly = 1,
    /// Local, low-risk state mutations that are fully compensable or reversible.
    Reversible = 2,
    /// External communications, file writes, network calls, or third-party tool executions.
    ExternalSideEffect = 3,
    /// Destructive actions, privacy right-to-erasure, OS process termination, or physical actuators.
    PhysicalOrIrreversible = 4,
}

impl RiskTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Reversible => "reversible",
            Self::ExternalSideEffect => "external_side_effect",
            Self::PhysicalOrIrreversible => "physical_or_irreversible",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "read_only" | "readonly" | "read" | "1" => Some(Self::ReadOnly),
            "reversible" | "rev" | "2" => Some(Self::Reversible),
            "external_side_effect" | "external" | "side_effect" | "3" => {
                Some(Self::ExternalSideEffect)
            }
            "physical_or_irreversible" | "irreversible" | "physical" | "critical" | "4" => {
                Some(Self::PhysicalOrIrreversible)
            }
            _ => None,
        }
    }

    /// Whether Human-in-the-Loop (HITL) continuation/confirmation is mandatory by default.
    pub fn is_hitl_mandatory(&self) -> bool {
        matches!(
            self,
            Self::ExternalSideEffect | Self::PhysicalOrIrreversible
        )
    }
}

impl fmt::Display for RiskTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Evaluation decision produced by the PolicyEngine for an ActionProposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PolicyDecision {
    /// Whether the action is permitted to execute (either automatically or after HITL).
    pub allowed: bool,
    /// Whether execution requires explicit human confirmation before proceeding.
    pub requires_hitl: bool,
    /// The evaluated risk tier of the action.
    pub risk_tier: RiskTier,
    /// Rationale explaining the governance decision.
    pub reason: String,
    /// Single-use confirmation token required for HITL execution.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_token: Option<String>,
}

impl PolicyDecision {
    /// Creates an immediate Allow decision (for ReadOnly or Reversible low-risk actions).
    pub fn allow(risk_tier: RiskTier, reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            requires_hitl: false,
            risk_tier,
            reason: reason.into(),
            confirmation_token: None,
        }
    }

    /// Creates a Confirm decision requiring HITL token verification.
    pub fn confirm(
        risk_tier: RiskTier,
        reason: impl Into<String>,
        confirmation_token: impl Into<String>,
    ) -> Self {
        Self {
            allowed: true,
            requires_hitl: true,
            risk_tier,
            reason: reason.into(),
            confirmation_token: Some(confirmation_token.into()),
        }
    }

    /// Creates an explicit Deny decision.
    pub fn deny(risk_tier: RiskTier, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            requires_hitl: false,
            risk_tier,
            reason: reason.into(),
            confirmation_token: None,
        }
    }
}

/// Static Policy and Risk Evaluator Engine.
pub struct PolicyEngine;

impl PolicyEngine {
    /// Classifies a tool or command identifier into its intrinsic RiskTier.
    pub fn classify_tool(tool_id: &str) -> RiskTier {
        let full = tool_id.trim().to_lowercase();
        let name = full.split('/').next_back().unwrap_or(&full);

        let matches_prefix = |prefix: &str| full.starts_with(prefix) || name.starts_with(prefix);
        let contains_sub = |sub: &str| full.contains(sub) || name.contains(sub);

        // 1. Tier 4: Physical or Irreversible / Privacy Erasure / OS Destructive
        if matches_prefix("delete_subject")
            || matches_prefix("delete_conversation")
            || matches_prefix("delete_database")
            || matches_prefix("drop_database")
            || matches_prefix("factory_reset")
            || matches_prefix("system_shutdown")
            || matches_prefix("system:shutdown")
            || matches_prefix("system:reboot")
            || matches_prefix("system_reboot")
            || matches_prefix("format_disk")
            || matches_prefix("wipe_all_data")
            || matches_prefix("door_unlock")
            || matches_prefix("kill_process")
            || matches_prefix("rm_rf")
            || matches_prefix("erase_disk")
            || matches_prefix("wipe_storage")
            || matches_prefix("kill_task")
            || matches_prefix("pkill")
            || matches_prefix("shutdown")
            || matches_prefix("reboot")
            || contains_sub("drop")
            || contains_sub("erase")
            || contains_sub("wipe")
            || contains_sub("format")
            || contains_sub("pkill")
            || contains_sub("rm_rf")
        {
            return RiskTier::PhysicalOrIrreversible;
        }

        // 2. Known Safe Read-Only tools (explicit whitelist)
        if matches_prefix("read_markdown")
            || matches_prefix("search_vault")
            || matches_prefix("get_weather")
            || matches_prefix("get_memory_data")
            || matches_prefix("get_config")
            || matches_prefix("ping")
            || matches_prefix("system_status")
            || matches_prefix("list_tasks")
            || matches_prefix("mcp:list_tools")
            || matches_prefix("mcp_client:list_servers")
            || matches_prefix("mcp_client:list_tools")
            || matches_prefix("vision:status")
            || matches_prefix("voice:status")
            || matches_prefix("consent:get_status")
            || matches_prefix("memory:get")
            || matches_prefix("memory:search")
            || matches_prefix("memory:conflict_list")
        {
            return RiskTier::ReadOnly;
        }

        // 3. Known Safe Reversible local state adjustments (explicit whitelist)
        if matches_prefix("control_volume")
            || matches_prefix("control_media")
            || matches_prefix("toggle_light")
            || matches_prefix("adjust_room_lighting")
            || matches_prefix("set_volume")
            || matches_prefix("set_brightness")
            || matches_prefix("play_pause")
            || matches_prefix("next_track")
            || matches_prefix("previous_track")
            || matches_prefix("stop_media")
            || matches_prefix("control_smarthome")
        {
            return RiskTier::Reversible;
        }

        // 4. Tier 3: External Side Effects (explicit known or mutating verbs)
        if matches_prefix("message:send")
            || matches_prefix("message:confirm")
            || matches_prefix("telegram:send")
            || matches_prefix("send_message")
            || matches_prefix("send_telegram")
            || matches_prefix("write_markdown")
            || matches_prefix("update_task")
            || matches_prefix("create_task")
            || matches_prefix("delete_task")
            || matches_prefix("webhook_dispatch")
            || matches_prefix("http_post")
            || matches_prefix("http_put")
            || matches_prefix("http_patch")
            || matches_prefix("http_delete")
            || matches_prefix("shell_exec")
            || matches_prefix("run_command")
            || matches_prefix("run_script")
            || matches_prefix("powershell")
            || matches_prefix("bash")
            || matches_prefix("cmd")
            || matches_prefix("curl")
            || matches_prefix("modify_record")
            || matches_prefix("alter_table")
            || matches_prefix("purge_logs")
            || matches_prefix("mcp_client:call_tool")
            || matches_prefix("mcp:call_tool")
            || contains_sub("delete")
            || contains_sub("remove")
            || contains_sub("kill")
            || contains_sub("terminate")
            || contains_sub("run")
            || contains_sub("start")
            || contains_sub("stop")
            || contains_sub("restart")
            || contains_sub("modify")
            || contains_sub("alter")
            || contains_sub("patch")
            || contains_sub("post")
            || contains_sub("put")
            || contains_sub("http")
            || contains_sub("curl")
            || contains_sub("powershell")
            || contains_sub("bash")
            || contains_sub("cmd")
            || contains_sub("write")
            || contains_sub("send")
            || contains_sub("create")
            || contains_sub("update")
            || contains_sub("exec")
        {
            return RiskTier::ExternalSideEffect;
        }

        // 5. Fail-Secure Default: Any unlisted / unknown tool defaults to ExternalSideEffect (HITL mandatory)
        RiskTier::ExternalSideEffect
    }

    /// Evaluates an ActionProposal against governance policies.
    pub fn evaluate_proposal(proposal: &ActionProposal) -> PolicyDecision {
        // Validation check
        if let Err(err) = proposal.validate() {
            return PolicyDecision::deny(
                RiskTier::ReadOnly,
                format!("Invalid action proposal: {err}"),
            );
        }

        // Derive intrinsic risk tier
        let intrinsic_tier = Self::classify_tool(&proposal.tool_id);
        // Effective risk tier is the maximum of intrinsic and proposal declared risk
        let effective_tier = std::cmp::max(intrinsic_tier, proposal.risk_tier);

        match effective_tier {
            RiskTier::ReadOnly => PolicyDecision::allow(
                RiskTier::ReadOnly,
                "Read-only operation permitted automatically",
            ),
            RiskTier::Reversible => PolicyDecision::allow(
                RiskTier::Reversible,
                "Reversible local action permitted with automatic audit logging",
            ),
            RiskTier::ExternalSideEffect => {
                let token = uuid::Uuid::new_v4().to_string();
                PolicyDecision::confirm(
                    RiskTier::ExternalSideEffect,
                    format!(
                        "External side-effect on tool '{}' requires human confirmation",
                        proposal.tool_id
                    ),
                    token,
                )
            }
            RiskTier::PhysicalOrIrreversible => {
                let token = uuid::Uuid::new_v4().to_string();
                PolicyDecision::confirm(
                    RiskTier::PhysicalOrIrreversible,
                    format!(
                        "High-impact irreversible action on '{}' requires explicit confirmation",
                        proposal.tool_id
                    ),
                    token,
                )
            }
        }
    }
}
