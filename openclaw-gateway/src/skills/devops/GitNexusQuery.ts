import { logger } from "@utils/logger";
import { EmbeddingService } from "@services/EmbeddingService";
import { LanceMemoryManager } from "@memory/LanceMemory";

export const metadata = {
    name: "gitnexus_query",
    search_keywords: ["gitnexus", "truy vấn code", "tìm code", "kiến trúc"],
    description: "Truy vấn kiến trúc hệ thống bằng vector siêu tốc (Zero VRAM Leak)",
    parameters: {
        type: "object",
        properties: {
            query: { type: "string", description: "Mô tả cần tìm kiếm trong mã nguồn" }
        },
        required: ["query"]
    }
};

export const execute = async (args: { query: string }): Promise<string> => {
    try {
        logger.info(`[GitNexusQuery] Đang nhúng query: ${args.query}`);
        
        // Zero VRAM Leak: Re-use Singleton
        const embedding = await EmbeddingService.getInstance().embed(args.query);

        // Access LanceMemory singleton
        const memoryManager = new LanceMemoryManager();
        const table = await memoryManager.getTable();
        if (!table) {
            throw new Error("LanceDB table chưa được khởi tạo");
        }

        // Auto-Truncation: Limit to Top 3 to avoid Context Window overflow
        const results = await table.vectorSearch(embedding).limit(3).toArray();

        let output = `Kết quả tìm kiếm cho "${args.query}":\n\n`;
        for (const block of results) {
            output += `File: ${block.filepath || 'Unknown'}\n\`\`\`typescript\n${block.content || ''}\n\`\`\`\n\n`;
        }

        if (!results || results.length === 0) {
            output += "Không tìm thấy kết quả phù hợp.";
        }

        return output;
    } catch (e: unknown) {
    const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`[GitNexusQuery] Lỗi truy vấn: ${errMsg}`);
        throw e;
    }
};
export default { metadata, execute };
