import { logger } from "../../utils/logger";
import { ToolExecutionEngine } from "./ToolExecutionEngine";

export class PlannerLoop {
    public async generateBlueprint(userText: string, context: string, engine: ToolExecutionEngine): Promise<Record<string, unknown>[]> {
        logger.info("🧠 [PlannerLoop] Đang phân tích yêu cầu phức tạp và lập Blueprint...");
        
        // Cấu hình GBNF / JSON Schema Enforcement cho llama.cpp Native Engine
        const blueprintSchema = {
            type: "json_schema",
            json_schema: {
                name: "execution_blueprint",
                schema: {
                    type: "object",
                    properties: {
                        steps: {
                            type: "array",
                            items: {
                                type: "object",
                                properties: {
                                    toolName: { type: "string" },
                                    arguments: { type: "object" },
                                    dependency: { type: "string" }
                                },
                                required: ["toolName", "arguments"]
                            }
                        }
                    },
                    required: ["steps"]
                }
            }
        };

        const response = await engine.toolOrchestrator.aiRouterClient.chat.completions.create({
            model: "local-ghost-expert",
            messages: [
                { role: "system", content: "Bạn là Não bộ Chiến lược (Planner). Dựa vào yêu cầu người dùng, hãy lập một bản kế hoạch từng bước (Blueprint) sử dụng các Tool phù hợp. Trả về đúng định dạng JSON Schema." },
                { role: "user", content: `Ngữ cảnh:\n${context}\n\nYêu cầu: ${userText}` }
            ],
            // @ts-expect-error JSON Schema format is perfectly supported by the underlying engine
            response_format: blueprintSchema, // Kích hoạt GBNF JSON Constraints
            temperature: 0.1
        });

        const rawJson = response.choices[0]?.message?.content || "{}";
        try {
            const blueprint = JSON.parse(rawJson);
            logger.info(`✅ [PlannerLoop] Đã lập Blueprint thành công với ${blueprint.steps?.length || 0} bước.`);
            return blueprint.steps || [];
        } catch {
            logger.error("❌ [PlannerLoop] Lỗi parse Blueprint JSON (Lý thuyết GBNF không bao giờ xảy ra lỗi này).");
            return [];
        }
    }
}
