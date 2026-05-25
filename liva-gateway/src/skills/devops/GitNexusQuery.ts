import { logger } from "@utils/logger";
import { EmbeddingService } from "@services/EmbeddingService";
import { StructuredMemory } from "@memory/StructuredMemory";

export const metadata = {
    name: "gitnexus_query",
    search_keywords: ["gitnexus", "truy vấn code", "tìm code", "kiến trúc"],
    description: "[SILENT] Query system architecture using ultra-fast vector search (Zero VRAM Leak)",
    parameters: {
        type: "object",
        properties: {
            query: { type: "string", description: "Description to search in source code" }
        },
        required: ["query"]
    }
};

// [v19] Lazy singleton for StructuredMemory (will be replaced by DI)
let _sm: StructuredMemory | null = null;
const getSM = async (): Promise<StructuredMemory> => {
    if (!_sm) _sm = await StructuredMemory.create("liva_core");
    return _sm;
};

export const execute = async (args: { query: string }): Promise<string> => {
    try {
        logger.info(`[GitNexusQuery] Đang nhúng query: ${args.query}`);
        
        // Zero VRAM Leak: Re-use Singleton
        const embedding = await EmbeddingService.getInstance().embed(args.query);
        const sm = await getSM();

        if (!sm.vecReady) {
            throw new Error("sqlite-vec chưa được khởi tạo");
        }

        // Auto-Truncation: Limit to Top 3 to avoid Context Window overflow
        const results = await sm.searchSimilarVectors(embedding, 3);

        let output = `Kết quả tìm kiếm cho "${args.query}":\n\n`;
        for (const block of results) {
            output += `[${block.type}] ${block.domain}/${block.category}\n\`\`\`\n${block.content}\n\`\`\`\n\n`;
        }

        if (results.length === 0) {
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
