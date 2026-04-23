import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { LocalMCPServer } from "./LocalMCPServer";

async function main() {
    // 1. Khởi tạo MCP Server nội bộ chuyên trị Legacy Skills
    const localMCPServer = new LocalMCPServer();
    
    // 2. Load toàn bộ 29 tools từ thư mục src/skills
    await localMCPServer.loadSkills();

    // 3. Khởi tạo Stdio Transport (dùng qua shell command npx tsx)
    const transport = new StdioServerTransport();
    
    // 4. Kết nối và phục vụ
    await localMCPServer.getServerInstance().connect(transport);
    
    // stdio transport uses stdout for JSON-RPC, so we MUST log with console.error
    console.error("[LocalAdapterServer] 🚀 MCP Adapter đã khởi chạy. Bọc thành công toàn bộ Legacy Skills.");
}

main().catch((err) => {
    console.error("[LocalAdapterServer] Fatal Error:", err);
    process.exit(1);
});
