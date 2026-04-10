import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import { BASE_SYSTEM_PROMPT } from "../system_prompt";

export class PromptBuilder {
    
    // Nạp toàn bộ bối cảnh vào System
    public static async buildContextPrompt(
        memory: MemoryManager,
        currentLocation: string
    ): Promise<string> {
        const userProfile = await memory.getUserProfile();
        if (userProfile) {
            userProfile.current_location = currentLocation;
        }

        const sensoryPrompt = SensoryManager.getInstance().injectSensoryPrompt();
        const profileContext = userProfile
            ? `\n\nTHÔNG TIN NGƯỜI DÙNG HIỆN TẠI (User Profile):\n${JSON.stringify(userProfile, null, 2)}\n(Hãy sử dụng Tên, Khách xưng hô và Vị trí này để phục vụ người dùng)`
            : "";

        return profileContext + sensoryPrompt;
    }

    private static tokenize(text: string): string[] {
        if (!text) return [];
        return text.toLowerCase().replace(/[.,!?;:()\[\]{}"']/g, " ")
            .split(/\s+/)
            .filter(w => w.length > 1);
    }

    private static semanticSkillFilter(userQuery: string, allSkills: any[], topK: number = 5): any[] {
        const queryTokens = this.tokenize(userQuery);
        
        // Nếu user chẳng gõ gì (VD: gửi ảnh, hoặc chỉ gí microphone), cứ bắn trả toàn bộ Core hoặc ngẫu nhiên TopK non-core
        if (queryTokens.length === 0) {
            return allSkills.filter(s => s.isCoreSkill).concat(allSkills.filter(s => !s.isCoreSkill).slice(0, topK));
        }

        const scoredSkills = allSkills.map(skill => {
            if (skill.isCoreSkill) return { skill, score: 9999 }; // CoreSkill auto max point (Tweak #2)

            let score = 0;
            const descTokens = this.tokenize(skill.description);
            const nameTokens = this.tokenize(skill.name.replace(/_/g, " "));
            const keywordTokens = skill.search_keywords ? skill.search_keywords.flatMap((k: string) => this.tokenize(k)) : [];

            queryTokens.forEach(qt => {
                if (nameTokens.includes(qt)) score += 2;
                if (descTokens.includes(qt)) score += 1;
                // Trọng số x3 cho Từ vựng do Dev khai báo (Tweak #1)
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

    // Nạp danh sách công cụ với RAG Component
    public static buildToolsPrompt(userText: string, toolsDefRaw: any[]): string {
        const nowStr = new Date().toLocaleString("vi-VN", {
            timeZone: "Asia/Ho_Chi_Minh",
            dateStyle: "short",
            timeStyle: "short",
        });

        // Bơm thêm Tool handoff_to_expert tự động vào cuối nhóm danh sách
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

        // Trích xuất qua Màng lọc RAG-Lexical (Đã tiêm Tweak #1 và Tweak #2)
        const selectedSkills = this.semanticSkillFilter(userText, allLocalSkills, 5);
        
        // Cần bỏ bớt field `search_keywords` và `isCoreSkill` ra khỏi Prompt để chống độn Token
        const finalSkillTokenJson = selectedSkills.map(s => ({
            name: s.name,
            description: s.description,
            parameters: s.parameters
        }));

        console.log(`[Tool RAG] Hệ thống đã lọc từ ${allLocalSkills.length} Tools xuống còn ${finalSkillTokenJson.length} Tools được nhét vào System Prompt.`);

        return `# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n${JSON.stringify(finalSkillTokenJson, null, 2)}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{"name": <function-name>, "arguments": <args-json-object>}\n</tool_call>\n\nHƯỚNG DẪN THÊM:\n- HÃY GỌI MỘT TOOL NGAY NẾU BẠN CẦN LÀM NHIỆM VỤ THAY VÌ LUYÊN THUYÊN.\n- NẾU NHIỆM VỤ QUÁ LỚN: Sử dụng ngay 'handoff_to_expert'.\n- ĐẶT CÂU HỎI TRỰC TIẾP: Nếu yêu cầu của người dùng thiếu dữ liệu/file cần thiết, đừng tự bịa chuyện, hãy hỏi ngay người dùng.\n\nNGỮ CẢNH HỆ THỐNG:\n- Thời gian: ${nowStr}`;
    }

    public static async prepareFullAiMessages(
        userText: string,
        memory: MemoryManager,
        currentLocation: string,
        toolsDef: any[]
    ): Promise<any[]> {
        const context = await this.buildContextPrompt(memory, currentLocation);
        const toolsPrompt = this.buildToolsPrompt(userText, toolsDef);
        const systemFinal = `${BASE_SYSTEM_PROMPT}\n\n${toolsPrompt}${context}`;

        const shortTermHistory = await memory.getHybridContext(userText, 6);

        let aiMessages: any[] = [{ role: "system", content: systemFinal }];
        for (const msg of shortTermHistory) {
            aiMessages.push({ role: msg.role, content: msg.content });
        }
        return aiMessages;
    }
}
