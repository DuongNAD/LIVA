import { StructuredMemory } from "@memory/StructuredMemory";

import { logger } from "@utils/logger";
/**
 * UpdateMemory — AI-Initiated Memory Storage Skill
 * ==================================================
 * Allows the AI to proactively store important personal information
 * when it recognizes something worth remembering during conversation.
 * 
 * This is a TOOL the AI can call — complementing the automatic
 * PersonalKnowledgeExtractor with explicit, intentional memory writes.
 * 
 * Example triggers:
 *   User: "Nhớ giúp anh: sinh nhật vợ anh ngày 20/5"
 *   AI calls: update_memory({key: "sinh_nhat_vo", value: "20/5", category: "Sự kiện"})
 */

// Singleton StructuredMemory instance — will be set by SkillRegistry during init
let memoryInstance: StructuredMemory | null = null;

export function setMemoryInstance(memory: StructuredMemory): void {
    memoryInstance = memory;
}

export const metadata = {
    name: "update_memory",
    search_keywords: [
        "update_memory", "nhớ", "ghi nhớ", "remember", "lưu", "save",
        "nhớ giúp", "ghi lại", "lưu lại", "đừng quên", "ghi chú"
    ],
    description: "Ghi nhớ thông tin quan trọng về người dùng vào bộ nhớ dài hạn. Dùng khi người dùng yêu cầu nhớ điều gì đó, hoặc khi phát hiện thông tin cá nhân quan trọng (sinh nhật, sở thích, thói quen, người thân, sự kiện).",
    isCoreSkill: true,
    parameters: {
        type: "object",
        properties: {
            key: {
                type: "string",
                description: "Tên thông tin dạng snake_case ngắn gọn (VD: sinh_nhat_vo, ten_me, so_thich_tra)"
            },
            value: {
                type: "string",
                description: "Nội dung cần ghi nhớ (VD: 'Vợ sinh nhật ngày 20/5, thích hoa hồng')"
            },
            category: {
                type: "string",
                enum: ["Người thân", "Sở thích", "Thói quen", "Công việc", "Sự kiện", "Cảm xúc", "Chung"],
                description: "Phân loại thông tin"
            }
        },
        required: ["key", "value", "category"]
    }
};

export const execute = async (args: {
    key: string;
    value: string;
    category: string;
}): Promise<string> => {
    try {
        if (!memoryInstance) {
            // Fallback: create instance if not injected
            memoryInstance = new StructuredMemory("liva_core");
        }

        const validCategories = ["Người thân", "Sở thích", "Thói quen", "Công việc", "Sự kiện", "Cảm xúc", "Chung"];
        const category = validCategories.includes(args.category) ? args.category : "Chung";

        // Set TTL based on category
        let ttlDays: number | undefined;
        if (category === "Cảm xúc") ttlDays = 7;
        else if (category === "Sự kiện") ttlDays = 30;

        memoryInstance.setFact(args.key, args.value, {
            source: "ai_tool",
            category,
            ttlDays
        });

        logger.info(`[UpdateMemory] ✅ Đã ghi nhớ: ${args.key} = "${args.value}" (${category})`);
        return `Đã ghi nhớ thành công: "${args.key}" → "${args.value}" (Phân loại: ${category}). Thông tin này sẽ được LIVA nhớ trong mọi cuộc trò chuyện sau.`;
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        return `Lỗi ghi nhớ: ${errMsg}`;
    }
};
