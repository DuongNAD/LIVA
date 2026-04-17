import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { DarwinianEvolver } from "../evolution/DarwinianEvolver.js";
import { LearningLog } from "../evolution/LearningLog.js";
import { MicroVMDaemon } from "../sandbox/MicroVMDaemon.js";
import { BlueGreenRouter } from "../deployment/BlueGreenRouter.js";
import { QualityChecker } from "../evolution/QualityChecker.js";
import { extractAndValidate, PopulationSchema, type PopulationPayload } from "../evolution/StructuredExtractor.js";
import { fullResearch } from "../evolution/WebResearchAgent.js";

const CONFIG = {
    AI_BASE_URL: process.env.AI_BASE_URL || "http://127.0.0.1:8001/v1",
    AI_API_KEY: process.env.AI_API_KEY || "liva-ghost-coder",
    AI_MODEL: process.env.AI_MODEL || "expert",
    MAX_CYCLES: 3,
    ENABLE_QUALITY_CHECKER: process.env.ENABLE_QUALITY_CHECKER !== "false",
    ENABLE_WEB_RESEARCH: process.env.ENABLE_WEB_RESEARCH !== "false",
};

// Singleton: Initialize Evolution Memory (prevents reloading Vector DB)
const memLog = new LearningLog();
memLog.connect().catch(() => {});

export interface AgentArgs {
    goal: string;
    targetFilePath: string;
    testCommand?: string;
}

/**
 * LIVA EVOLUTION ENGINE V7 — DARWINIAN TRIAD ORCHESTRATOR
 * ========================================================
 * Pipeline: RAG Memory → Web Research → Darwinian Coder → AST Surgery 
 *         → Quality Review → Local Sandbox → Git Deploy
 * 
 * V7 Upgrades:
 * - Structured Output Enforcement (Zod schema validation)
 * - Web-Augmented Research (DuckDuckGo error/goal lookup)
 * - Cross-Cycle Learning (carry forward partial diffs + reviewer feedback)
 * - Smart Token Budget (keep JSDoc, strip inline noise)
 * - Progressive Temperature (0.6 → 0.4 → 0.2 across cycles)
 */
export const execute = async (args: AgentArgs): Promise<string> => {
    const workspace = process.cwd();
    const targetFile = path.isAbsolute(args.targetFilePath) 
        ? args.targetFilePath 
        : path.resolve(workspace, args.targetFilePath);
    
    if (!fs.existsSync(targetFile)) {
        return `🔴 Target file not found: ${targetFile}`;
    }

    // Core subsystems
    const evolver = new DarwinianEvolver(workspace, memLog);
    const vmDaemon = new MicroVMDaemon();
    const bgRouter = new BlueGreenRouter(workspace);

    let report = `\n# LIVA V7 EVOLUTION ENGINE: DARWINIAN LOOP INITIATED\n`;
    console.log(report);

    const aiClient = new OpenAI({ baseURL: CONFIG.AI_BASE_URL, apiKey: CONFIG.AI_API_KEY });
    let currentCycle = 1;

    // Cross-Cycle State: carried forward between iterations
    let previousCycleErrors = "";
    let previousReviewerFeedback = "";
    let previousBestDiff = "";

    while (currentCycle <= CONFIG.MAX_CYCLES) {
        report += `\n>> [Cycle #${currentCycle}/${CONFIG.MAX_CYCLES}] Analyzing target...\n`;
        console.log(`\n========== DARWINIAN CYCLE #${currentCycle} ==========`);

        // Progressive temperature: decreases each cycle for more focused output
        const cycleTemp = Math.max(0.2, 0.6 - (currentCycle - 1) * 0.2);

        // ==========================================
        // PHASE 1: RAG MEMORY RETRIEVAL
        // ==========================================
        console.log(">> [Phase 1] Retrieving axioms from Vector Database...");
        const axioms = await memLog.getRelevantAxioms(targetFile, args.goal);
        
        let safeAxioms = axioms;
        if (safeAxioms.length > 2000) {
            safeAxioms = safeAxioms.substring(0, 2000) + "\n... (Truncated to save tokens)";
        }
        
        report += `[Phase 1] RAG Axioms applied to constrain hallucination zone.\n`;

        // ==========================================
        // PHASE 1.5: WEB RESEARCH (NEW)
        // ==========================================
        let webContext = "";
        if (CONFIG.ENABLE_WEB_RESEARCH) {
            console.log(">> [Phase 1.5] Web Research — searching for solutions...");
            const research = await fullResearch(args.goal, previousCycleErrors || undefined);
            
            if (research.goalInsights) {
                webContext += `\n<web_research>\n  <goal_insights>\n    ${research.goalInsights}\n  </goal_insights>\n</web_research>\n`;
                report += `[Phase 1.5] 🌐 Web research found ${research.totalResults} relevant results.\n`;
            }
            if (research.errorFixes) {
                webContext += research.errorFixes;
                report += `[Phase 1.5] 🔍 Found error fixes from previous cycle failures.\n`;
            }
        }

        // ==========================================
        // PHASE 2: DARWINIAN AST-CODER
        // ==========================================
        console.log(">> [Phase 2] Darwinian Coder generating population...");
        const rawCode = fs.readFileSync(targetFile, "utf8");
        
        // Smart Token Budget: keep JSDoc but strip inline comments and blank lines
        const originalCode = rawCode
            .replace(/(?<!:)\/\/(?!\/).*$/gm, '')     // Strip inline comments (but not URLs)
            .replace(/^\s*[\r\n]/gm, '');              // Strip blank lines
        
        // Build cross-cycle context
        let crossCycleContext = "";
        if (previousReviewerFeedback && currentCycle > 1) {
            crossCycleContext += `\n<previous_cycle_feedback>\n  The previous cycle (${currentCycle - 1}) was REJECTED by the Quality Reviewer:\n  "${previousReviewerFeedback}"\n  You MUST address this feedback in your new mutations.\n</previous_cycle_feedback>\n`;
        }
        if (previousCycleErrors && currentCycle > 1) {
            crossCycleContext += `\n<previous_cycle_errors>\n  ${previousCycleErrors.slice(0, 1500)}\n</previous_cycle_errors>\n`;
        }
        
        const coderPrompt = `
You are the Darwinian Coder (LIVA V7).
Your job is to generate a POPULATION of multiple code variations to safely achieve the Goal. You are now in a Multi-File Sandbox. You can both modify existing files and create new files.

Goal: ${args.goal}

<axioms>
${safeAxioms}
</axioms>
${webContext}
${crossCycleContext}

Original Target Epicenter Source Code (${targetFile}):
\`\`\`typescript
${originalCode}
\`\`\`

REQUIREMENTS:
1. DO NOT return standard patches. Return ONLY a valid JSON object (no markdown fences, no explanation).
2. Generate 2 different "candidates" (Candidate A and B) for the population.
3. For each candidate, you can perform MULTIPLE mutations (actions). An action can be 'modify' (modify a specific method, or a full class, or a FULL FILE if className and methodName are omitted) or 'create' (create a brand new file).
4. GUARDRAILS: You CANNOT touch files outside of 'src/'. You CANNOT modify 'src/skills/AIScientist.ts'. Max 3 'create' actions and 5 'modify' actions per candidate.
5. TYPESCRIPT RULE: Do NOT use the 'private' accessibility modifier with a '#' identifier (e.g. 'private #myVar' is forbidden, use '#myVar' or 'private myVar' only).
6. EXTREMELY IMPORTANT: The 'code' property must be perfectly escaped for JSON (use \\n for newlines, escape " as \\"). If you provide methodName, the 'code' must contain the FULL METHOD DECLARATION (including public/private keywords, parameters, and the body block). DO NOT use placeholders inside the code.
7. AST MUTATION RULE: If you use "modify" on a class without specifying methodName (replacing the whole class), you MUST include ALL existing class property/field declarations (especially private '#' fields) at the top of your code. Do NOT drop them.

EXPECTED JSON SCHEMA:
{
  "population": [
    { 
      "id": "cand_A",
      "mutations": [
        { "type": "modify", "filePath": "src/core/AgentLoop.ts", "className": "AgentLoop", "methodName": "dispatch", "code": "public dispatch(...) { ... }" },
        { "type": "create", "filePath": "src/events/MessageBus.ts", "code": "export class Bus {}" }
      ]
    },
    { "id": "cand_B", "mutations": [...] }
  ]
}
        `.trim();

        let populationRes: PopulationPayload | null = null;
        let rawTextContent = "";
        
        try {
            const streamRes = await aiClient.chat.completions.create({
                model: CONFIG.AI_MODEL,
                messages: [{ role: "user", content: coderPrompt }],
                temperature: cycleTemp,
                max_tokens: 16380,
                response_format: { type: "json_object" },
            }, { timeout: 1800000 });

            rawTextContent = streamRes.choices[0]?.message?.content || "";
            
            // Log thinking blocks for debug (then strip)
            const thinkMatch = rawTextContent.match(/<think>([\s\S]*?)<\/think>/i);
            if (thinkMatch) {
                console.log(`\n[Coder Internal Reasoning]:\n${thinkMatch[1].trim().slice(0, 500)}`);
            }

            // Structured Extraction + Zod Validation
            const extraction = extractAndValidate(rawTextContent, PopulationSchema);
            
            if (!extraction.success) {
                console.error(`\n[AIScientist] 🔴 Structured extraction FAILED!`);
                extraction.errors.forEach(e => console.error(`  ${e}`));
                console.error(`--- RAW OUTPUT (first 500 chars) ---\n${rawTextContent.slice(0, 500)}`);
                
                previousCycleErrors = extraction.errors.join("\n");
                report += `[Phase 2] 🔴 Coder output failed Zod validation. Method tried: ${extraction.method}\n`;
                currentCycle++; continue;
            }

            populationRes = extraction.data;
            console.log(`✅ [Phase 2] Population extracted via ${extraction.method}: ${populationRes!.population.length} candidates`);
            
        } catch (error: any) {
             const errMsg = error.message || "";
             if (errMsg.includes("maximum context length") || errMsg.includes("tokens")) {
                 console.log(`[Coder Fatal] TOKEN OVERFLOW: ${errMsg}`);
                 report += `[Phase 2] 🔴 Context OOM: Prompt too large for n_ctx!\n`;
             } else {
                 console.log(`[Coder Fatal] API/JSON error: ${errMsg}\n>>> RAW:\n${rawTextContent.slice(0, 500)}\n`);
                 report += `[Phase 2] 🔴 Coder hallucinated invalid output. (${errMsg})\n`;
             }
             previousCycleErrors = errMsg;
             currentCycle++; continue;
        }

        if (!populationRes || !populationRes.population || populationRes.population.length === 0) {
            console.log(`Population empty — evolution stalled.`);
            currentCycle++; continue;
        }

        // ==========================================
        // PHASE 3: DARWINIAN AST SURGERY + PARETO SELECTOR
        // ==========================================
        console.log(">> [Phase 3] AST Healer & Pareto Selection...");
        const gePaResult = await evolver.evaluateBatchPopulation(
            targetFile,
            populationRes.population
        );

        if (gePaResult.bestCandidateId && gePaResult.bestSandboxRoot) {
            console.log(`🟢 [Pareto Selector] Survivor: ${gePaResult.bestCandidateId}`);
            report += `[Phase 3] 🟢 Candidate ${gePaResult.bestCandidateId} passed AST verification.\n`;
            
            // ==========================================
            // PHASE 3.5: SENIOR AI CODE REVIEWER
            // ==========================================
            if (CONFIG.ENABLE_QUALITY_CHECKER) {
                 console.log(`>> [Phase 3.5] Senior AI Reviewer evaluating logic...`);
                 const reviewer = new QualityChecker(CONFIG.AI_BASE_URL, CONFIG.AI_API_KEY, CONFIG.AI_MODEL);
                 const qcResult = await reviewer.evaluateCodeQuality(args.goal, gePaResult.bestSandboxRoot);
                 
                 if (!qcResult.pass) {
                     console.log(`🔴 [Quality Reviewer] Rejected: ${qcResult.feedback}`);
                     report += `[Phase 3.5] 🔴 Reviewer rejected (Semantic Mismatch): ${qcResult.feedback}\n`;
                     
                     // Cross-Cycle Learning: carry reviewer feedback to next iteration
                     previousReviewerFeedback = qcResult.feedback;
                     await memLog.recordAttempt(targetFile, `Quality Review (${gePaResult.bestCandidateId})`, qcResult.feedback, false);
                     
                     if (fs.existsSync(gePaResult.bestSandboxRoot)) fs.rmSync(gePaResult.bestSandboxRoot, { recursive: true, force: true });
                     currentCycle++; continue;
                 } else {
                     console.log(`🟢 [Quality Reviewer] Approved: Semantic match confirmed.`);
                     report += `[Phase 3.5] 🟢 Code logic approved by Reviewer.\n`;
                 }
            }

            // ==========================================
            // PHASE 4: LOCAL SANDBOX VERIFICATION
            // ==========================================
            console.log(`>> [Phase 4] Local Sandbox verification...`);
            const vmTest = await vmDaemon.verifyShadowCandidate(gePaResult.bestSandboxRoot, args.testCommand);
            
            if (vmTest.pass) {
                console.log(`🟢 [LocalSandbox] Passed in ${vmTest.executionTimeMs}ms.`);
                report += `[Phase 4] 🟢 Sandbox passed! (${vmTest.executionTimeMs}ms)\n`;

                // ==========================================
                // PHASE 5: DEPLOY TO HOST
                // ==========================================
                const deployed = await bgRouter.deployToGreenBatch(gePaResult.bestSandboxRoot);
                if (deployed) {
                    report += `\n🎯 [CONCLUSION]: DARWINIAN EVOLUTION CYCLE ${currentCycle} SUCCEEDED — DEPLOYED TO HOST!\n`;
                    return report; 
                }
            } else {
                console.log(`🔴 [LocalSandbox] Failed:\n${vmTest.vmLogs.slice(0, 300)}...`);
                report += `[Phase 4] 🔴 Sandbox runtime FAILED. Not deploying.\n`;
                
                // Cross-Cycle Learning: carry sandbox errors to next iteration
                previousCycleErrors = vmTest.vmLogs;
                await bgRouter.autoRollbackBatch();
                await memLog.recordAttempt(targetFile, `Sandbox Test (${gePaResult.bestCandidateId})`, vmTest.vmLogs, false);
            }
        } else {
            console.log(`🔴 [Pareto Selector] All candidates eliminated by AST Healer!`);
            report += `[Phase 3] 🔴 All population eliminated (TypeScript errors).\n${gePaResult.asiFeedbackReport}\n`;
            
            // Cross-Cycle Learning: carry ASI errors for web research in next cycle
            previousCycleErrors = gePaResult.asiFeedbackReport;
        }
        
        currentCycle++;
    }

    report += `\n[END] Evolution stalled after ${CONFIG.MAX_CYCLES} cycles. Chain-Breaker engaged.\n`;
    await bgRouter.autoRollback(targetFile);
    return report;
}

export const metadata = {
    name: "liva_ai_scientist",
    search_keywords: ["liva_ai_scientist", "tiến hóa", "evolution", "đột biến", "self-upgrade", "tối ưu"],
    description: "LIVA V7 Darwinian Evolution Engine. Generates code mutations, validates via AST + sandbox, deploys via git-native blue-green router. Web-augmented research for error fixes.",
    parameters: {
      type: "object",
      properties: {
        goal: { type: "string" },
        targetFilePath: { type: "string" },
        testCommand: { type: "string" }
      },
      required: ["goal", "targetFilePath"],
    },
};
