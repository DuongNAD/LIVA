import { SkillRegistry } from "../SkillRegistry";
import OpenAI from "openai";
import { ZMAS_Guard } from "../security/ZMAS_Guard";
import { logger } from "../utils/logger";

export class ToolExecutionOrchestrator {
    #registry: SkillRegistry;
    #aiRouterClient: OpenAI;
    #guard: ZMAS_Guard;
    
    // Callback chờ người dùng duyệt lệnh qua UI
    public onExecApprovalRequired?: (
        toolName: string, 
        command: string, 
        reason: string
    ) => Promise<{ approved: boolean; editedCommand?: string }>;

    constructor(registry: SkillRegistry, routerClient: OpenAI, guard: ZMAS_Guard = new ZMAS_Guard()) {
        this.#registry = registry;
        this.#aiRouterClient = routerClient;
        this.#guard = guard;
    }

    async executeWithReflection(toolName: string, args: any): Promise<{ resultStr: string; valid: boolean; rawObj: any }> {
        try {
            // [Z-MAS ZERO-TRUST] Exec Approval Mode
            const dangerousTools = ["run_shell_command", "run_python_script", "docker_exec"];
            if (dangerousTools.includes(toolName) && this.onExecApprovalRequired) {
                const cmdString = args.command || args.script || args.code || JSON.stringify(args);
                const reason = args.reason || args.intent || "AI cần chạy mã thực thi trên hệ thống cục bộ.";
                
                logger.warn(`⚠️ [Zero-Trust] Yêu cầu duyệt lệnh (Human-in-the-loop) cho công cụ: ${toolName}`);
                const decision = await this.onExecApprovalRequired(toolName, cmdString, reason);
                
                if (!decision.approved) {
                    logger.warn(`🛑 [Zero-Trust] Người dùng đã từ chối lệnh.`);
                    return { resultStr: "Lỗi Bảo Mật: Người dùng (Human-in-the-loop) đã TỪ CHỐI thực thi lệnh này.", valid: false, rawObj: null };
                }
                
                if (decision.editedCommand) {
                    logger.info(`✅ [Zero-Trust] Người dùng đã duyệt (với lệnh chỉnh sửa).`);
                    if (args.command) args.command = decision.editedCommand;
                    else if (args.script) args.script = decision.editedCommand;
                    else if (args.code) args.code = decision.editedCommand;
                } else {
                    logger.info(`✅ [Zero-Trust] Người dùng đã duyệt lệnh nguyên bản.`);
                }
            }

            const resultObj = await this.#registry.executeSkill(toolName, args);
            let resultStr = typeof resultObj === "string" ? resultObj : JSON.stringify(resultObj);

            resultStr = this.#guard.executeAutoRemediation(resultStr, toolName);

            if (resultStr.length > 2000) {
                logger.warn(`Dữ liệu dài (${resultStr.length} chars). Chuyển hướng chui qua [Sanitizer Sub-Agent]...`);
                resultStr = await this.sanitize(resultStr);
            }

            const lowerResult = resultStr.toLowerCase();
            const isValid = resultStr.length > 5
                && !lowerResult.includes("traceback (most recent call last)")
                && !lowerResult.includes("error: spawn")
                && !lowerResult.includes("econnrefused")
                && !lowerResult.includes("timeout sandbox")
                && !(resultStr.startsWith("{") && resultStr.includes('"error"'));

            return { resultStr, valid: isValid, rawObj: resultObj };
        } catch (toolError: any) {
            return { resultStr: `Tool runtime error: ${toolError.message}`, valid: false, rawObj: null };
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
}
