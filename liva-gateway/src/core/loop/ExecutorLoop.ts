import { logger } from "../../utils/logger";
import { ToolExecutionEngine } from "./ToolExecutionEngine";
export class ExecutorLoop {
    public async executeBlueprint(blueprintSteps: Record<string, unknown>[], _userText: string, engine: ToolExecutionEngine): Promise<string> {
        logger.info(`⚡ [ExecutorLoop] Bắt đầu thực thi Blueprint gồm ${blueprintSteps.length} bước bằng 4B Draft Model...`);
        let finalResult = "";

        for (let i = 0; i < blueprintSteps.length; i++) {
            const step = blueprintSteps[i];
            logger.info(`[Executor] Bước ${i + 1}/${blueprintSteps.length}: Gọi Tool [${step.toolName}]...`);
            
            try {
                // Execute individual step
                const result = await engine.toolOrchestrator.executeWithReflection(step.toolName as string, step.arguments as Record<string, unknown>);
                finalResult += `\nBước ${i+1} (${step.toolName}): ${result.resultStr}`;
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[Executor] Lỗi khi thực thi Tool [${step.toolName}]: ${errMsg}`);
                finalResult += `\nBước ${i+1} (${step.toolName}): LỖI - ${errMsg}`;
            }
        }

        logger.info(`✅ [ExecutorLoop] Hoàn tất toàn bộ Blueprint! Báo cáo lại cho Planner...`);
        return finalResult;
    }
}
