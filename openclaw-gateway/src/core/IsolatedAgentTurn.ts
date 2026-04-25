import OpenAI from "openai";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { ToolExecutionOrchestrator } from "./ToolExecutionOrchestrator";
import { logger } from "../utils/logger";
import { PromptBuilder } from "./PromptBuilder";
import { TaskQueue } from "./TaskQueue";

/**
 * IsolatedAgentTurn
 * -----------------
 * Một luồng thực thi tách biệt hoàn toàn khỏi Pipeline hội thoại chính.
 * Cho phép AI tự động bảo trì hệ thống (System Maintenance, GC) ở Background
 * mà không làm kẹt (is_busy) AgentLoop hoặc làm ồn UI (không có TTS).
 */
export class IsolatedAgentTurn {
    #aiClient: OpenAI;
    #memory: MemoryManager;
    #registry: SkillRegistry;
    #toolOrchestrator: ToolExecutionOrchestrator;

    constructor(memory: MemoryManager, registry: SkillRegistry, aiClient: OpenAI) {
        this.#memory = memory;
        this.#registry = registry;
        this.#aiClient = aiClient;
        this.#toolOrchestrator = new ToolExecutionOrchestrator(registry, aiClient);
    }

    /**
     * Chạy một chu trình khép kín ở Background (Sử dụng TaskQueue để chống tràn VRAM)
     */
    public async runBackgroundTurn(systemGoal: string): Promise<string> {
        return TaskQueue.getInstance().enqueue(async () => {
            logger.info(`🕵️‍♂️ [IsolatedTurn] Bắt đầu phiên ngầm: ${systemGoal}`);
            
            try {
            const toolsDef = this.#registry.getAllSkills().map((s: any) => ({
                name: s.name,
                description: s.description,
                parameters: s.parameters,
            }));

            // Ép AI hiểu nó đang chạy ngầm, không được nói luyên thuyên
            const backgroundPrompt = `[LỆNH TỪ HỆ THỐNG]: Bạn đang chạy trong Isolated Background Turn (Luồng chạy ngầm). Bạn KHÔNG ĐANG NÓI CHUYỆN VỚI NGƯỜI DÙNG. 
Nhiệm vụ của bạn là đọc các dữ liệu và tự động gọi Kỹ Năng (Tool) để hoàn thành mục tiêu hệ thống. 
Sau khi xong việc, hãy trả lời cực kỳ ngắn gọn dạng BÁO CÁO NHẬT KÝ.
MỤC TIÊU CỦA BẠN LÀ: ${systemGoal}`;

            const aiMessages = await PromptBuilder.prepareFullAiMessages(
                backgroundPrompt,
                this.#memory,
                "Background Task Server",
                toolsDef
            );

            let isFinished = false;
            let turnCount = 0;
            let finalReply = "";

            let currentQuery = backgroundPrompt;

            while (!isFinished && turnCount < 3) {
                turnCount++;
                
                const stream = await this.#aiClient.chat.completions.create({
                    model: process.env.AI_PROVIDER === "cloud" ? (process.env.AI_MODEL || "gpt-4") : "local-ghost-router",
                    messages: [...aiMessages, { role: "user", content: currentQuery }],
                    temperature: 0.2,
                    max_tokens: 1500,
                    stream: false,
                });

                let contentText = stream.choices[0]?.message?.content || "";
                let parsedToolCalls: any[] = [];

                // Parse XML Tool
                if (contentText.includes("<tool_call>")) {
                    const regex = /<tool_call>([\s\S]*?)<\/tool_call>/g;
                    const matches = [...contentText.matchAll(regex)];
                    for (const match of matches) {
                        try {
                            parsedToolCalls.push(JSON.parse(match[1].trim()));
                        } catch (e) { void e; }
                    }
                    contentText = contentText.replaceAll(regex, "").trim();
                }

                if (parsedToolCalls.length > 0) {
                    let toolResultsStr = "";
                    aiMessages.push({ role: "user", content: currentQuery });
                    aiMessages.push({ role: "assistant", content: stream.choices[0]?.message?.content || "" });

                    for (const toolCall of parsedToolCalls) {
                        const executionResult = await this.#toolOrchestrator.executeWithReflection(toolCall.name, toolCall.arguments);
                        if (executionResult.valid) {
                            toolResultsStr += `[Result ${toolCall.name}]: ${executionResult.resultStr}\n`;
                        } else {
                            toolResultsStr += `[Error ${toolCall.name}]: ${executionResult.resultStr}\n`;
                        }
                    }
                    currentQuery = `[BACKGROUND TOOL DATA]:\n${toolResultsStr}\nHãy tóm tắt kết quả bảo trì này vào nhật ký.`;
                } else {
                    isFinished = true;
                    finalReply = contentText;
                }
            }

            logger.info(`🕵️‍♂️ [IsolatedTurn] Hoàn tất phiên ngầm. Log: ${finalReply}`);
            return finalReply;
            
        } catch (error: any) {
            logger.error(`[IsolatedTurn] Lỗi trong quá trình chạy ngầm: ${error.message}`);
            return `Lỗi hệ thống: ${error.message}`;
        }
        });
    }
}
