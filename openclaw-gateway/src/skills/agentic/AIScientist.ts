import OpenAI from "openai";
import { logger } from "@utils/logger";
import * as fs from "node:fs";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import { DarwinianEvolver } from "@evolution/DarwinianEvolver.js";
import { LearningLog } from "@evolution/LearningLog.js";
import { MicroVMDaemon } from "@sandbox/MicroVMDaemon.js";
import { BlueGreenRouter } from "@deployment/BlueGreenRouter.js";
import { QualityChecker } from "@evolution/QualityChecker.js";
import { extractXMLPatches, type PopulationPayload } from "@evolution/StructuredExtractor.js";
import { fullResearch } from "@evolution/WebResearchAgent.js";

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
    logger.info(report);

    const aiClient = new OpenAI({ baseURL: CONFIG.AI_BASE_URL, apiKey: CONFIG.AI_API_KEY });
    let currentCycle = 1;

    // Cross-Cycle State: carried forward between iterations
    let previousCycleErrors = "";
    let previousReviewerFeedback = "";
    const previousBestDiff = "";

    while (currentCycle <= CONFIG.MAX_CYCLES) {
        report += `\n>> [Cycle #${currentCycle}/${CONFIG.MAX_CYCLES}] Analyzing target...\n`;
        logger.info(`\n========== DARWINIAN CYCLE #${currentCycle} ==========`);

        // Progressive temperature: decreases each cycle for more focused output
        const cycleTemp = Math.max(0.2, 0.6 - (currentCycle - 1) * 0.2);

        // ==========================================
        // PHASE 1: RAG MEMORY RETRIEVAL
        // ==========================================
        logger.info(">> [Phase 1] Retrieving axioms from Vector Database...");
        const axioms = await memLog.getRelevantAxioms(targetFile, args.goal);
        
        let safeAxioms = axioms;
        if (safeAxioms.length > 8000) {
            safeAxioms = safeAxioms.substring(0, 8000) + "\n... (Truncated to save tokens)";
        }
        
        report += `[Phase 1] RAG Axioms applied to constrain hallucination zone.\n`;

        // ==========================================
        // PHASE 1.5: WEB RESEARCH (NEW)
        // ==========================================
        let webContext = "";
        if (CONFIG.ENABLE_WEB_RESEARCH) {
            logger.info(">> [Phase 1.5] Web Research — searching for solutions...");
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
        logger.info(">> [Phase 2] Darwinian Coder generating population...");
        const rawCode = await fsp.readFile(targetFile, "utf8");
        
        // Giữ nguyên file Raw để AI clone y hệt code vào khối SEARCH
        const originalCode = rawCode;
        
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
STRICT LANGUAGE RULE: ALL OF YOUR INTERNAL REASONING, GENERATED CODE COMMENTS, AND OUTPUTS MUST BE STRICTLY IN ENGLISH.

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
1. DO NOT return JSON. You must return EXACTLY 2 candidates using the XML format below.
2. Inside each candidate, provide your code modifications using the SEARCH/REPLACE block format.
3. EXTREMELY CRITICAL: Every SEARCH block must EXACTLY match the original source file lines (character for character, including whitespace and indentation) so the Actuator regex can find it. Include at least 2 lines of context before and after the change in the SEARCH block.
4. If you need to create a new file, provide an empty SEARCH block, and put the new file content in the REPLACE section.

EXPECTED OUTPUT FORMAT (No conversational text):
<candidate id="cand_A">
<patch filePath="src/skills/AIScientist.ts">
<<<< SEARCH
    // this is context from the file
    const oldLine = 10;
====
    // this is context from the file
    const newLine = 20;
>>>> REPLACE
</patch>
</candidate>

<candidate id="cand_B">
  ... (second approach) ...
</candidate>
        `.trim();

        let populationRes: PopulationPayload | null = null;
        let rawTextContent = "";
        
        try {
            const streamRes = await aiClient.chat.completions.create({
                model: CONFIG.AI_MODEL,
                messages: [{ role: "user", content: coderPrompt }],
                temperature: cycleTemp,
                max_tokens: 16380,
                // response_format: none -> Stream raw xml/markdown
            }, { timeout: 1800000 });

            rawTextContent = streamRes.choices[0]?.message?.content || "";
            
            // Log thinking blocks for debug (then strip)
            const thinkMatch = rawTextContent.match(/<think>([\s\S]*?)<\/think>/i);
            if (thinkMatch) {
                logger.info(`\n[Coder Internal Reasoning]:\n${thinkMatch[1].trim().slice(0, 500)}`);
            }

            // Structured Extraction + XML-Patch Regex Validation
            const extraction = extractXMLPatches(rawTextContent);
            
            if (!extraction.success) {
                logger.error(`\n[AIScientist] 🔴 Structured extraction FAILED!`);
                extraction.errors.forEach(e => logger.error(`  ${e}`));
                logger.error(`--- RAW OUTPUT (first 500 chars) ---\n${rawTextContent.slice(0, 500)}`);
                
                previousCycleErrors = extraction.errors.join("\n");
                report += `[Phase 2] 🔴 Coder output failed Zod validation. Method tried: ${extraction.method}\n`;
                currentCycle++; continue;
            }

            populationRes = extraction.data;
            logger.info(`✅ [Phase 2] Population extracted via ${extraction.method}: ${populationRes!.population.length} candidates`);
            
        } catch (error: any) {
             const errMsg = error.message || "";
             if (errMsg.includes("maximum context length") || errMsg.includes("tokens")) {
                 logger.info(`[Coder Fatal] TOKEN OVERFLOW: ${errMsg}`);
                 report += `[Phase 2] 🔴 Context OOM: Prompt too large for n_ctx!\n`;
             } else {
                 logger.info(`[Coder Fatal] API/JSON error: ${errMsg}\n>>> RAW:\n${rawTextContent.slice(0, 500)}\n`);
                 report += `[Phase 2] 🔴 Coder hallucinated invalid output. (${errMsg})\n`;
             }
             previousCycleErrors = errMsg;
             currentCycle++; continue;
        }

        if (!populationRes || !populationRes.population || populationRes.population.length === 0) { // NOSONAR
            logger.info(`Population empty — evolution stalled.`);
            currentCycle++; continue;
        }

        // ==========================================
        // PHASE 3: DARWINIAN AST SURGERY + PARETO SELECTOR
        // ==========================================
        logger.info(">> [Phase 3] AST Healer & Pareto Selection...");
        const gePaResult = await evolver.evaluateBatchPopulation(
            targetFile,
            populationRes.population
        );

        if (gePaResult.bestCandidateId && gePaResult.bestSandboxRoot) {
            logger.info(`🟢 [Pareto Selector] Survivor: ${gePaResult.bestCandidateId}`);
            report += `[Phase 3] 🟢 Candidate ${gePaResult.bestCandidateId} passed AST verification.\n`;
            
            // ==========================================
            // PHASE 3.5: SENIOR AI CODE REVIEWER
            // ==========================================
            if (CONFIG.ENABLE_QUALITY_CHECKER) {
                 logger.info(`>> [Phase 3.5] Senior AI Reviewer evaluating logic...`);
                 const reviewer = new QualityChecker(CONFIG.AI_BASE_URL, CONFIG.AI_API_KEY, CONFIG.AI_MODEL);
                 const qcResult = await reviewer.evaluateCodeQuality(args.goal, gePaResult.bestSandboxRoot);
                 
                 if (!qcResult.pass) {
                     logger.info(`🔴 [Quality Reviewer] Rejected: ${qcResult.feedback}`);
                     report += `[Phase 3.5] 🔴 Reviewer rejected (Semantic Mismatch): ${qcResult.feedback}\n`;
                     
                     // Cross-Cycle Learning: carry reviewer feedback to next iteration
                     previousReviewerFeedback = qcResult.feedback;
                     await memLog.recordAttempt(targetFile, `Quality Review (${gePaResult.bestCandidateId})`, qcResult.feedback, false);
                     
                     if (fs.existsSync(gePaResult.bestSandboxRoot)) fs.rmSync(gePaResult.bestSandboxRoot, { recursive: true, force: true });
                     currentCycle++; continue;
                 } else {
                     logger.info(`🟢 [Quality Reviewer] Approved: Semantic match confirmed.`);
                     report += `[Phase 3.5] 🟢 Code logic approved by Reviewer.\n`;
                 }
            }

            // ==========================================
            // PHASE 4: LOCAL SANDBOX VERIFICATION
            // ==========================================
            logger.info(`>> [Phase 4] Local Sandbox verification...`);
            const vmTest = await vmDaemon.verifyShadowCandidate(gePaResult.bestSandboxRoot, args.testCommand);
            
            if (vmTest.pass) {
                logger.info(`🟢 [LocalSandbox] Passed in ${vmTest.executionTimeMs}ms.`);
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
                logger.info(`🔴 [LocalSandbox] Failed:\n${vmTest.vmLogs.slice(0, 300)}...`);
                report += `[Phase 4] 🔴 Sandbox runtime FAILED. Not deploying.\n`;
                
                // Cross-Cycle Learning: carry sandbox errors to next iteration
                previousCycleErrors = vmTest.vmLogs;
                await bgRouter.autoRollbackBatch();
                await memLog.recordAttempt(targetFile, `Sandbox Test (${gePaResult.bestCandidateId})`, vmTest.vmLogs, false);
            }
        } else {
            logger.info(`🔴 [Pareto Selector] All candidates eliminated by AST Healer!`);
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
    search_keywords: ["liva_ai_scientist", "evolution", "mutation", "self-upgrade", "optimize"],
    description: "LIVA V7 Darwinian Evolution Engine. Generates code mutations, validates via AST + sandbox, deploys via git-native blue-green router. REQUIRED: All inputs to this tool must be exclusively in English.",
    parameters: {
      type: "object",
      properties: {
        goal: { type: "string", description: "The precise coding goal or feature to implement. YOU MUST TRANSLATE THIS COMPLETELY INTO ENGLISH before passing it as a parameter." },
        targetFilePath: { type: "string", description: "Target source file to mutate (e.g. src/core/AgentLoop.ts)" },
        testCommand: { type: "string", description: "Command to verify the code mathematically (e.g. npx tsc --noEmit)" }
      },
      required: ["goal", "targetFilePath"],
    },
};
