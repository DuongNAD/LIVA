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

    /// M3 — MA TRẬN fail-closed: các lệnh điều hành hệ thống KHÔNG được tới tay
    /// principal không tin cậy. Bất kỳ ai thêm lệnh nhạy cảm vào
    /// `WIDGET_COMMANDS`/`REMOTE_COMMANDS`/`TELEGRAM_COMMANDS`/`SETUP_COMMANDS`
    /// đều làm test này đỏ ngay tại đây, kèm tên lệnh vi phạm.
    #[test]
    fn dieu_hanh_he_thong_khong_cho_principal_yeu() {
        const DIEU_HANH: &[&str] = &[
            "update_config",
            "reset_memory",
            "consent:grant",
            "consent:revoke",
            "delete_memory_fact",
            "memory:sweep_retention",
            "skills:list",
            "mcp:list_tools",
            "vision:set_config",
            "task_plan_chat",
            "echo",
            "get_preflight_status",
        ];
        const YEU: &[CommandPrincipal] = &[
            CommandPrincipal::TauriWidget,
            CommandPrincipal::TauriSetup,
            CommandPrincipal::WebSocketWidget,
            CommandPrincipal::WebSocketRemote,
            CommandPrincipal::Telegram,
        ];
        for principal in YEU {
            for cmd in DIEU_HANH {
                assert!(
                    authorize_command(*principal, cmd).is_err(),
                    "{principal:?} KHÔNG được phép gọi '{cmd}'"
                );
            }
        }
    }

    /// Cửa sổ setup chỉ phục vụ cài đặt artifact: đúng các lệnh `setup:*` —
    /// mọi thứ khác (kể cả `ping`) phải bị từ chối để cửa sổ này không thành
    /// đường vào phụ có đặc quyền dashboard.
    #[test]
    fn setup_chi_co_cac_lenh_setup() {
        for cmd in SETUP_COMMANDS {
            assert!(authorize_command(CommandPrincipal::TauriSetup, cmd).is_ok());
        }
        for cmd in ["ping", "status", "get_config", "chat:completion", "echo"] {
            assert!(
                authorize_command(CommandPrincipal::TauriSetup, cmd).is_err(),
                "setup không được phép gọi '{cmd}'"
            );
        }
    }

    /// LocalCli/Test là kênh chẩn đoán đáng tin cậy — chấp nhận mọi lệnh kể cả
    /// lệnh lạ. Đây là HỢP ĐỒNG; nếu đổi phải sửa threat-model trước.
    #[test]
    fn local_cli_va_test_la_tin_cay_day_du() {
        for cmd in ["lenh-la-ky", "update_config", "reset_memory"] {
            assert!(authorize_command(CommandPrincipal::LocalCli, cmd).is_ok());
            assert!(authorize_command(CommandPrincipal::Test, cmd).is_ok());
        }
    }

    /// Trùng lắp trong một allow-list không mở quyền thêm nhưng là dấu hiệu drift
    /// giữa các danh sách — bắt lúc chạy test thay vì khi audit bảo mật.
    #[test]
    fn khong_trung_lap_trong_moi_danh_sach() {
        for (ten, ds) in [
            ("SETUP", SETUP_COMMANDS),
            ("WIDGET", WIDGET_COMMANDS),
            ("DASHBOARD", DASHBOARD_COMMANDS),
            ("REMOTE", REMOTE_COMMANDS),
            ("TELEGRAM", TELEGRAM_COMMANDS),
        ] {
            let mut da_xem = std::collections::HashSet::new();
            for cmd in ds {
                assert!(
                    da_xem.insert(*cmd),
                    "danh sách {ten} chứa lệnh trùng '{cmd}'"
                );
            }
        }
    }
}
