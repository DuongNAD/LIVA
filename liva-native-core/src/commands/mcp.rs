//! Miền lệnh MCP (Model Context Protocol).
//!
//! Bao gồm:
//! - LIVA làm MCP Server cục bộ (`mcp:list_tools`, `mcp:call_tool`)
//! - LIVA làm MCP Client kết nối tool ngoài (`mcp_client:list_servers`, `mcp_client:list_tools`, `mcp_client:call_tool`)

use crate::{AppState, llm, mcp};
use std::sync::Arc;

/// Kiểm tra xem lệnh có thuộc miền MCP hay không.
pub fn owns(command: &str) -> bool {
    matches!(
        command,
        "mcp:list_tools"
            | "mcp:call_tool"
            | "mcp_client:list_servers"
            | "mcp_client:list_tools"
            | "mcp_client:call_tool"
    )
}

/// Xử lý các lệnh thuộc miền MCP.
pub async fn handle(
    state: Arc<AppState>,
    command: &str,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match command {
        // ── MCP Server cục bộ ──────────────────────────────────────────────
        // `NativeMcpServer` cung cấp các tool thao tác Obsidian Vault.
        // Ranh giới an toàn: mọi thao tác file đi qua `resolve_path`, chặn
        // đường dẫn tuyệt đối và `..`, và ghim mọi thứ dưới `LIVA_VAULT_PATH`.
        "mcp:list_tools" => Ok(serde_json::to_value(state.mcp_server.list_tools())
            .map_err(|e| format!("Failed to serialize tool list: {}", e))?),

        "mcp:call_tool" => {
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'name' (ten tool). Dung mcp:list_tools de xem danh sach.")?
                .to_string();
            // Không có `arguments` thì coi như object rỗng — tool nào cần tham
            // số sẽ tự báo lỗi deserialize với thông tin cụ thể hơn.
            let arguments = payload
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            // Hàng rào allowlist — xem `llm::tool_calling::guard_direct_call`.
            // Tới 26/07/2026 nhánh này gọi thẳng `state.mcp_server` không kiểm gì,
            // nên `write_markdown` mở cho bất kỳ client nào nối được vào lớp lệnh
            // (WS 8002 chưa có xác thực). Phát hiện ở tài liệu 02 §C1.1.
            llm::tool_calling::guard_direct_call(llm::tool_calling::NATIVE_SERVER, &name)?;

            let result = state
                .mcp_server
                .call_tool(mcp::protocol::CallToolRequest { name, arguments })
                .await?;
            serde_json::to_value(result)
                .map_err(|e| format!("Failed to serialize tool result: {}", e))
        }

        // ── MCP Client — chiều gọi RA ngoài (G0) ───────────────────────────
        // Kết nối các server ngoài trong `mcp_config.json`.
        // Registry là singleton phạm vi tiến trình: mỗi client giữ một tiến
        // trình con thật. Xem `mcp::client::global_registry`.
        "mcp_client:list_servers" => Ok(mcp::client::global_registry().list_servers().await),

        "mcp_client:list_tools" => {
            let server = payload
                .get("server")
                .and_then(|v| v.as_str())
                .ok_or("Thiếu 'server'. Dùng mcp_client:list_servers để xem danh sách.")?;
            let tools = mcp::client::global_registry().list_tools(server).await?;
            serde_json::to_value(tools).map_err(|e| format!("Failed to serialize tool list: {}", e))
        }

        "mcp_client:call_tool" => {
            let server = payload
                .get("server")
                .and_then(|v| v.as_str())
                .ok_or("Thiếu 'server'. Dùng mcp_client:list_servers để xem danh sách.")?;
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("Thiếu 'name' (tên tool). Dùng mcp_client:list_tools để xem danh sách.")?
                .to_string();
            let arguments = payload
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            // Hàng rào allowlist. Nhánh này nghiêm trọng hơn `mcp:call_tool`: nó
            // tới được MỌI tool trên MỌI server MCP ngoài trong `mcp_config.json`
            // — tiến trình của người lạ, với đúng quyền chúng có. Mặc định
            // `ExecPolicy` cho tool ngoài là ProposeOnly, nên mặc định là TỪ CHỐI.
            llm::tool_calling::guard_direct_call(server, &name)?;

            let result = mcp::client::global_registry()
                .call_tool(server, mcp::protocol::CallToolRequest { name, arguments })
                .await?;
            serde_json::to_value(result)
                .map_err(|e| format!("Failed to serialize tool result: {}", e))
        }

        _ => Err(format!("Command '{command}' does not belong to MCP domain")),
    }
}
