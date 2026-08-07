//! Principal-aware command authorization for every external command-plane entry.
//!
//! Trusted local diagnostics keep the legacy unrestricted dispatcher. Every
//! WebView/WebSocket/Telegram principal is fail-closed and must appear in an
//! explicit allow-list here before it can reach [`crate::handle_command`].

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandPrincipal {
    LocalCli,
    Test,
    TauriWidget,
    TauriDashboard,
    TauriSetup,
    WebSocketWidget,
    WebSocketDashboard,
    WebSocketRemote,
    Telegram,
}

const SETUP_COMMANDS: &[&str] = &["setup:status", "setup:paths", "setup:fetch"];

const WIDGET_COMMANDS: &[&str] = &[
    "ping",
    "status",
    "get_config",
    "get_ai_config",
    "get_voice_status",
    "get_voice_profiles",
    "get_system_status",
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
    "get_system_status",
    "get_preflight_status",
    "get_skills_list",
    "get_user_profile",
    "get_avatar_models",
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
];

const REMOTE_COMMANDS: &[&str] = &[
    "ping",
    "status",
    "llm:health_check",
    "chat:completion",
    "voice:stt_start",
    "voice:stt_chunk",
    "voice:stt_stop",
    "voice:tts_speak",
    "voice:tts_stop",
];

const TELEGRAM_COMMANDS: &[&str] = &["ping", "status", "chat:completion"];

pub fn authorize_command(principal: CommandPrincipal, command: &str) -> Result<(), String> {
    let allowed = match principal {
        CommandPrincipal::LocalCli | CommandPrincipal::Test => true,
        CommandPrincipal::TauriSetup => SETUP_COMMANDS.contains(&command),
        CommandPrincipal::TauriWidget | CommandPrincipal::WebSocketWidget => {
            WIDGET_COMMANDS.contains(&command)
        }
        CommandPrincipal::TauriDashboard | CommandPrincipal::WebSocketDashboard => {
            DASHBOARD_COMMANDS.contains(&command)
        }
        CommandPrincipal::WebSocketRemote => REMOTE_COMMANDS.contains(&command),
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
        assert!(
            authorize_command(CommandPrincipal::WebSocketDashboard, "get_preflight_status").is_ok()
        );
        assert!(authorize_command(CommandPrincipal::TauriWidget, "get_preflight_status").is_err());
        assert!(
            authorize_command(CommandPrincipal::WebSocketRemote, "get_preflight_status").is_err()
        );
    }
}
