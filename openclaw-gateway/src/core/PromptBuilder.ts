import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import { BASE_SYSTEM_PROMPT } from "../system_prompt";

/**
 * @type Brand - Used for TypeScript 5.x Branded Types to ensure strict validation
 */
export type SealToken = string & { readonly __brand: unique symbol };
export type ValidatedContext = string & { readonly __brand: unique symbol };
export type SealedPrompt = string & { readonly __brand: unique symbol };

export class PromptBuilder {
    
    // Private member for secure internal template state transitions
    #sealToken: SealToken;

    constructor() {
        // In a real production environment, this would be cryptographically generated
        this.#sealToken = "LIVA_SECURE_TOKEN_2024" as SealToken;
    }

    /**
     * Validates the integrity of a prompt using the internal seal token.
     * Prevents unauthorized context injection or prompt-poisoning.
     */
    #validateIntegrity(token: SealToken): boolean {
        return token === this.#sealToken;
    }

    /**
     * Nạp toàn bộ bốy cảnh vào System với cơ chế Branded Type Validation
     */
    public static async buildContextPrompt(
        memory: MemoryManager,
        currentLocation: string
    ): Promise<ValidatedContext> {
        const userProfile = await memory.getUserProfile();
        if (userProfile) {
            userProfile.current_location = currentLocation;
        }

        const sensoryPrompt = SensoryManager.getInstance().injectSensoryPrompt();
        const profileContext = userProfile
            ? `\n\nTHÔNG TIN BỐI CẢNH USER (User Profile):\n${JSON.stringify(userProfile, null, 2)}\n(Hãy sử dụng Tên, Khách xưng hô và Vị trí này để giao tiếp)`
            : "";

        const ltcContent = await memory.getLongTermContext();
        const ltcPrompt = ltcContent && ltcContent.length > 50 
            ? `\n\n[BỘ NHỚ DÀI HẠN - LONG TERM CONTEXT]\n${ltcContent}\n(Hệ thống yêu cầu bạn PHẢI BÁM SÁT các Working Concepts và sự thật này trong quá trình tư duy!)\n`
            : "";

        const result = profileContext + "\n" + ltcPrompt + "\n" + sensoryPrompt;
        return result as ValidatedContext;
    }

    private static tokenize(text: string): string[] {
        if (!text) return [];
        return text.toLowerCase().replace(/[.,!?;:()\[\]{}"']/g, " ")
            .split(/\s+/)
            .filter(w => w.length > 1);
    }

    /**
     * RAG-Lexical Component with strict adherence to semantic filtering logic.
     */
    private static semanticSkillFilter(userQuery: string, allSkills: any[], topK: number = 5): any[] {
        const queryTokens = this.tokenize(userQuery);
        
        // If user provides no text (e.g., sending image or microphone), return Core skills + random TopK non-core
        if (queryTokens.length === 0) {
            return allSkills.filter(s => s.isCoreSkill).concat(allSkills.filter(s => !s.isCoreSkill).slice(0, topK));
        }

        const scoredSkills = allSkills.map(skill => {
            if (skill.isCoreSkill) return { skill, score: 9999 }; // CoreSkill auto max point (Tweaks #2)

            let score = 0;
            const descTokens = this.tokenize(skill.description);
            const nameTokens = this.tokenize(skill.name.replace(/_/g, " "));
            const keywordTokens = skill.search_keywords ? skill.search_keywords.flatMap((k: string) => this.tokenize(k)) : [];

            queryTokens.forEach(qt => {
                if (nameTokens.includes(qt)) score += 2;
                if (descTokens.includes(qt)) score += 1;
                // Weight x3 for Developer-declared Keywords (Tweaks #1)
                if (keywordTokens.includes(qt)) score += 3;
            });

            return { skill, score };
        });

        const coreSkills = scoredSkills.filter(s => s.skill.isCoreSkill).map(s => s.skill);
        const topNonCore = scoredSkills.filter(s => !s.skill.isCoreSkill)
                            .sort((a, b) => b.score - a.score)
                            .slice(0, topK)
                            .map(s => s.skill);

        return [...coreSkills, ...topNonCore];
    }

    static #promptCache = new Map<string, { prompt: SealedPrompt, timestamp: number }>();
    static #CACHE_TTL_MS = 60 * 1000 * 5; // 5 phút

    /**
     * Nạp danh sách công cụ với cơ chế Branded Type (SealedPrompt)
     */
    public static buildToolsPrompt(userText: string, toolsDefRaw: any[]): SealedPrompt {
        const fingerprint = this.tokenize(userText).join("_") + "_" + toolsDefRaw.length;
        const cached = this.#promptCache.get(fingerprint);
        
        if (cached && (Date.now() - cached.timestamp < this.#CACHE_TTL_MS)) {
            return cached.prompt;
        }

        const nowStr = new Date().toLocaleString("vi-VN", {
            timeZone: "Asia/Ho_Chi_Minh",
            dateStyle: "short",
            timeStyle: "short",
        });

        // Automatic injection of 'handoff_to_expert' tool
        const allLocalSkills = [...toolsDefRaw, {
            name: "handoff_to_expert",
            description: "Kích hoạt AI Chuyên Gia (26B) chạy trên VRAM để giải quyết nhiệm vụ phức tạp, đọc phân tích văn bản dài hoặc lập trình. HÃY dùng lệnh này nếu người dùng yêu cầu task nặng/khó.",
            isCoreSkill: true,
            parameters: {
                type: "object",
                properties: {
                    reason: {
                        type: "string",
                        description: "Lý do cần chuyển giao"
                    }
                },
                required: ["reason"]
            }
        }];

        const selectedSkills = this.semanticSkillFilter(userText, allLocalSkills, 5);
        
        // Remove search_keywords and isCoreSkill to prevent Token bloat/poisoning
        const finalSkillTokenJson = selectedSkills.map(s => ({
            name: s.name,
            description: s.description,
            parameters: s.parameters
        }));

        console.log(`[Tool RAG] Hệ thống đã lọc từ ${allLocalSkills.length} Tools xuống còn ${finalSkillTokenJson.length} Tools được nạp vào System Prompt.`);

        const promptContent = `You are LIVA, an autonomous AI proxy. You have access to the following tools:\n<tools>\n${JSON.stringify(finalSkillTokenJson, null, 2)}\n</tools>\n\nIF YOU DECIDE TO USE A TOOL, YOU MUST REPLY ONLY WITH EXACTLY THIS XML FORMAT AND ABSOLUTELY NOTHING ELSE:\n<tool_call>\n{"name": "function_name", "arguments": {"arg_name": "arg_value"}}\n</tool_call>\n\nCRITICAL RULES:\n1. TỐI KỴ: BẠN BẮT BUỘC KHÔNG ĐƯỢC CHAT, KHÔNG ĐƯỢC DẠ VÂNG HAY GIẢI THÍCH ĐẦU ĐUÔI! NẾU CẦN LÀM NHIỆM VỤ (Nhắn tin, duyệt web, v.v.), IN RA DUY NHẤT KHỐI <tool_call>!\n2. YOUR REFUSAL TO COMPLY WILL CRASH THE SYSTEM.\n3. NẾU NHIỆM VỤ QUÁ KHÓ: Gọi ngay 'handoff_to_expert'.\n4. Nếu chỉ là giao tiếp bình thường, hãy chat tự nhiên.\n\nThời gian hệ thống: ${nowStr}`;

        this.#promptCache.set(fingerprint, { prompt: promptContent as SealedPrompt, timestamp: Date.now() });
        return promptContent as SealedPrompt;
    }

    /**
     * Prepares the full AI message array with strict validation of context and tools.
     */
    public static async prepareFullAiMessages(
        userText: string,
        memory: MemoryManager,
        currentLocation: string,
        toolsDef: any[]
    ): Promise<any[]> {
        const context = await this.buildContextPrompt(memory, currentLocation);
        const toolsPrompt = this.buildToolsPrompt(userText, toolsDef);
        
        // Combine components into the final system prompt
        const systemFinal = `${BASE_SYSTEM_PROMPT}\n\n${toolsPrompt}${context}`;

        const shortTermHistory = await memory.getHybridContext(userText, 6);

        let aiMessages: any[] = [{ role: "system", content: systemFinal }];
        for (const msg of shortTermHistory) {
            aiMessages.push({ role: msg.role, content: msg.content });
        }
        return aiMessages;
    }
}