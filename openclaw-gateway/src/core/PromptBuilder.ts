import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import {BASE_SYSTEM_PROMPT} from "../system_prompt";

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

    // Nạp danh sách công cụ
    public static buildToolsPrompt(toolsDef: any[]): string {
        const nowStr = new Date().toLocaleString("vi-VN", {
            timeZone: "Asia/Ho_Chi_Minh",
            dateStyle: "short",
            timeStyle: "short",
        });

        // Bơm thêm Tool handoff_to_expert tự động vào cuối
        toolsDef.push({
            name: "handoff_to_expert",
            description: "Kích hoạt AI Chuyên Gia (26B) chạy trên VRAM để giải quyết nhiệm vụ phức tạp, đọc phân tích văn bản dài hoặc lập trình. HÃY dùng lệnh này nếu người dùng yêu cầu task nặng/khó.",
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
        });

        return `# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n${JSON.stringify(toolsDef, null, 2)}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{"name": <function-name>, "arguments": <args-json-object>}\n</tool_call>\n\nHƯỚNG DẪN THÊM:\n- HÃY GỌI MỘT TOOL NGAY NẾU BẠN CẦN LÀM NHIỆM VỤ THAY VÌ LUYÊN THUYÊN.\n- NẾU NHIỆM VỤ QUÁ LỚN: Sử dụng ngay 'handoff_to_expert'.\n- ĐẶT CÂU HỎI TRỰC TIẾP: Nếu yêu cầu của người dùng thiếu dữ liệu/file cần thiết, đừng tự bịa chuyện, hãy hỏi ngay người dùng.\n\nNGỮ CẢNH HỆ THỐNG:\n- Thời gian: ${nowStr}`;
    }

    public static async prepareFullAiMessages(
        userText: string,
        memory: MemoryManager,
        currentLocation: string,
        toolsDef: any[]
    ): Promise<any[]> {
        const context = await this.buildContextPrompt(memory, currentLocation);
        const toolsPrompt = this.buildToolsPrompt(toolsDef);
        const systemFinal = `${BASE_SYSTEM_PROMPT}\n\n${toolsPrompt}${context}`;

        const shortTermHistory = await memory.getHybridContext(userText, 6);

        let aiMessages: any[] = [{ role: "system", content: systemFinal }];
        for (const msg of shortTermHistory) {
            aiMessages.push({ role: msg.role, content: msg.content });
        }
        return aiMessages;
    }
}
