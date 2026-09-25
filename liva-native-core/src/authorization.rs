//! Principal-aware command authorization for every external command-plane entry.
//!
//! Trusted local diagnostics keep the legacy unrestricted dispatcher. Every
//! WebView/WebSocket/Telegram principal is fail-closed and must appear in an
//! explicit allow-list here before it can reach [`crate::handle_command`].

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum CommandPrincipal {
    LocalCli,
    Test,
    TauriWidget,
    TauriDashboard,
    TauriSetup,
    Telegram,
}

/// Canonical identifiers for CUA desktop automation IPC commands.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandName {
    CuaListWindows,
    CuaExecuteAction,
    CuaEmergencyStop,
    CuaSetMode,
    CuaGetStatus,
    CuaQueryAuditLogs,
}

impl CommandName {
    #[allow(dead_code)]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::CuaListWindows => "cua:list_windows",
            Self::CuaExecuteAction => "cua:execute_action",
            Self::CuaEmergencyStop => "cua:emergency_stop",
            Self::CuaSetMode => "cua:set_mode",
            Self::CuaGetStatus => "cua:get_status",
            Self::CuaQueryAuditLogs => "cua:query_audit_logs",
        }
    }
}

#[allow(dead_code)]
pub const CUA_COMMANDS: &[&str] = &[
    "cua:list_windows",
    "cua:execute_action",
    "cua:emergency_stop",
    "cua:set_mode",
    "cua:get_status",
    "cua:query_audit_logs",
];

const SETUP_COMMANDS: &[&str] = &["setup:status", "setup:paths", "setup:fetch"];

const WIDGET_COMMANDS: &[&str] = &[
    "ping",
    "status",
    "audio_play_started",
    "audio_play_finished",
    "get_config",
    "get_ai_config",
    "get_voice_status",
    "get_voice_profiles",
    "get_system_status",
    "get_memory_status",
    "get_user_profile",
    "get_avatar_models",
    "llm:health_check",
    "chat:completion",
    "vision:capture",
    "vision:ask",
    "voice:stt_start",
    "voice:stt_chunk",
    "voice:stt_stop",
    "voice:tts_speak",
    "voice:tts_stop",
    "message:draft",
    "message:confirm",
    "message:cancel",
    "message:pending",
    "messenger:status",
    "telemetry:summary",
    // CUA Subsystem (Desktop Automation Monitoring & Control)
    "cua:list_windows",
    "cua:execute_action",
    "cua:emergency_stop",
    "cua:set_mode",
    "cua:get_status",
];

const DASHBOARD_COMMANDS: &[&str] = &[
    "ping",
    "echo",
    "status",
    "get_config",
    "update_config",
    "get_ai_config",
    "get_voice_status",
    "get_voice_profiles",
    "select_voice_profile",
    "get_system_status",
    "get_memory_status",
    "get_preflight_status",
    "get_skills_list",
    "toggle_skill",
    "toggle_all_skills",
    "get_user_profile",
    "update_user_profile",
    "get_avatar_models",
    "import_avatar_folder",
    "delete_avatar_model",
    "consent:get",
    "consent:grant",
    "consent:revoke",
    "integrations:list",
    "llm:embed",
    "llm:health_check",
    "chat:completion",
    "task_plan_chat",
    "get_memory_data",
    "memory:set_fact",
    "memory:get_fact",
    "delete_memory_fact",
    "memory:delete_conversation",
    "memory:delete_subject",
    "memory:sweep_retention",
    "consolidate_memory",
    "reset_memory",
    "memory:search_hybrid",
    "memory:upsert_vector",
    "contacts:list",
    "contacts:upsert",
    "contacts:delete",
    "message:draft",
    "message:confirm",
    "message:cancel",
    "message:pending",
    "messenger:status",
    "get_tasks",
    "add_task",
    "delete_task",
    "update_task",
    "vision:capture",
    "vision:add_region",
    "vision:remove_region",
    "vision:get_changed_regions",
    "vision:set_config",
    "vision:ask",
    "voice:stt_start",
    "voice:stt_chunk",
    "voice:stt_stop",
    "voice:set_language",
    "voice:list_vieneu_voices",
    "voice:set_vieneu_voice",
    "voice:tts_speak",
    "voice:tts_stop",
    "mcp:list_tools",
    "mcp_client:list_servers",
    "mcp_client:list_tools",
    "skills:list",
    "skills:search",
    "skills:signals",
    "skills:history",
    "telemetry:summary",
    // CUA Subsystem (Full Management, Inspector & Audit Ledger)
    "cua:list_windows",
    "cua:execute_action",
    "cua:emergency_stop",
    "cua:set_mode",
    "cua:get_status",
    "cua:query_audit_logs",
];

const TELEGRAM_COMMANDS: &[&str] = &["ping", "status", "chat:completion"];

pub fn is_known_command(command: &str) -> bool {
    SETUP_COMMANDS.contains(&command)
        || WIDGET_COMMANDS.contains(&command)
        || DASHBOARD_COMMANDS.contains(&command)
        || TELEGRAM_COMMANDS.contains(&command)
}

pub fn authorize_command(principal: CommandPrincipal, command: &str) -> Result<(), String> {
    let allowed = match principal {
        CommandPrincipal::LocalCli | CommandPrincipal::Test => true,
        CommandPrincipal::TauriSetup => SETUP_COMMANDS.contains(&command),
        CommandPrincipal::TauriWidget => WIDGET_COMMANDS.contains(&command),
        CommandPrincipal::TauriDashboard => DASHBOARD_COMMANDS.contains(&command),
        CommandPrincipal::Telegram => TELEGRAM_COMMANDS.contains(&command),
    };

    if allowed {
        Ok(())
    } else {
        Err(format!(
            "principal {principal:?} is not authorized for command '{command}'"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preflight_chi_mo_cho_dashboard_cuc_bo() {
        assert!(
            authorize_command(CommandPrincipal::TauriDashboard, "get_preflight_status").is_ok()
        );
        assert!(authorize_command(CommandPrincipal::TauriWidget, "get_preflight_status").is_err());
    }

    #[test]
    fn import_avatar_folder_chi_mo_cho_dashboard() {
        assert!(
            authorize_command(CommandPrincipal::TauriDashboard, "import_avatar_folder").is_ok()
        );
        assert!(authorize_command(CommandPrincipal::TauriWidget, "import_avatar_folder").is_err());
        assert!(authorize_command(CommandPrincipal::Telegram, "import_avatar_folder").is_err());
    }

    #[test]
    fn toggle_skills_chi_mo_cho_dashboard() {
        for cmd in ["toggle_skill", "toggle_all_skills"] {
            assert!(authorize_command(CommandPrincipal::TauriDashboard, cmd).is_ok());
            assert!(authorize_command(CommandPrincipal::TauriWidget, cmd).is_err());
            assert!(authorize_command(CommandPrincipal::Telegram, cmd).is_err());
        }
    }

    #[test]
    fn disk_modifying_config_commands_chi_mo_cho_dashboard() {
        for cmd in [
            "update_user_profile",
            "select_voice_profile",
            "delete_avatar_model",
        ] {
            assert!(authorize_command(CommandPrincipal::TauriDashboard, cmd).is_ok());
            assert!(authorize_command(CommandPrincipal::TauriWidget, cmd).is_err());
            assert!(authorize_command(CommandPrincipal::Telegram, cmd).is_err());
        }
    }
}
