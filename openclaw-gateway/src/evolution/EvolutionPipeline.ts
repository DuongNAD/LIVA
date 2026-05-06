import * as path from "node:path";
import * as fsSync from "node:fs";
import { evoLogger } from "./EvolutionLogger";
import { EngineManager, sleep } from "./EngineManager";
import { VulnerabilityScanner } from "./VulnerabilityScanner";
import { KnowledgeDistiller } from "./KnowledgeDistiller";
import { HypothesisGenerator } from "./HypothesisGenerator";
import { RollbackManager } from "./RollbackManager";
import { ASTMutator } from "./ASTMutator";
import { SandboxValidator } from "./SandboxValidator";
import { EvolutionContext } from "./types";
import { notifyZalo } from "../utils/ZaloNotifier";
import { LanceMemoryManager } from "../memory/LanceMemory";

export class EvolutionPipeline {
    async startInfiniteSingularity() {
        evoLogger.info(`================================================================`);
        evoLogger.info(` 🚀 [LIVA SINGULARITY DAEMON] - CHU TRÌNH TỰ TIẾN HÓA KÍCH HOẠT`);
        evoLogger.info(`================================================================`);
        
        let iteration = 1;
        while (true) {
            evoLogger.info(`=== [INFINITY LOOP] BẮT ĐẦU CHU KỲ TIẾN HÓA THỨ #${iteration} ===`);
            
            await this.runEpoch(iteration);
            
            evoLogger.info(`[INFINITY LOOP] Chu kỳ #${iteration} hoàn tất. AI đang hạ nhiệt GPU và cân bằng VRAM... (Chờ 60s)`);
            iteration++;
            
            if (global.gc) {
                global.gc();
                evoLogger.info(`[LIVA GC] Đã vắt cạn bộ nhớ rác của tiến trình Hệ thống.`);
            }

            await sleep(60000);
        }
    }

    private async runEpoch(iteration: number) {
        evoLogger.info(`[HOT-SWAP] TẠM NGƯNG HỆ THỐNG ZALO BOT. THU HỒI VRAM TỪ NÃO E4B...`);
        await EngineManager.killPortWindows(8000);
        await EngineManager.killPortWindows(8001);
        await EngineManager.waitForVRAMClear(2048, 30);

        const workspaceDir = path.join(process.cwd(), ".workspace");
        if (!fsSync.existsSync(workspaceDir)) fsSync.mkdirSync(workspaceDir, { recursive: true });

        let crashErrorMsg: string | null = null;
        
        const ctx: EvolutionContext = {
            iteration,
            hasBugs: false,
            projectSurfaceInfo: "",
            bottlenecks: "",
            pastExperiences: "",
            axioms: "",
            blacklistFiles: [],
            compilationPassed: false,
            workspaceDir
        };

        try {
            evoLogger.info(`[Hot-Swap] PHA 1: KHỞI ĐỘNG KỸ SƯ TRƯỞNG (PLANNER) - CỔNG 8001`);
            await EngineManager.checkPortAvailable(8001);
            await EngineManager.startEngineWindows("ai_engine.py", ["--role", "planner", "--port", "8001", "--n_ctx", "24576"]);
            
            evoLogger.info(`[HOT-SWAP] Đang đợi Kỹ sư Trưởng khởi động động cơ Uvicorn...`);
            const isPlannerAwake = await EngineManager.pingUvicorn(8001, 240);
            if (!isPlannerAwake) {
                throw new Error("Không thể đánh thức não Planner. Sụp đổ kiến trúc!");
            }
            evoLogger.info(`[HOT-SWAP] NÃO PLANNER (8001) ĐÃ SẴN SÀNG TOÀN VRAM!`);

            // --- DAG PIPELINE EXECUTION ---
            await VulnerabilityScanner.run(ctx);
            if (!ctx.hasBugs) return;
            
            await KnowledgeDistiller.run(ctx);
            
            await HypothesisGenerator.run(ctx);
            
            await RollbackManager.backup(ctx); // 🛡️ Save state
            
            await ASTMutator.apply(ctx);
            
            const isValid = await SandboxValidator.verify(ctx);
            if (!isValid) {
                throw new Error(`Compilation Failed: ${ctx.errorMsg}`);
            }

            // Ghi nhật ký học tập thành công
            const memory = new LanceMemoryManager();
            await memory.connect();
            await memory.addMemory("SUCCESS", `[SUCCESS] ${ctx.hypothesis?.idea}`, ctx.hypothesis?.targetFilePath || "SYSTEM_CORE");
            await RollbackManager.cleanup(ctx);

            evoLogger.info(`[Singularity] Vòng lặp tiến hóa thành công rực rỡ!`);

        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            crashErrorMsg = errMsg;
            evoLogger.error({ err }, `[Singularity] Lỗi Pipeline, đang tiến hành khôi phục (Rollback)`);
            await RollbackManager.restore(ctx); // 🛡️ Revert on failure
            
            const memory = new LanceMemoryManager();
            await memory.connect();
            await memory.addMemory("DEAD-END", `[FAILED] ${ctx.hypothesis?.idea} -> Lỗi: ${crashErrorMsg}`, ctx.hypothesis?.targetFilePath || "SYSTEM_CORE");
        } finally {
            evoLogger.info(`[HOT-SWAP] THU HỒI CÁC CỔNG AI TIẾN HÓA VÀ DỌN DẸP VRAM...`);
            await EngineManager.killPortWindows(8001);
            await EngineManager.waitForVRAMClear(2048, 30);
            
            evoLogger.info(`[HOT-SWAP] KHÔI PHỤC NÃO TRỰC BAN E4B VÀO CỔNG 8000. TIẾP TỤC DỊCH VỤ ZALO...`);
            await EngineManager.startEngineWindows("engine.py", []);

            if (crashErrorMsg) {
                evoLogger.info(`[HOT-SWAP] Đang phát tín hiệu SOS về Bộ chỉ huy thông qua Zalo...`);
                try {
                    await notifyZalo(`🚨 [LIVA SOS]\nVòng lặp Cải tiến Sinh tồn (Singularity) vừa gặp sự cố!\nNguyên nhân: ${crashErrorMsg}\n\nHệ thống đã tự động Rollback an toàn và khôi phục Zalo Router (8000). Sếp vui lòng kiểm tra Logs.`);
                } catch (e) { void e; }
            } else if (ctx.hypothesis) {
                try {
                    await notifyZalo(`✨ [LIVA UPDATE]\nĐã tiến hóa thành công hệ thống ở vòng lặp thứ ${iteration}!\nFile: ${ctx.hypothesis.targetFilePath}\nGoal: ${ctx.hypothesis.idea}`);
                } catch (e) { void e; }
            }

            evoLogger.info(`=== J.A.R.V.I.S ĐÃ TRỞ LẠI PHA TRỰC BAN (ROUTER E4B)! MỌI THỨ THEO QUỸ ĐẠO! ===`);
        }
    }
}
