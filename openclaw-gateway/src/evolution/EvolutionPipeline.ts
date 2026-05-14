import * as path from "node:path";
import * as fsSync from "node:fs";
import * as os from "node:os";
import { exec } from "node:child_process";
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
import { StructuredMemory } from "../memory/StructuredMemory";
import { EmbeddingService } from "../services/EmbeddingService";

/** v25 Hardening: Evolution Guardrails — Prevent Singularity Fork-Bomb */
const MAX_EPOCHS = 10;
const MAX_CONSECUTIVE_FAILURES = 3;
const COOLDOWN_ESCALATION_MS = [60_000, 300_000, 900_000]; // 1m → 5m → 15m

/** v25 Hardening: Max heap in MB before halting evolution */
const MAX_HEAP_MB = 2048;

/** v25 Hardening: Max CPU load average (1-min) ratio before halting */
const MAX_CPU_LOAD_RATIO = 0.80;

interface EpochResult {
    success: boolean;
    hypothesisIdea?: string;
    errorMsg?: string;
}

export class EvolutionPipeline {
    async startInfiniteSingularity() {
        evoLogger.info(`================================================================`);
        evoLogger.info(` 🚀 [LIVA SINGULARITY DAEMON] - CHU TRÌNH TỰ TIẾN HÓA KÍCH HOẠT`);
        evoLogger.info(`   MAX_EPOCHS=${MAX_EPOCHS} | CIRCUIT_BREAKER=${MAX_CONSECUTIVE_FAILURES} failures`);
        evoLogger.info(`================================================================`);
        
        let iteration = 1;
        let consecutiveFailures = 0;
        const attemptedHypotheses = new Set<string>();

        while (iteration <= MAX_EPOCHS) {
            evoLogger.info(`=== [SINGULARITY] CHU KỲ TIẾN HÓA #${iteration}/${MAX_EPOCHS} ===`);

            // v25 Hardware-Aware: Check OS vitals before each epoch
            const vitals = await this.#checkHardwareBudget();
            if (!vitals.canProceed) {
                evoLogger.warn(`[Singularity] ⚠️ EPOCH SKIPPED: ${vitals.reason}`);
                // Wait longer and retry — don't count as failure
                await sleep(COOLDOWN_ESCALATION_MS[2]); // 15 min cooldown
                iteration++;
                continue;
            }

            const result = await this.runEpoch(iteration, attemptedHypotheses);

            // Track hypothesis to prevent infinite retry of the same fix
            if (result.hypothesisIdea) {
                attemptedHypotheses.add(result.hypothesisIdea);
            }

            // Circuit breaker: halt if too many consecutive failures
            if (!result.success) {
                consecutiveFailures++;
                evoLogger.warn(`[Singularity] ⚠️ Failure #${consecutiveFailures}/${MAX_CONSECUTIVE_FAILURES}: ${result.errorMsg}`);
                if (consecutiveFailures >= MAX_CONSECUTIVE_FAILURES) {
                    evoLogger.error(`[Singularity] 🛑 HALTED: ${consecutiveFailures} consecutive failures — circuit breaker engaged.`);
                    try {
                        await notifyZalo(`🛑 [LIVA SINGULARITY HALTED]\nCircuit breaker: ${consecutiveFailures} consecutive failures.\nLast error: ${result.errorMsg}\n\nDaemon has stopped. Manual restart required.`);
                    } catch { /* fire-and-forget */ }
                    break;
                }
            } else {
                consecutiveFailures = 0;
            }

            iteration++;

            if (global.gc) {
                global.gc();
                evoLogger.info(`[LIVA GC] Đã vắt cạn bộ nhớ rác của tiến trình Hệ thống.`);
            }

            // Escalating cooldown: healthy = 1m, failing = 5m/15m
            const cooldownIdx = Math.min(consecutiveFailures, COOLDOWN_ESCALATION_MS.length - 1);
            const cooldown = COOLDOWN_ESCALATION_MS[cooldownIdx];
            evoLogger.info(`[SINGULARITY] Cooldown: ${cooldown / 1000}s before next epoch.`);
            await sleep(cooldown);
        }

        if (iteration > MAX_EPOCHS) {
            evoLogger.info(`[Singularity] ✅ Completed all ${MAX_EPOCHS} epochs. Daemon exiting gracefully.`);
        }
    }

    /**
     * v25 Hardware-Aware Evolution — Check OS vitals before each epoch.
     * Blocks evolution when:
     *   1. Machine is on battery (not plugged in)
     *   2. CPU load > 80% (user is doing heavy work)
     *   3. Heap memory > 2GB (memory pressure)
     */
    async #checkHardwareBudget(): Promise<{ canProceed: boolean; reason: string }> {
        // Check 1: Heap memory
        const heapMB = Math.round(process.memoryUsage().heapUsed / (1024 * 1024));
        if (heapMB > MAX_HEAP_MB) {
            return { canProceed: false, reason: `Heap memory ${heapMB}MB > ${MAX_HEAP_MB}MB limit` };
        }

        // Check 2: CPU load (1-min average / number of CPUs)
        const cpuCount = os.cpus().length || 1;
        const loadAvg1m = os.loadavg()[0];
        const loadRatio = loadAvg1m / cpuCount;
        if (loadRatio > MAX_CPU_LOAD_RATIO) {
            return { canProceed: false, reason: `CPU load ${(loadRatio * 100).toFixed(0)}% > ${MAX_CPU_LOAD_RATIO * 100}% limit` };
        }

        // Check 3: Battery status (Windows only — skip on other platforms)
        if (process.platform === "win32") {
            const isOnBattery = await this.#isOnBattery();
            if (isOnBattery) {
                return { canProceed: false, reason: "Machine is on battery power — evolution deferred to preserve energy" };
            }
        }

        evoLogger.info(`[HW Budget] ✅ OK — Heap: ${heapMB}MB, CPU: ${(loadRatio * 100).toFixed(0)}%, Power: AC`);
        return { canProceed: true, reason: "" };
    }

    /**
     * Check if machine is running on battery (Windows PowerShell).
     * Returns true if on battery, false if plugged in or unknown.
     */
    #isOnBattery(): Promise<boolean> {
        return new Promise((resolve) => {
            exec(
                "powershell -Command \"(Get-WmiObject Win32_Battery).BatteryStatus\"",
                { timeout: 3000, windowsHide: true },
                (err, stdout) => {
                    if (err || !stdout) return resolve(false); // Assume plugged in if unknown
                    const status = parseInt(stdout.trim(), 10);
                    // BatteryStatus: 1 = Discharging (on battery), 2 = AC Power
                    resolve(status === 1);
                }
            );
        });
    }

    private async runEpoch(iteration: number, attemptedHypotheses: Set<string>): Promise<EpochResult> {
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
            if (!ctx.hasBugs) return { success: true };
            
            await KnowledgeDistiller.run(ctx);
            
            await HypothesisGenerator.run(ctx);

            // v25 Hardening: Hypothesis Deduplication — block infinite retry of the same fix
            if (ctx.hypothesis?.idea && attemptedHypotheses.has(ctx.hypothesis.idea)) {
                evoLogger.warn(`[Singularity] 🔁 Duplicate hypothesis: "${ctx.hypothesis.idea}" — skipping to prevent infinite retry loop.`);
                return { success: false, hypothesisIdea: ctx.hypothesis.idea, errorMsg: "Duplicate hypothesis blocked" };
            }
            
            await RollbackManager.backup(ctx); // 🛡️ Save state
            
            await ASTMutator.apply(ctx);
            
            const isValid = await SandboxValidator.verify(ctx);
            if (!isValid) {
                throw new Error(`Compilation Failed: ${ctx.errorMsg}`);
            }

            // Ghi nhật ký học tập thành công
            const memory = await StructuredMemory.create("liva_core");
            const emb = EmbeddingService.getInstance();
            const successVec = await emb.embed(`SUCCESS: ${ctx.hypothesis?.idea}`);
            memory.upsertVector({
                vecId: `evo_success_${Date.now()}`,
                type: 'SUCCESS',
                content: `[SUCCESS] ${ctx.hypothesis?.idea}`,
                vector: successVec,
                domain: ctx.hypothesis?.targetFilePath || 'SYSTEM_CORE',
            });
            await RollbackManager.cleanup(ctx);

            evoLogger.info(`[Singularity] Vòng lặp tiến hóa thành công rực rỡ!`);
            return { success: true, hypothesisIdea: ctx.hypothesis?.idea };

        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            crashErrorMsg = errMsg;
            evoLogger.error({ err }, `[Singularity] Lỗi Pipeline, đang tiến hành khôi phục (Rollback)`);
            await RollbackManager.restore(ctx); // 🛡️ Revert on failure
            
            const memory = await StructuredMemory.create("liva_core");
            const emb = EmbeddingService.getInstance();
            const failVec = await emb.embed(`DEAD-END: ${ctx.hypothesis?.idea} ${crashErrorMsg}`);
            memory.upsertVector({
                vecId: `evo_deadend_${Date.now()}`,
                type: 'DEAD-END',
                content: `[FAILED] ${ctx.hypothesis?.idea} -> Lỗi: ${crashErrorMsg}`,
                vector: failVec,
                domain: ctx.hypothesis?.targetFilePath || 'SYSTEM_CORE',
            });

            return { success: false, hypothesisIdea: ctx.hypothesis?.idea, errorMsg: crashErrorMsg };
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
