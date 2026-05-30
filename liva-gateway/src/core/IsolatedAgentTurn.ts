import OpenAI from "openai";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { ToolExecutionOrchestrator } from "./orchestrators/ToolExecutionOrchestrator";
import { logger } from "../utils/logger";
import { PromptBuilder } from "./PromptBuilder";
import { TaskQueue } from "./TaskQueue";
import { ToolCallExtractor } from "./stream/ToolCallExtractor";
import { ConfigManager } from "./config/ConfigManager";

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
    #toolCallExtractor: ToolCallExtractor;

    constructor(memory: MemoryManager, registry: SkillRegistry, aiClient: OpenAI) {
        this.#memory = memory;
        this.#registry = registry;
        this.#aiClient = aiClient;
        this.#toolOrchestrator = new ToolExecutionOrchestrator(registry, aiClient);
        this.#toolCallExtractor = new ToolCallExtractor();
    }

    /**
     * Chạy một chu trình khép kín ở Background (Sử dụng TaskQueue để chống tràn VRAM)
     */
    public async runBackgroundTurn(systemGoal: string): Promise<string> {
        return TaskQueue.getInstance().enqueue(async () => {
            logger.info(`🕵️‍♂️ [IsolatedTurn] Bắt đầu phiên ngầm: ${systemGoal}`);
            
            try {
            // [v28] Semantic filter: only inject relevant tools for background task (saves ~1500 tokens)
            const topSkills = await this.#registry.getSemanticTopK(systemGoal, undefined, 5);
            const toolsDef = topSkills.map((s: any) => ({
                name: s.name,
                description: s.short_desc || s.description?.substring(0, 80),
                parameters: s.parameters,
            }));

            // Ép AI hiểu nó đang chạy ngầm, không được nói luyên thuyên
            const backgroundPrompt = `[LỆNH TỪ HỆ THỐNG]: Bạn đang chạy trong Isolated Background Turn (Luồng chạy ngầm). Bạn KHÔNG ĐANG NÓI CHUYỆN VỚI NGƯỜI DÙNG. 
Nhiệm vụ của bạn là đọc các dữ liệu và tự động gọi Kỹ Năng (Tool) để hoàn thành mục tiêu hệ thống. 
Sau khi xong việc, hãy trả lời cực kỳ ngắn gọn dạng BÁO CÁO NHẬT KÝ.
MỤC TIÊU CỦA BẠN LÀ: ${systemGoal}`;

            const { aiMessages, dynamicContextBlock } = await PromptBuilder.prepareFullAiMessages(
                backgroundPrompt,
                this.#memory,
                { location: "Background Task Server", timezone: "Asia/Ho_Chi_Minh" },
                toolsDef
            );

            let isFinished = false;
            let turnCount = 0;
            let finalReply = "";

            let currentQuery = backgroundPrompt;

            const executionMessages = [...aiMessages];

            while (!isFinished && turnCount < 3) {
                turnCount++;
                
                if (turnCount === 1) {
                    const nowStr = new Date().toLocaleString("vi-VN", {
                        timeZone: "Asia/Ho_Chi_Minh",
                    });
                    const dynamicContext = `\n\n<DYNAMIC_CONTEXT>\nSystem Time: ${nowStr}\nUser's Real-Time Location (via IP/GPS): Background Task Server\n</DYNAMIC_CONTEXT>`;
                    
                    executionMessages.push({
                        role: "user",
                        content: currentQuery + dynamicContextBlock + dynamicContext
                    });
                } else {
                    executionMessages.push({
                        role: "user",
                        content: currentQuery
                    });
                }

                const stream = await this.#aiClient.chat.completions.create({
                    model: ConfigManager.getInstance().aiProvider === "cloud" ? (ConfigManager.getInstance().env.AI_MODEL) : "local-ghost-router",
                    messages: executionMessages,
                    temperature: 0.2,
                    max_tokens: 1500,
                    stream: false,
                });

                const rawContent = stream.choices[0]?.message?.content || "";
                
                // [v27 FIX] Use ToolCallExtractor instead of raw regex + JSON.parse
                // Previously: crash on malformed JSON, no thinking block stripping, no jsonrepair fallback
                const extraction = this.#toolCallExtractor.extract(rawContent);
                const contentText = extraction.cleanedContent;
                const parsedToolCalls = extraction.parsedToolCalls;

                if (parsedToolCalls.length > 0) {
                    let toolResultsStr = "";
                    executionMessages.push({ role: "assistant", content: rawContent });

                    for (const toolCall of parsedToolCalls) {
                        const functionArgs = this.#toolCallExtractor.parseArguments(toolCall.name, toolCall.arguments);
                        if (functionArgs === null) {
                            toolResultsStr += `[Error ${toolCall.name}]: Malformed arguments\n`;
                            continue;
                        }
                        const executionResult = await this.#toolOrchestrator.executeWithReflection(toolCall.name, functionArgs);
                        if (executionResult.valid) {
                            toolResultsStr += `[Result ${toolCall.name}]: ${executionResult.resultStr}\n`;
                        } else {
                            toolResultsStr += `[Error ${toolCall.name}]: ${executionResult.resultStr}\n`;
                        }
                    }
                    currentQuery = `[BACKGROUND TOOL DATA]:\n${toolResultsStr}\nHãy tóm tắt kết quả bảo trì này vào nhật ký.`;
                } else {
                    executionMessages.push({ role: "assistant", content: rawContent });
                    isFinished = true;
                    finalReply = contentText;
                }
            }

            logger.info(`🕵️‍♂️ [IsolatedTurn] Hoàn tất phiên ngầm. Log: ${finalReply}`);
            return finalReply;
            
        } catch (error: unknown) {
            const errMsg = error instanceof Error ? error.message : String(error);
            logger.error(`[IsolatedTurn] Lỗi trong quá trình chạy ngầm: ${errMsg}`);
            return `Lỗi hệ thống: ${errMsg}`;
        }
        });
    }
}

