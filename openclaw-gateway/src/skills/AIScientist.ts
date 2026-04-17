import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { DarwinianEvolver } from "../evolution/DarwinianEvolver.js";
import { LearningLog } from "../evolution/LearningLog.js";
import { MicroVMDaemon } from "../sandbox/MicroVMDaemon.js";
import { BlueGreenRouter } from "../deployment/BlueGreenRouter.js";
import { QualityChecker } from "../evolution/QualityChecker.js";
import { jsonrepair } from "jsonrepair";

const CONFIG = {
    AI_BASE_URL: process.env.AI_BASE_URL || "http://127.0.0.1:8001/v1", // Cổng Expert 14B/26B Host
    AI_API_KEY: process.env.AI_API_KEY || "liva-ghost-coder",
    AI_MODEL: process.env.AI_MODEL || "expert",
    MAX_CYCLES: 3,
    ENABLE_QUALITY_CHECKER: process.env.ENABLE_QUALITY_CHECKER !== "false" // Mặc định bật Reviewer
};

// Singleton Khởi tạo lõi Trí Nhớ Tiến Hóa (Chống tải lại Vector DB quá nhiều lần)
const memLog = new LearningLog();
memLog.connect().catch(() => {});

export interface AgentArgs {
    goal: string;
    targetFilePath: string;
    testCommand?: string;
}

/**
 * BỘ CHỈ HUY TIẾN HÓA LIVA V6 ULTIMATE (Darwinian Triad Orchestrator)
 * Vòng Lặp Pareto: Kỹ Sư Trưởng -> Thợ Code GEPA -> Thanh Tra Đột Biến
 */
export const execute = async (args: AgentArgs): Promise<string> => {
    const workspace = process.cwd();
    const targetFile = path.isAbsolute(args.targetFilePath) 
        ? args.targetFilePath 
        : path.resolve(workspace, args.targetFilePath);
    
    if (!fs.existsSync(targetFile)) {
        return `🔴 Mù mục tiêu: File ${targetFile} không tồn tại trên Gateway.`;
    }

    // Các thành phần Hệ điều hành
    const evolver = new DarwinianEvolver(workspace, memLog);
    const vmDaemon = new MicroVMDaemon();
    const bgRouter = new BlueGreenRouter(workspace);

    let report = `\n# LIVA V6 ULTIMATE: DARWINIAN EVOLUTION KHỞI ĐỘNG\n`;
    console.log(report);

    const aiClient = new OpenAI({ baseURL: CONFIG.AI_BASE_URL, apiKey: CONFIG.AI_API_KEY });
    let currentCycle = 1;

    while (currentCycle <= CONFIG.MAX_CYCLES) {
        report += `\n>> [Vòng lặp #${currentCycle}] Phân tích mục tiêu...\n`;
        console.log(`\n========== VÒNG LẶP DARWINIAN #${currentCycle} ==========`);

        // ==========================================
        // PHA 1: MASTER PLANNER (KỸ SƯ TRƯỞNG & GEPA)
        // ==========================================
        console.log(">> [Pha 1] Rút trích Tiên Đề từ Vector Database...");
        const axioms = await memLog.getRelevantAxioms(targetFile, args.goal);
        
        // Giới hạn chiều dài của Axioms để không làm phình OOM Context (Max 2000 chars)
        let safeAxioms = axioms;
        if (safeAxioms.length > 2000) {
            safeAxioms = safeAxioms.substring(0, 2000) + "\n... (Truncated past failures to save tokens)";
        }
        
        // (Trong tương lai có thể gọi @ast-grep/napi ở đây để xuất Blueprint phức tạp)
        const plannerContext = `Cảnh báo RAG: ${safeAxioms}`;
        report += `[Pha 1] Kỹ sư Trưởng đã áp dụng RAG Axioms để giới hạn vùng ảo giác.\n`;

        // ==========================================
        // PHA 2: DARWINIAN AST-CODER (THỢ CODE ĐỘT BIẾN)
        // ==========================================
        console.log(">> [Pha 2] Darwinian Coder đang sinh quần thể mảng (Population)...");
        const rawCode = fs.readFileSync(targetFile, "utf8");
        // Giải pháp Thắt Lưng Buộc Bụng: Xóa comments và dòng trống để tiết kiệm cực độ Tokens
        const originalCode = rawCode.replace(/\/\*[\s\S]*?\*\/|\/\/.*/g, '').replace(/^\s*[\r\n]/gm, '');
        
        
        const coderPrompt = `
You are the Darwinian Coder (LIVA V6).
Your job is to generate a POPULATION of multiple code variations to safely achieve the Goal. You are now in a Multi-File Sandbox. You can both modify existing files and create new files.

Goal: ${args.goal}

<axioms>
${plannerContext}
</axioms>

Original Target Epicenter Source Code (${targetFile}):
\`\`\`typescript
${originalCode}
\`\`\`

REQUIREMENTS:
1. DO NOT return standard patches. Return ONLY a valid JSON object.
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
        { "type": "modify", "filePath": "src/core/AgentLoop.ts", "code": "import * as path from 'path';\\nexport class AgentLoop { private a: string = 'fix'; }" },
        { "type": "create", "filePath": "src/events/MessageBus.ts", "code": "export class Bus {}" }
      ]
    },
    { "id": "cand_B", "mutations": [...] }
  ]
}
        `.trim();

        let textContent = "";
        let populationRes: any = null;
        try {
            const streamRes = await aiClient.chat.completions.create({
                model: CONFIG.AI_MODEL,
                messages: [{ role: "user", content: coderPrompt }],
                temperature: 0.6, // Nhiệt độ trung bình để sinh biến thể đa dạng nhưng không rác
                max_tokens: 16380, // Tăng trần để tránh đứt gãy JSON nưã chừng
            }, { timeout: 1800000 }); // Chờ tối đa 30 phút cho mô hình Offload siêu nặng
            textContent = streamRes.choices[0]?.message?.content || "";
            
            // TRÍCH XUẤT VÀ IN NỘI TÂM ĐỂ DEBUG
            const thinkMatch = textContent.match(/<think>([\s\S]*?)<\/think>/i);
            if (thinkMatch) {
                console.log(`\n[R1 32B Nội tâm (Coder)]:\n${thinkMatch[1].trim()}`);
            }

            // Lọc sạch toàn bộ dòng suy nghĩ nội tâm <think>... để chống gãy JSON
            textContent = textContent.replace(/<think>[\s\S]*?<\/think>/gi, "").trim();

            const firstBrace = textContent.indexOf('{');
            const lastBrace = textContent.lastIndexOf('}');
            const extractedJson = (firstBrace !== -1 && lastBrace !== -1 && lastBrace >= firstBrace) 
                ? textContent.substring(firstBrace, lastBrace + 1)
                : textContent;
            
            try {
                populationRes = JSON.parse(extractedJson);
            } catch(e) {
                console.error(`\n[AIScientist] 🔴 Lỗi Parse JSON Coder! R1 đang sinh JSON hỏng cấu trúc. Đang thử cứu hộ bằng jsonrepair...\n--- NỘI DUNG GÃY ---\n${extractedJson}\n--------------------`);
                populationRes = JSON.parse(jsonrepair(extractedJson));
            }
            
        } catch (error: any) {
             const errMsg = error.message || "";
             if (errMsg.includes("maximum context length") || errMsg.includes("tokens")) {
                 console.log(`[Coder Fatal] QUÁ TẢI TOKENS: ${errMsg}`);
                 report += `[Pha 2] Báo động đỏ: Tràn RAM Context (OOM Tokens). Kích thước Prompt quá lớn so với n_ctx!\n`;
             } else {
                 console.log(`[Coder Fatal] Lỗi API hoặc Vỡ Json: ${errMsg}\n>>> [TRÍCH XUẤT RAW]:\n${textContent.slice(0, 1000)}\n`);
                 report += `[Pha 2] Báo động đỏ: Coder bị mù ảo giác và không thể xuất JSON chuẩn. (${errMsg})\n`;
             }
             currentCycle++; continue;
        }

        if (!populationRes || !populationRes.population || populationRes.population.length === 0) {
            console.log(`Hệ thống từ chối tiến hóa: Dữ liệu quần thể rỗng.`);
            currentCycle++; continue;
        }

        // ==========================================
        // PHA 3: QUALITY EVALUATOR & PARETO SELECTOR
        // ==========================================
        console.log(">> [Pha 3] Khởi động AST Healer & Bộ Lọc Pareto GEPA...");
        const gePaResult = await evolver.evaluateBatchPopulation(
            targetFile, // still pass epicenter for logging
            populationRes.population
        );

        if (gePaResult.bestCandidateId && gePaResult.bestSandboxRoot) {
            console.log(`🟢 [Pareto Selector] Ứng viên sinh tồn: ${gePaResult.bestCandidateId}`);
            report += `[Pha 3] 🟢 Ứng viên ${gePaResult.bestCandidateId} đã vượt qua lưới bảo vệ AST Healer.\\n`;
            
            // ==========================================
            // PHA 3.5: SENIOR AI CODE REVIEWER
            // ==========================================
            if (CONFIG.ENABLE_QUALITY_CHECKER) {
                 console.log(`>> [Pha 3.5] Kích hoạt Senior AI Reviewer để soi Logic code...`);
                 const reviewer = new QualityChecker(CONFIG.AI_BASE_URL, CONFIG.AI_API_KEY, CONFIG.AI_MODEL);
                 const qcResult = await reviewer.evaluateCodeQuality(args.goal, gePaResult.bestSandboxRoot);
                 
                 if (!qcResult.pass) {
                     console.log(`🔴 [Quality Reviewer] Bác bỏ ứng viên: ${qcResult.feedback}`);
                     report += `[Pha 3.5] 🔴 Trí tuệ Reviewer từ chối (Semantic Mismatch): ${qcResult.feedback}\\n`;
                     await memLog.recordAttempt(targetFile, `Quality Review (${gePaResult.bestCandidateId})`, qcResult.feedback, false);
                     
                     if (fs.existsSync(gePaResult.bestSandboxRoot)) fs.rmSync(gePaResult.bestSandboxRoot, { recursive: true, force: true });
                     currentCycle++; continue;
                 } else {
                     console.log(`🟢 [Quality Reviewer] Phê duyệt: Khớp Semantic và không phá hoại.`);
                     report += `[Pha 3.5] 🟢 Cấu trúc Logic đã được Reviewer đóng mộc Phê duyệt.\\n`;
                 }
            }

            // Xúc tiến vào Không gian Cách ly Firecracker/E2B
            console.log(`>> Khởi động Buồng giam MicroVM chạy Test Run...`);
            const vmTest = await vmDaemon.verifyShadowCandidate(gePaResult.bestSandboxRoot, args.testCommand);
            
            if (vmTest.pass) {
                console.log(`🟢 [MicroVM Runtime] Hoàn hảo! Passed trong ${vmTest.executionTimeMs}ms.`);
                report += `[Hệ Miễn Dịch MicroVM] 🟢 Passed! VM bị hủy sau ${vmTest.executionTimeMs}ms.\\n`;

                // Triển khai BLUE-GREEN RA HOST THEO BATCH
                const deployed = await bgRouter.deployToGreenBatch(gePaResult.bestSandboxRoot);
                if (deployed) {
                    report += `\\n🎯 [KẾT LUẬN]: TIẾN HÓA DARWINIAN HOÀN TẤT VÀ DEPLOY BATCH THÀNH CÔNG VÀO LÕI V6!\\n`;
                    return report; 
                }
            } else {
                console.log(`🔴 [MicroVM Runtime] Kịch bản Runtime cháy máy:\\n${vmTest.vmLogs.slice(0, 300)}...`);
                report += `[Hệ Miễn Dịch MicroVM] 🔴 Runtime Failed. Không cho phép ghi vào Host. Lưu hồi ức.\\n`;
                
                await bgRouter.autoRollbackBatch();
                await memLog.recordAttempt(targetFile, `MicroVM Test (${gePaResult.bestCandidateId})`, vmTest.vmLogs, false);
            }
        } else {
            console.log(`🔴 [Pareto Selector] Rác Đột Biến: Toàn bộ Population bị AST Healer tiêu diệt!`);
            report += `[Pha 3] 🔴 Toàn bộ quần thể bị chối từ vì lỗi Syntax TypeScript. Phản hồi ASI:\n${gePaResult.asiFeedbackReport}\n`;
        }
        
        currentCycle++;
    }

    report += `\n[HẾT VÒNG LẶP] Quá trình đột biến bế tắc. Chain-Breaker GEPA kích hoạt cắt đứt tiến trình.\n`;
    await bgRouter.autoRollback(targetFile); // Bảo hiểm an toàn chót
    return report;
}

export const metadata = {
    name: "liva_ai_scientist",
    search_keywords: ["liva_ai_scientist", "tiến hóa", "thợ code geapa", "đột biến"],
    description: "Kỹ năng đặc vụ Darwinian. Giải quyết bug cấu trúc, tối ưu hàm, tự phẫu thuật AST và gọi hộp cát Firecracker test.",
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
