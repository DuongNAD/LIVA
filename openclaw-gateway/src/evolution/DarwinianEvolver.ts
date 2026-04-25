import { ASTActuator, FileMutation } from "../core/ASTActuator.js";
import { ASTHealer } from "../core/ASTHealer.js";
import { LearningLog } from "./LearningLog.js";
import * as fs from "node:fs";

export interface MutationCandidate {
    id: string;      
    mutations: FileMutation[]; 
}

export interface FitnessScore {
    candidateId: string;
    compilePasses: boolean;
    diagnosticCount: number;      // Lower is better
    mutationCount: number;        // Fewer mutations = less invasive
    totalCodeSize: number;        // Size of all mutation code
    fitnessValue: number;         // Computed aggregate fitness (higher is better)
    sandboxPath: string;
}

/**
 * Darwinian Evolver V7: Multi-Objective Pareto Selection
 * ======================================================
 * Evaluates the population with fitness scoring across multiple dimensions:
 * - Compile pass (must be true)
 * - Diagnostic count (lower is better)
 * - Mutation count (fewer = less invasive)
 * - Code size change (smaller diff preferred)
 * 
 * The best candidate is selected by aggregate fitness, not first-wins.
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
     * Evaluate batch population with multi-objective fitness scoring.
     */
    public async evaluateBatchPopulation(
        epicenterFileName: string,
        population: MutationCandidate[] 
    ): Promise<{ 
        bestCandidateId: string | null; 
        bestSandboxRoot: string | null;
        asiFeedbackReport: string;
        fitnessScores: FitnessScore[];
    }> {
        const asiReports: Record<string, string> = {};
        const fitnessScores: FitnessScore[] = [];
        let createdSandboxRoots: string[] = [];

        console.log(`\n[DarwinianEvolver] Evaluating ${population.length} candidates via Multi-File AST Surgery...`);
        
        for (const candidate of population) {
            console.log(`\n--- Analyzing Candidate: [${candidate.id}] ---`);
            const mutateResult = await this.actuator.actuateCandidateBatch(
                candidate.id, 
                candidate.mutations
            );

            if (!mutateResult.success) {
                console.log(`🔴 [Cand: ${candidate.id}] ASTActuator failed: ${mutateResult.asi}`);
                asiReports[candidate.id] = mutateResult.asi || "Unknown surgery error";
                await this.learningLog.recordAttempt(epicenterFileName, `Mutate Batch (${candidate.id})`, asiReports[candidate.id], false);
                
                // Record failed fitness
                fitnessScores.push({
                    candidateId: candidate.id,
                    compilePasses: false,
                    diagnosticCount: 999,
                    mutationCount: candidate.mutations.length,
                    totalCodeSize: candidate.mutations.reduce((sum, m) => sum + m.code.length, 0),
                    fitnessValue: -1,
                    sandboxPath: "",
                });
                continue;
            }

            console.log(`🟢 [Cand: ${candidate.id}] Surgery succeeded. Running cross-heal (Healer)...`);
            const sandboxRoot = mutateResult.sandboxRoot!;
            createdSandboxRoots.push(sandboxRoot);
            await this.healer.autoHealImportsOnSandbox(sandboxRoot);
            
            console.log(`[Cand: ${candidate.id}] Collecting TypeScript PreEmitDiagnostics...`);
            const asiDiagnostic = this.healer.getASIFromPreEmitDiagnosticsOnSandbox(sandboxRoot);
            const diagnosticCount = this.countDiagnostics(asiDiagnostic);
            
            if (diagnosticCount > 0) {
                 console.log(`🔴 [Cand: ${candidate.id}] Found ${diagnosticCount} TypeScript errors`);
                 asiReports[candidate.id] = asiDiagnostic;
                 await this.learningLog.recordAttempt(epicenterFileName, `Verify Batch (${candidate.id})`, asiDiagnostic, false);
                 
                 fitnessScores.push({
                     candidateId: candidate.id,
                     compilePasses: false,
                     diagnosticCount,
                     mutationCount: candidate.mutations.length,
                     totalCodeSize: candidate.mutations.reduce((sum, m) => sum + m.code.length, 0),
                     fitnessValue: -diagnosticCount, // Negative = failed
                     sandboxPath: sandboxRoot,
                 });
            } else {
                 console.log(`🟢 [Cand: ${candidate.id}] Zero compile errors!`);
                 asiReports[candidate.id] = "✅ PASS AST VERIFICATION (No compile errors)";
                 
                 // Compute positive fitness (higher is better)
                 const mutationCount = candidate.mutations.length;
                 const codeSize = candidate.mutations.reduce((sum, m) => sum + m.code.length, 0);
                 // Prefer fewer mutations and smaller code size (least invasive)
                 const fitnessValue = 100 - (mutationCount * 5) - (codeSize / 1000);
                 
                 fitnessScores.push({
                     candidateId: candidate.id,
                     compilePasses: true,
                     diagnosticCount: 0,
                     mutationCount,
                     totalCodeSize: codeSize,
                     fitnessValue,
                     sandboxPath: sandboxRoot,
                 });
                 
                 await this.learningLog.recordAttempt(epicenterFileName, `Survived Batch (${candidate.id})`, "Pareto Validated", true);
            }
        }

        // ========================================
        // PARETO SELECTION: Pick the best candidate
        // ========================================
        const survivors = fitnessScores
            .filter(f => f.compilePasses)
            .sort((a, b) => b.fitnessValue - a.fitnessValue);

        let bestCandidateId = null;
        let bestSandboxRoot = null;
        
        if (survivors.length > 0) {
             const selected = survivors[0];
             bestCandidateId = selected.candidateId;
             bestSandboxRoot = selected.sandboxPath;
             
             console.log(`\n🏆 [Pareto Selection] Winner: ${bestCandidateId} (fitness: ${selected.fitnessValue.toFixed(1)})`);
             if (survivors.length > 1) {
                 console.log(`   Runner-up: ${survivors[1].candidateId} (fitness: ${survivors[1].fitnessValue.toFixed(1)})`);
             }
        }

        // Build combined ASI report
        let combinedASI = "";
        for (const [candId, report] of Object.entries(asiReports)) {
             combinedASI += `\n>> [Candidate ${candId}]:\n${report}\n`;
        }
        
        // CLEANUP: Remove sandbox dirs that weren't selected
        for (const spath of createdSandboxRoots) {
             if (spath && spath !== bestSandboxRoot) {
                 if (fs.existsSync(spath)) fs.rmSync(spath, { recursive: true, force: true });
             }
        }

        return { bestCandidateId, bestSandboxRoot, asiFeedbackReport: combinedASI.trim(), fitnessScores };
    }

    /**
     * Count the number of diagnostics in an ASI report.
     */
    private countDiagnostics(asiReport: string): number {
        if (!asiReport || asiReport.length === 0) return 0;
        // Count lines that match the diagnostic pattern "- [File: ...]"
        const matches = asiReport.match(/- \[File:/g);
        return matches ? matches.length : (asiReport.length > 0 ? 1 : 0);
    }
}
