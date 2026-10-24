//! Dynamic Tool Pruning based on turn principal authority tier (Feature F21).

use crate::authorization::principal::{AuthorityTier, Principal};
use crate::llm::tool_calling::CatalogTool;

pub struct DynamicToolPruner;

impl DynamicToolPruner {
    pub fn classify_tool_requirement(tool_name: &str) -> AuthorityTier {
        let name = tool_name.to_lowercase();

        // Owner-Only: System, shell, disk mutation, config, wallet, secrets
        if name.contains("bash")
            || name.contains("shell")
            || name.contains("exec")
            || name.contains("powershell")
            || name.contains("cmd")
            || name.contains("cua:")
            || name.contains("os_control")
            || name.contains("terminal")
            || name.contains("file_write")
            || name.contains("write_markdown")
            || name.contains("delete_file")
            || name.contains("import_avatar")
            || name.contains("delete_avatar")
            || name.contains("keyring")
            || name.contains("secret")
            || name.contains("wallet")
            || name.contains("payment")
            || name.contains("transfer")
            || name.contains("update_config")
            || name.contains("toggle_skill")
            || name.contains("memory:delete")
            || name.contains("memory:sweep")
            || name.contains("reset_memory")
            || name.contains("system:shutdown")
            || name.contains("system:reboot")
            || name.contains("kill_process")
        {
            return AuthorityTier::Owner;
        }

        // Known Contact: Controlled memos, schedule availability
        if name.contains("leave_message")
            || name.contains("check_availability")
            || name.contains("get_calendar_free_busy")
            || name.contains("read_public_bio")
        {
            return AuthorityTier::KnownContact;
        }

        // Untrusted / Public: Read-only safe queries
        if name.contains("get_weather")
            || name.contains("get_time")
            || name.contains("search_public")
            || name.contains("ping")
        {
            return AuthorityTier::Untrusted;
        }

        // Fail-closed default
        AuthorityTier::Owner
    }

    pub fn prune_tools(tools: &[CatalogTool], principal: &Principal) -> Vec<CatalogTool> {
        let user_tier = principal.authority_tier();
        tools
            .iter()
            .filter(|t| Self::classify_tool_requirement(&t.name) <= user_tier)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_tool(name: &str) -> CatalogTool {
        CatalogTool {
            server: "native".into(),
            name: name.into(),
            description: format!("Tool {name}"),
            input_schema: json!({}),
            embed_extra: String::new(),
        }
    }

    #[test]
    fn test_dynamic_tool_pruning_by_authority_tier() {
        let tools = vec![
            make_tool("exec_powershell"),
            make_tool("delete_file"),
            make_tool("leave_message"),
            make_tool("get_weather"),
        ];

        // 1. Owner sees all tools
        let owner_tools = DynamicToolPruner::prune_tools(&tools, &Principal::Owner);
        assert_eq!(owner_tools.len(), 4);

        // 2. KnownContact cannot see Owner tools (exec_powershell, delete_file stripped)
        let contact = Principal::KnownContact {
            contact_id: "c1".into(),
            display_name: "Mom".into(),
            channel: "telegram".into(),
            identifier: "@mom".into(),
            trust_level: liva_core_types::ContactTrustLevel::Family,
        };
        let contact_tools = DynamicToolPruner::prune_tools(&tools, &contact);
        let names: Vec<_> = contact_tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["leave_message", "get_weather"]);

        // 3. Untrusted cannot see KnownContact or Owner tools (only get_weather remains)
        let untrusted = Principal::UntrustedExternal {
            channel_id: "telegram".into(),
            sender_address: "stranger_123".into(),
            is_public_channel: false,
        };
        let untrusted_tools = DynamicToolPruner::prune_tools(&tools, &untrusted);
        let untrusted_names: Vec<_> = untrusted_tools.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(untrusted_names, vec!["get_weather"]);
    }
}
