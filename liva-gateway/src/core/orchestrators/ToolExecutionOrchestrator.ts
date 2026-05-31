import { SkillRegistry } from "../../SkillRegistry";
import OpenAI from "openai";
import { logger } from "../../utils/logger";
import { ZMAS_Guard } from "../../security/ZMAS_Guard";

export class ToolExecutionOrchestrator {
    #registry: SkillRegistry;
    #aiRouterClient: OpenAI;
    public onExecApprovalRequired?: (toolName: string, command: string, reason: string) => Promise<{ approved: boolean; editedCommand?: string }>;
    private logger: any;

    constructor(registry: SkillRegistry, routerClient: OpenAI) {
        this.#registry = registry;
        this.#aiRouterClient = routerClient;
        this.logger = logger.child({ component: 'ToolExecutionOrchestrator' });
    }

    async executeWithReflection(toolName: string, args: any): Promise<{ resultStr: string; valid: boolean; rawObj: any }> {
        try {
            const resultObj = await this.#registry.executeSkill(toolName, args);
            let resultStr = typeof resultObj === "string" ? resultObj : JSON.stringify(resultObj);

            const zmas = new ZMAS_Guard();
            resultStr = zmas.executeAutoRemediation(resultStr, toolName);

            if (resultStr.length > 2000) {
                this.logger.warn(`Dữ liệu dài (${resultStr.length} chars). Chuyển hướng chui qua [Sanitizer Sub-Agent]...`);
                resultStr = await this.sanitize(resultStr);
            }

            // [REFLECTION LAYER V2 — Rule-Based Validation]
            // Thay thế AI Reflection (chậm ~3s, sai lệch trên Router 4B) bằng heuristic nhanh O(1)
            const lowerResult = resultStr.toLowerCase();
            const isValid = resultStr.length > 5
                && !lowerResult.includes("traceback (most recent call last)")
                && !lowerResult.includes("error: spawn")
                && !lowerResult.includes("econnrefused")
                && !lowerResult.includes("timeout sandbox")
                && !lowerResult.includes("unable to ")
                && !lowerResult.includes("failed to ")
                && !(resultStr.startsWith("{") && resultStr.includes('"error"'));

            return { resultStr, valid: isValid, rawObj: resultObj };
        } catch (toolError: unknown) {
        const errMsg = toolError instanceof Error ? toolError.message : String(toolError);
            return { resultStr: `Tool runtime error: ${errMsg}`, valid: false, rawObj: null };
        }
    }

    private async sanitize(rawString: string): Promise<string> {
        try {
            const res = await this.#aiRouterClient.chat.completions.create({
                model: "router",
                messages: [
                    { role: "system", content: "You are a neutral data filter. ACCURATELY AND OBJECTIVELY SUMMARIZE the provided content. MUST NOT reply or address anyone, only return the raw summarized text." },
                    { role: "user", content: `Summarize:\n${rawString.substring(0, 6000)}` }
                ],
                temperature: 0.1,
            });
            return res.choices[0].message?.content || rawString.substring(0, 1500);
        } catch {
            return rawString.substring(0, 1500) + "\n\n[System: Data too large, safely trimmed]";
        }
    }

    public heuristicSanitize(data: string, maxLen = 2500): string {
        if (data.length <= maxLen) return data;
        
        const trimmedData = data.trim();
        if (trimmedData.startsWith("{") || trimmedData.startsWith("[")) {
            try {
                const parsed = JSON.parse(trimmedData);
                if (Array.isArray(parsed) && parsed.length > 20) {
                    const truncated = parsed.slice(0, 20);
                    return JSON.stringify(truncated) + "\n\n<truncated_output>... [System: Cắt xén do vượt quá kích thước. DO NOT PARSE THIS AS JSON. Vui lòng gọi tool khác hỗ trợ phân trang (pagination) nếu cần thêm dữ liệu] ...</truncated_output>\n";
                }
            } catch {
                // Ignore parsing errors
            }
        }
        
        const head = data.substring(0, maxLen / 2);
        const tail = data.substring(data.length - (maxLen / 2));
        return "\n<truncated_output>\n" + head + "\n... [System: Cắt xén do vượt quá kích thước. DO NOT PARSE THIS AS JSON. Vui lòng gọi tool khác hỗ trợ phân trang (pagination) nếu cần thêm dữ liệu] ...\n" + tail + "\n</truncated_output>\n";
    }
}
