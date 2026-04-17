import { ASTActuator, FileMutation } from "../core/ASTActuator.js";
import { ASTHealer } from "../core/ASTHealer.js";
import { LearningLog } from "./LearningLog.js";
import * as fs from "fs";

export interface MutationCandidate {
    id: string;      
    mutations: FileMutation[]; 
}

/**
 * Darwinian Evolver (Người Quản Lý Đột Biến) - V7 MULTI-FILE ARCHITECTURE
 */
export class DarwinianEvolver {
    private actuator: ASTActuator;
    private healer: ASTHealer;
    private learningLog: LearningLog;

    constructor(workspace: string, learningLog: LearningLog) {
        this.actuator = new ASTActuator(workspace);
        this.healer = new ASTHealer();
        this.learningLog = learningLog;
    }

    /**
     * Vòng lặp Evaluator - GEPA
     */
    public async evaluateBatchPopulation(
        epicenterFileName: string,
        population: MutationCandidate[] 
    ): Promise<{ 
        bestCandidateId: string | null; 
        bestSandboxRoot: string | null;
        asiFeedbackReport: string 
    }> {
        const asiReports: Record<string, string> = {};
        let successCandidates: { id: string, path: string }[] = [];
        let createdSandboxRoots: string[] = [];

        console.log(`\n[DarwinianEvolver] Bắt đầu duyệt ${population.length} ứng viên qua Phẫu thuật AST Đa Tệp...`);
        for (const candidate of population) {
            console.log(`\n--- Phân tích Ứng viên: [${candidate.id}] ---`);
            const mutateResult = await this.actuator.actuateCandidateBatch(
                candidate.id, 
                candidate.mutations
            );

            if (!mutateResult.success) {
                console.log(`🔴 [Cand: ${candidate.id}] Thất bại tại ASTActuator: ${mutateResult.asi}`);
                asiReports[candidate.id] = mutateResult.asi || "Lỗi phẫu thuật không xác định";
                await this.learningLog.recordAttempt(epicenterFileName, `Mutate Batch (${candidate.id})`, asiReports[candidate.id], false);
                continue;
            }

            console.log(`🟢 [Cand: ${candidate.id}] Phẫu thuật thành công. Bắt đầu tự chữa lành chéo (Healer)...`);
            const sandboxRoot = mutateResult.sandboxRoot!;
            createdSandboxRoots.push(sandboxRoot);
            await this.healer.autoHealImportsOnSandbox(sandboxRoot);
            
            console.log(`[Cand: ${candidate.id}] Đang thu thập Báo cáo TypeScript (PreEmitDiagnostics) cho toàn Sandbox...`);
            const asiDiagnostic = this.healer.getASIFromPreEmitDiagnosticsOnSandbox(sandboxRoot);
            if (asiDiagnostic.length > 0) {
                 console.log(`🔴 [Cand: ${candidate.id}] Phát hiện Diagnostics lỗi TypeScript:\n${asiDiagnostic}`);
                 asiReports[candidate.id] = asiDiagnostic;
                 await this.learningLog.recordAttempt(epicenterFileName, `Verify Batch (${candidate.id})`, asiDiagnostic, false);
            } else {
                 console.log(`🟢 [Cand: ${candidate.id}] Hoàn hảo! Không có bất kỳ lỗi Compile nào.`);
                 asiReports[candidate.id] = "✅ PASS AST VERIFICATION (No compile errors)";
                 successCandidates.push({ id: candidate.id, path: sandboxRoot });
                 await this.learningLog.recordAttempt(epicenterFileName, `Survived Batch (${candidate.id})`, "Pareto Validated", true);
            }
        }

        let bestCandidateId = null;
        let bestSandboxRoot = null;
        
        if (successCandidates.length > 0) {
             const selected = successCandidates[0];
             bestCandidateId = selected.id;
             bestSandboxRoot = selected.path;
        }

        let combinedASI = "";
        for (const [candId, report] of Object.entries(asiReports)) {
             combinedASI += `\n>> [Kênh ứng viên ${candId}]:\n${report}\n`;
        }
        
        // DỌN DẸP RÁC
        for (const spath of createdSandboxRoots) {
             if (spath && spath !== bestSandboxRoot) {
                 if (fs.existsSync(spath)) fs.rmSync(spath, { recursive: true, force: true });
             }
        }

        return { bestCandidateId, bestSandboxRoot, asiFeedbackReport: combinedASI.trim() };
    }
}
