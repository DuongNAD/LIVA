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
    description: "[SILENT] Store important information about the user into long-term memory. Use when the user asks you to remember something, or when you detect important personal info (birthdays, hobbies, habits, relatives, events). IMPORTANT: ALWAYS write the key, value, and category in English (en-US) to save context tokens and ensure standardized matching, even if the user speaks Vietnamese.",
    isCoreSkill: true,
    parameters: {
        type: "object",
        properties: {
            key: {
                type: "string",
                description: "Short snake_case key in English for the information (e.g., wife_birthday, mother_name, tea_hobby)"
            },
            value: {
                type: "string",
                description: "The content to remember, always written in English (e.g., 'Wife birthday is May 20, likes roses')"
            },
            category: {
                type: "string",
                enum: ["Family", "Hobbies", "Habits", "Work", "Events", "Emotions", "General"],
                description: "Category of the information"
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
            memoryInstance = await StructuredMemory.create("liva_core");
        }

        const validCategories = ["Family", "Hobbies", "Habits", "Work", "Events", "Emotions", "General"];
        const category = validCategories.includes(args.category) ? args.category : "General";

        // Set TTL based on category
        let ttlDays: number | undefined;
        if (category === "Emotions") ttlDays = 7;
        else if (category === "Events") ttlDays = 30;

        await memoryInstance.setFact(args.key, args.value, {
            source: "ai_tool",
            category,
            ttlDays
        });

        logger.info(`[UpdateMemory] ✅ Đã ghi nhớ: ${args.key} = "${args.value}" (${category})`);
        return `Memory saved: "${args.key}" → "${args.value}" (Category: ${category}). LIVA will remember this across all future conversations.`;
    } catch (error: unknown) {
    const errMsg = error instanceof Error ? error.message : String(error);
        return `Memory save error: ${errMsg}`;
    }
};
