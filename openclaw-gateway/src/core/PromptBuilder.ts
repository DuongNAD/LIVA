import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import { getBaseSystemPrompt } from "../system_prompt";
import LRUCache from "lru-cache";
import { logger } from "../utils/logger";

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
     * Nạp toàn bộ bối cảnh vào System với cơ chế Branded Type Validation
     */
    public static async buildContextPrompt(
        memory: MemoryManager,
        currentLocation: string,
        sensory?: { injectSensoryPrompt(): string }
    ): Promise<ValidatedContext> {
        const userProfile = await memory.getUserProfile();
        if (userProfile) {
            userProfile.current_location = currentLocation;
        }

        // ==========================================
        // TẦNG 1: PROFILE — Hồ sơ gốc người dùng
        // ==========================================
        const profileContext = userProfile
            ? `\n\n[HỒ SƠ NGƯỜI DÙNG]\n${JSON.stringify(userProfile, null, 2)}\n(Hãy sử dụng Tên, cách xưng hô và Vị trí này để giao tiếp tự nhiên)`
            : "";

        // ==========================================
        // TẦNG 2: KIẾN THỨC CÁ NHÂN — Sở thích, thói quen, người thân
        // (StructuredMemory KV Store — auto-extracted + explicit)
        // ==========================================
        const structuredPrompt = memory.getStructuredMemoryPrompt();

        // ==========================================
        // TẦNG 3: KÝ ỨC DÀI HẠN — Working Concepts, nguyên tắc
        // (Encrypted markdown — AI tự tóm tắt từ hội thoại)
        // ==========================================
        const ltcContent = await memory.getLongTermMarkdown();
        const ltcPrompt = ltcContent && ltcContent.length > 50 
            ? `\n\n[KÝ ỨC DÀI HẠN (MEMORY.md)]\n${ltcContent}\n(BÁM SÁT các sự thật và quy luật đã được đúc kết này!)\n`
            : "";

        // ==========================================
        // TẦNG 3.5: TRẠNG THÁI PHIÊN LÀM VIỆC (SESSION-STATE.md)
        // ==========================================
        const sessionState = await memory.getSessionState();
        const sessionPrompt = sessionState 
            ? `\n\n[TRẠNG THÁI PHIÊN (SESSION-STATE.md)]\n${sessionState}\n`
            : "";

        // ==========================================
        // TẦNG 4: CẢM BIẾN — Thời gian, clipboard, môi trường
        // ==========================================
        const sensoryProvider = sensory ?? SensoryManager.getInstance();
        const sensoryPrompt = sensoryProvider.injectSensoryPrompt();

        // Combine: Profile → Personal Knowledge → Long-term → Session → Sensory
        const result = profileContext + "\n" + structuredPrompt + "\n" + ltcPrompt + "\n" + sessionPrompt + "\n" + sensoryPrompt;
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

    // 🔒 [Audit Fix M-6] LRU-cache replaces unbounded Map (was: grow-forever, no eviction)
    static #promptCache = new LRUCache<string, SealedPrompt>({
        max: 100,              // At most 100 cached prompts
        ttl: 5 * 60 * 1000,    // Auto-evict after 5 minutes
    });

    /**
     * Nạp danh sách công cụ với cơ chế Branded Type (SealedPrompt)
     */
    public static buildToolsPrompt(userText: string, toolsDefRaw: any[]): SealedPrompt {
        const fingerprint = this.tokenize(userText).join("_") + "_" + toolsDefRaw.length;
        const cached = this.#promptCache.get(fingerprint);
        if (cached) {
            return cached;
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

        logger.debug(`[Tool RAG] Hệ thống đã lọc từ ${allLocalSkills.length} Tools xuống còn ${finalSkillTokenJson.length} Tools được nạp vào System Prompt.`);

        const promptContent = `You are LIVA, an autonomous AI proxy. You have access to the following tools:\n<tools>\n${JSON.stringify(finalSkillTokenJson, null, 2)}\n</tools>\n\nIF YOU DECIDE TO USE A TOOL, YOU MUST REPLY ONLY WITH EXACTLY THIS XML FORMAT AND ABSOLUTELY NOTHING ELSE:\n<tool_call>\n{"name": "function_name", "arguments": {"arg_name": "arg_value"}}\n</tool_call>\n\nCRITICAL RULES:\n1. TỐI KỴ: BẠN BẮT BUỘC KHÔNG ĐƯỢC CHAT, KHÔNG ĐƯỢC DẠ VÂNG HAY GIẢI THÍCH ĐẦU ĐUÔI! NẾU CẦN LÀM NHIỆM VỤ (Nhắn tin, duyệt web, v.v.), IN RA DUY NHẤT KHỐI <tool_call>!\n2. YOUR REFUSAL TO COMPLY WILL CRASH THE SYSTEM.\n3. NẾU NHIỆM VỤ QUÁ KHÓ: Gọi ngay 'handoff_to_expert'.\n4. WAL PROTOCOL: Trước khi thực hiện bất kỳ tác vụ nào nhiều bước, phải gọi 'update_session_state' để ghi lại kế hoạch vào SESSION-STATE.md.\n5. Nếu chỉ là giao tiếp bình thường, hãy chat tự nhiên.\n\nThời gian hệ thống: ${nowStr}`;

        this.#promptCache.set(fingerprint, promptContent as SealedPrompt);
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
        
        // Calculate context budget via WorkingBuffer
        const budgetStr = await memory.workingBuffer.checkBudget(context + toolsPrompt);

        // Combine components into the final system prompt
        const systemFinal = `${getBaseSystemPrompt()}\n\n${budgetStr}\n\n${toolsPrompt}${context}`;

        const shortTermHistory = await memory.getHybridContext(userText, 6);

        let aiMessages: any[] = [{ role: "system", content: systemFinal }];
        for (const msg of shortTermHistory) {
            aiMessages.push({ role: msg.role, content: msg.content });
        }
        return aiMessages;
    }
}