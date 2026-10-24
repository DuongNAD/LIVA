use super::proposal::ActionProposal;
use serde::{Deserialize, Serialize};
use std::fmt;
use unicode_normalization::UnicodeNormalization;

/// 5-Tier Static Risk Classification Hierarchy (Feature F23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    /// R0: Auto-approved read-only operations and local drafting (zero mutation, zero external leakage).
    #[serde(rename = "r0", alias = "read_only")]
    R0 = 0,
    /// R1: Auto-approved undoable workspace mutations (reversible with atomic backup/rollback).
    #[serde(rename = "r1", alias = "reversible")]
    R1 = 1,
    /// R2: Narrow standing grants (pre-authorized bounded actions e.g. periodic weather check, calendar lookup).
    #[serde(rename = "r2", alias = "standing_grant")]
    R2 = 2,
    /// R3: Mandatory HITL approval with normalized argument hash verification.
    #[serde(rename = "r3", alias = "external_side_effect", alias = "physical_or_irreversible")]
    R3 = 3,
    /// R4: Absolute forbidden operations (host destruction, policy bypass, secret export, audit tampering).
    #[serde(rename = "r4", alias = "forbidden")]
    R4 = 4,
}

impl RiskTier {
    // Backward-compatible aliases for legacy callers
    #[allow(non_upper_case_globals)]
    pub const ReadOnly: Self = Self::R0;
    #[allow(non_upper_case_globals)]
    pub const Reversible: Self = Self::R1;
    #[allow(non_upper_case_globals)]
    pub const StandingGrant: Self = Self::R2;
    #[allow(non_upper_case_globals)]
    pub const ExternalSideEffect: Self = Self::R3;
    #[allow(non_upper_case_globals)]
    pub const PhysicalOrIrreversible: Self = Self::R3;
    #[allow(non_upper_case_globals)]
    pub const Forbidden: Self = Self::R4;

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::R0 => "read_only",
            Self::R1 => "reversible",
            Self::R2 => "standing_grant",
            Self::R3 => "external_side_effect",
            Self::R4 => "forbidden",
        }
    }

    pub fn tier_code(&self) -> &'static str {
        match self {
            Self::R0 => "r0",
            Self::R1 => "r1",
            Self::R2 => "r2",
            Self::R3 => "r3",
            Self::R4 => "r4",
        }
    }

    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "r0" | "read_only" | "readonly" | "read" | "0" => Some(Self::R0),
            "r1" | "reversible" | "rev" | "1" => Some(Self::R1),
            "r2" | "standing_grant" | "standing" | "grant" | "batch" | "2" => Some(Self::R2),
            "r3" | "external_side_effect" | "external" | "side_effect"
            | "physical_or_irreversible" | "irreversible" | "physical" | "critical" | "3" => {
                Some(Self::R3)
            }
            "r4" | "forbidden" | "denied" | "4" => Some(Self::R4),
            _ => None,
        }
    }

    /// Whether Human-in-the-Loop (HITL) continuation/confirmation is mandatory.
    pub fn is_hitl_mandatory(&self) -> bool {
        matches!(self, Self::R3)
    }

    /// Whether this action is unconditionally forbidden.
    pub fn is_forbidden(&self) -> bool {
        matches!(self, Self::R4)
    }
}

impl fmt::Display for RiskTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Canonicalizes JSON values for deterministic hashing.
pub fn canonicalize_json(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by_key(|(k, _)| *k);
            let parts: Vec<String> = entries
                .into_iter()
                .map(|(k, v)| format!("\"{}\":{}", k, canonicalize_json(v)))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonicalize_json).collect();
            format!("[{}]", parts.join(","))
        }
        serde_json::Value::String(s) => {
            let nfc: String = s.trim().nfc().collect();
            serde_json::to_string(&nfc).unwrap_or_else(|_| format!("\"{}\"", s))
        }
        other => other.to_string(),
    }
}

/// Computes BLAKE3 canonical argument hash.
pub fn compute_canonical_args_hash(args: &serde_json::Value) -> String {
    let canonical = canonicalize_json(args);
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

/// Visualizes and highlights invisible/zero-width Unicode characters for human approval.
pub fn visualize_invisible_chars(input: &str) -> (String, bool) {
    let mut has_invisible = false;
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '\u{200B}' => {
                has_invisible = true;
                out.push_str("[ZWSP]");
            }
            '\u{200C}' => {
                has_invisible = true;
                out.push_str("[ZWNJ]");
            }
            '\u{200D}' => {
                has_invisible = true;
                out.push_str("[ZWJ]");
            }
            '\u{200E}' => {
                has_invisible = true;
                out.push_str("[LRM]");
            }
            '\u{200F}' => {
                has_invisible = true;
                out.push_str("[RLM]");
            }
            '\u{202A}' => {
                has_invisible = true;
                out.push_str("[LRE]");
            }
            '\u{202B}' => {
                has_invisible = true;
                out.push_str("[RLE]");
            }
            '\u{202C}' => {
                has_invisible = true;
                out.push_str("[PDF]");
            }
            '\u{202D}' => {
                has_invisible = true;
                out.push_str("[LRO]");
            }
            '\u{202E}' => {
                has_invisible = true;
                out.push_str("[RLO]");
            }
            '\u{FEFF}' => {
                has_invisible = true;
                out.push_str("[BOM]");
            }
            '\u{00AD}' => {
                has_invisible = true;
                out.push_str("[SHY]");
            }
            other => out.push(other),
        }
    }
    (out, has_invisible)
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
    /// Creates an immediate Allow decision (for R0, R1, or R2 low-risk actions).
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

        // 1. Tier R4: Forbidden Operations (host destruction, security policy tampering, secret export, audit tampering)
        if matches_prefix("cedar_policy")
            || matches_prefix("export_key")
            || matches_prefix("dpapi")
            || matches_prefix("keyring:export")
            || matches_prefix("disable_audit")
            || matches_prefix("stop_audit")
            || matches_prefix("delete_audit")
            || matches_prefix("wipe_audit")
            || matches_prefix("clear_audit")
            || matches_prefix("escape_sandbox")
            || matches_prefix("disable_sandbox")
            || matches_prefix("grant_permanent")
            || matches_prefix("modify_persona")
            || matches_prefix("system_persona")
            || matches_prefix("export_secrets")
            || matches_prefix("export_passwords")
            || matches_prefix("disable_hmac")
            || matches_prefix("force_load_binary")
            || matches_prefix("unsigned_binary")
            || contains_sub("cedar_policy")
            || contains_sub("dpapi")
            || contains_sub("wasm_escape")
            || contains_sub("disable_audit")
            || contains_sub("keyring:export")
        {
            return RiskTier::R4;
        }

        // 2. Known Safe Read-Only tools (explicit whitelist) -> R0
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
            return RiskTier::R0;
        }

        // 3. Known Safe Reversible local state adjustments (explicit whitelist) -> R1
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
            return RiskTier::R1;
        }

        // 4. Standing Grants (pre-authorized bounded actions) -> R2
        if matches_prefix("standing_grant")
            || matches_prefix("periodic_sync")
            || matches_prefix("batch_read")
        {
            return RiskTier::R2;
        }

        // 5. Tier R3: External Side Effects & Mutating actions
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
            || matches_prefix("delete_subject")
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
            return RiskTier::R3;
        }

        // Fail-Secure Default: Any unlisted / unknown tool defaults to R3 (HITL mandatory)
        RiskTier::R3
    }

    /// Evaluates an ActionProposal against governance policies.
    pub fn evaluate_proposal(proposal: &ActionProposal) -> PolicyDecision {
        // Validation check
        if let Err(err) = proposal.validate() {
            return PolicyDecision::deny(
                RiskTier::R0,
                format!("Invalid action proposal: {err}"),
            );
        }

        // Derive intrinsic risk tier
        let intrinsic_tier = Self::classify_tool(&proposal.tool_id);
        // Effective risk tier is the maximum of intrinsic and proposal declared risk
        let effective_tier = std::cmp::max(intrinsic_tier, proposal.risk_tier);

        match effective_tier {
            RiskTier::R0 => PolicyDecision::allow(
                RiskTier::R0,
                "Read-only operation permitted automatically",
            ),
            RiskTier::R1 => PolicyDecision::allow(
                RiskTier::R1,
                "Reversible local action permitted with automatic audit logging",
            ),
            RiskTier::R2 => PolicyDecision::allow(
                RiskTier::R2,
                "Narrow standing grant operation permitted within quota",
            ),
            RiskTier::R3 => {
                let token = uuid::Uuid::new_v4().to_string();
                PolicyDecision::confirm(
                    RiskTier::R3,
                    format!(
                        "External side-effect on tool '{}' requires human confirmation",
                        proposal.tool_id
                    ),
                    token,
                )
            }
            RiskTier::R4 => PolicyDecision::deny(
                RiskTier::R4,
                format!(
                    "Operation on tool '{}' is unconditionally forbidden by policy (R4)",
                    proposal.tool_id
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_5_tier_risk_classification_and_aliases() {
        assert_eq!(RiskTier::ReadOnly, RiskTier::R0);
        assert_eq!(RiskTier::Reversible, RiskTier::R1);
        assert_eq!(RiskTier::StandingGrant, RiskTier::R2);
        assert_eq!(RiskTier::ExternalSideEffect, RiskTier::R3);
        assert_eq!(RiskTier::PhysicalOrIrreversible, RiskTier::R3);
        assert_eq!(RiskTier::Forbidden, RiskTier::R4);

        assert_eq!(RiskTier::R0.tier_code(), "r0");
        assert_eq!(RiskTier::R4.tier_code(), "r4");
        assert_eq!(RiskTier::R0.as_str(), "read_only");
        assert_eq!(RiskTier::R1.as_str(), "reversible");
        assert_eq!(RiskTier::R4.as_str(), "forbidden");

        assert_eq!(RiskTier::from_str_loose("r0"), Some(RiskTier::R0));
        assert_eq!(RiskTier::from_str_loose("read_only"), Some(RiskTier::R0));
        assert_eq!(RiskTier::from_str_loose("r4"), Some(RiskTier::R4));
        assert_eq!(RiskTier::from_str_loose("forbidden"), Some(RiskTier::R4));
    }

    #[test]
    fn test_r4_forbidden_tool_classification() {
        assert_eq!(PolicyEngine::classify_tool("cedar_policy/modify"), RiskTier::R4);
        assert_eq!(PolicyEngine::classify_tool("export_key/dpapi"), RiskTier::R4);
        assert_eq!(PolicyEngine::classify_tool("disable_audit"), RiskTier::R4);
        assert_eq!(PolicyEngine::classify_tool("escape_sandbox"), RiskTier::R4);
    }

    #[test]
    fn test_canonicalize_json_and_blake3_hash() {
        let json_a = serde_json::json!({ "z": 1, "a": "hello", "m": [3, 2, 1] });
        let json_b = serde_json::json!({ "a": "hello", "m": [3, 2, 1], "z": 1 });

        let canon_a = canonicalize_json(&json_a);
        let canon_b = canonicalize_json(&json_b);
        assert_eq!(canon_a, canon_b);

        let hash_a = compute_canonical_args_hash(&json_a);
        let hash_b = compute_canonical_args_hash(&json_b);
        assert_eq!(hash_a, hash_b);
    }

    #[test]
    fn test_visualize_invisible_chars() {
        let input_clean = "normal text";
        let (vis_clean, has_inv_clean) = visualize_invisible_chars(input_clean);
        assert_eq!(vis_clean, "normal text");
        assert!(!has_inv_clean);

        let input_invis = "secret\u{200B}text\u{202E}attack\u{FEFF}";
        let (vis_invis, has_inv) = visualize_invisible_chars(input_invis);
        assert!(has_inv);
        assert_eq!(vis_invis, "secret[ZWSP]text[RLO]attack[BOM]");
    }
}
