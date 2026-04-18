import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { DarwinianEvolver } from "../evolution/DarwinianEvolver.js";
import { LearningLog } from "../evolution/LearningLog.js";
import { MicroVMDaemon } from "../sandbox/MicroVMDaemon.js";
import { BlueGreenRouter } from "../deployment/BlueGreenRouter.js";
import { jsonrepair } from "jsonrepair";

const CONFIG = {
    AI_BASE_URL: process.env.AI_BASE_URL || "http://127.0.0.1:8001/v1", // Cổng Expert 14B/26B Host
    AI_API_KEY: process.env.AI_API_KEY || "liva-ghost-coder",
    AI_MODEL: process.env.AI_MODEL || "expert",
    MAX_CYCLES: 3,
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
Your job is to generate a POPULATION of multiple code variations (mutations) to safely achieve the Goal.

Goal: ${args.goal}

<axioms>
${plannerContext}
</axioms>

Original Source Code of ${path.basename(targetFile)}:
\`\`\`typescript
${originalCode}
\`\`\`

REQUIREMENTS:
1. DO NOT return standard patches. Return ONLY a valid JSON object. DO NOT wrap with markdown json blocks or backticks.
2. Provide the EXACT class name and method name you want to mutate. If mutating a standalone function (not in a class), set "className" to "".
3. Generate 2 different "mutations" (Candidate A and B) for the method/function body.
4. EXTREMELY IMPORTANT: The 'code' property must be perfectly escaped for JSON (use \n for newlines, escape " as \"). Do not use raw multiline strings.

EXPECTED JSON SCHEMA:
{
  "className": "ClassNameToModify", 
  "methodName": "methodToModify",
  "population": [
    { "id": "cand_A", "code": "code for A" },
    { "id": "cand_B", "code": "code for B" }
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
            });
            textContent = streamRes.choices[0]?.message?.content || "";
            const firstBrace = textContent.indexOf('{');
            const lastBrace = textContent.lastIndexOf('}');
            const extractedJson = (firstBrace !== -1 && lastBrace !== -1 && lastBrace >= firstBrace) 
                ? textContent.substring(firstBrace, lastBrace + 1)
                : textContent;
            
            try {
                populationRes = JSON.parse(extractedJson);
            } catch(e) {
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
            targetFile,
            populationRes.className,
            populationRes.methodName,
            populationRes.population
        );

        if (gePaResult.bestCandidateId && gePaResult.bestShadowPath) {
            console.log(`🟢 [Pareto Selector] Ứng viên sinh tồn: ${gePaResult.bestCandidateId}`);
            report += `[Pha 3] 🟢 Ứng viên ${gePaResult.bestCandidateId} đã vượt qua lưới bảo vệ AST Healer.\n`;
            
            // Xúc tiến vào Không gian Cách ly Firecracker/E2B
            console.log(`>> Khởi động Buồng giam MicroVM chạy Test Run...`);
            const vmTest = await vmDaemon.verifyShadowCandidate(gePaResult.bestShadowPath, args.testCommand);
            
            if (vmTest.pass) {
                console.log(`🟢 [MicroVM Runtime] Hoàn hảo! Passed trong ${vmTest.executionTimeMs}ms.`);
                report += `[Hệ Miễn Dịch MicroVM] 🟢 Passed! VM bị hủy sau ${vmTest.executionTimeMs}ms.\n`;

                // Triển khai BLUE-GREEN RA HOST
                const deployed = await bgRouter.deployToGreen(gePaResult.bestShadowPath, targetFile);
                if (deployed) {
                    report += `\n🎯 [KẾT LUẬN]: TIẾN HÓA DARWINIAN HOÀN TẤT VÀ DEPLOY THÀNH CÔNG VÀO LÕI V6!\n`;
                    return report; 
                }
            } else {
                console.log(`🔴 [MicroVM Runtime] Kịch bản Runtime cháy máy:\n${vmTest.vmLogs.slice(0, 300)}...`);
                report += `[Hệ Miễn Dịch MicroVM] 🔴 Runtime Failed. Không cho phép ghi vào Host. Lưu hồi ức.\n`;
                
                await bgRouter.autoRollback(targetFile);
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
