import { ASTActuator } from "../core/ASTActuator.js";
import { ASTHealer } from "../core/ASTHealer.js";
import { LearningLog } from "./LearningLog.js";
import * as fs from "fs";

export interface MutationCandidate {
    id: string;      // ID nhận diện (vd: cand_A)
    code: string;    // Mã nguồn đột biến
}

/**
 * Darwinian Evolver (Người Quản Lý Đột Biến)
 * GEPA Engine: Tạo nhánh đột biến song song, thanh lọc rác AST, lưu Learning Log
 * Định tuyến chọn ra Biến thể (Candidate) có chỉ số Pareto tối ưu nhất.
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
     * Tiếp nhận mảng Mẫu đột biến (Population), kiểm tra AST song song, và thanh lọc Pareto.
     */
    public async evaluateBatchPopulation(
        targetFileName: string,
        className: string,
        methodName: string,
        population: MutationCandidate[] // Thợ Code Coder trả về một mảng thay vì 1 JSON tĩnh
    ): Promise<{ 
        bestCandidateId: string | null; 
        bestShadowPath: string | null;
        asiFeedbackReport: string 
    }> {
        const asiReports: Record<string, string> = {};
        let successCandidates: { id: string, path: string }[] = [];
        let createdShadowPaths: string[] = [];

        // 1. Phẫu thuật độc lập qua Shadow Workspace (Ngăn Race Conditions)
        console.log(`\n[DarwinianEvolver] Bắt đầu duyệt ${population.length} ứng viên qua Phẫu thuật AST...`);
        for (const candidate of population) {
            console.log(`\n--- Phân tích Ứng viên: [${candidate.id}] ---`);
            const mutateResult = await this.actuator.replaceMethod(
                targetFileName, 
                candidate.id, 
                className, 
                methodName, 
                candidate.code
            );

            if (!mutateResult.success) {
                console.log(`🔴 [Cand: ${candidate.id}] Thất bại tại ASTActuator: ${mutateResult.asi}`);
                asiReports[candidate.id] = mutateResult.asi || "Lỗi phẫu thuật không xác định";
                await this.learningLog.recordAttempt(targetFileName, `Mutate ${methodName} (${candidate.id})`, asiReports[candidate.id], false);
                continue;
            }

            console.log(`🟢 [Cand: ${candidate.id}] Phẫu thuật thành công. Bắt đầu tự chữa lành (Healer)...`);
            // 2. Tự chữa lành Import & Lấy định kiến AST
            const shadowPath = mutateResult.shadowPath!;
            createdShadowPaths.push(shadowPath);
            await this.healer.autoHealImports(shadowPath);
            
            console.log(`[Cand: ${candidate.id}] Đang thu thập Báo cáo TypeScript (PreEmitDiagnostics)...`);
            const asiDiagnostic = this.healer.getASIFromPreEmitDiagnostics(shadowPath);
            if (asiDiagnostic.length > 0) {
                 console.log(`🔴 [Cand: ${candidate.id}] Phát hiện Diagnostics lỗi TypeScript:\n${asiDiagnostic}`);
                 asiReports[candidate.id] = asiDiagnostic;
                 await this.learningLog.recordAttempt(targetFileName, `Verify ${methodName} (${candidate.id})`, asiDiagnostic, false);
                 // Loại ngay lập tức biến thể này khỏi nhóm sinh tồn
            } else {
                 console.log(`🟢 [Cand: ${candidate.id}] Hoàn hảo! Không có bất kỳ lỗi Compile nào.`);
                 // Vượt qua Khảo sát AST Hẹp
                 asiReports[candidate.id] = "✅ PASS AST VERIFICATION (No compile errors)";
                 successCandidates.push({ id: candidate.id, path: shadowPath });
                 await this.learningLog.recordAttempt(targetFileName, `Survived ${methodName} (${candidate.id})`, "Pareto Validated", true);
            }
        }

        // 3. Phân quyền Pareto Selection (Tối ưu hóa đa biến độ trễ/tokens)
        // Hiện tại: Nếu nhiều mẫu sống, chọn mẫu sống đầu tiên. 
        // Về sau (Phase 3): Đẩy tất cả mảng successCandidates này vào MicroVM thi đấu Dynamic Traffic.
        let bestCandidateId = null;
        let bestShadowPath = null;
        
        if (successCandidates.length > 0) {
             const selected = successCandidates[0];
             bestCandidateId = selected.id;
             bestShadowPath = selected.path;
        }

        // 4. Tổng hợp ASI Report của toàn bộ quần thể để báo về cho Coder nếu tất cả thất bại
        let combinedASI = "";
        for (const [candId, report] of Object.entries(asiReports)) {
             combinedASI += `\n>> [Kênh ứng viên ${candId}]:\n${report}\n`;
        }
        
        // 5. DỌN DẸP RÁC: Xóa bỏ mọi File Shadow của các ứng viên thua cuộc (hoặc lỗi)
        for (const spath of createdShadowPaths) {
             if (spath && spath !== bestShadowPath) {
                 if (fs.existsSync(spath)) fs.unlinkSync(spath);
             }
        }

        return { bestCandidateId, bestShadowPath, asiFeedbackReport: combinedASI.trim() };
    }
}
