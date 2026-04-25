import { exec } from "node:child_process";
import { promisify } from "util";
import { evoLogger } from "./EvolutionLogger";
import { EvolutionContext } from "./types";

const execAsync = promisify(exec);

export class SandboxValidator {
    static async verify(ctx: EvolutionContext): Promise<boolean> {
        evoLogger.info(`[SandboxValidator] Đang chạy kiểm định an toàn toàn cục...`);
        
        const commands = ["npx tsc --noEmit"];
        if (ctx.hypothesis?.testCommand && ctx.hypothesis.testCommand !== "npx tsc --noEmit") {
            commands.push(ctx.hypothesis.testCommand);
        }

        for (const cmd of commands) {
            try {
                evoLogger.info(`[SandboxValidator] Đang chạy lệnh: ${cmd}`);
                await execAsync(cmd, { cwd: process.cwd() });
            } catch (error: any) {
                evoLogger.error({ err: error.stdout || error.message }, `[SandboxValidator] Lỗi tại lệnh ${cmd}! Phát hiện mã hỏng.`);
                ctx.compilationPassed = false;
                ctx.errorMsg = `Lệnh ${cmd} thất bại:\n${error.stdout || error.message}`;
                return false;
            }
        }

        evoLogger.info(`[SandboxValidator] Toàn bộ kiểm định an toàn thành công! Không phát hiện lỗi.`);
        ctx.compilationPassed = true;
        return true;
    }
}
