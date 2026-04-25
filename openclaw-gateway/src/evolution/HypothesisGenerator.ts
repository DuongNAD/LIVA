import OpenAI from "openai";
import { jsonrepair } from "jsonrepair";
import * as fsSync from "node:fs";
import * as path from "node:path";
import { evoLogger } from "./EvolutionLogger";
import { EvolutionContext, SingularityHypothesisSchema } from "./types";
import { sleep } from "./EngineManager";

const EXPERT_API_URL = "http://127.0.0.1:8001/v1";

export class HypothesisGenerator {
    static async run(ctx: EvolutionContext) {
        const systemPrompt = `You are J.A.R.V.I.S - Supreme Singularity Architect.
Task: Meticulously find 1 CENTRAL file with extreme OPTIMIZATION potential based on the API Surface. CREATIVITY INJECTION WARNING:
1. TargetFilePath: Choose a path that EXACTLY exists in the Architecture Map. DO NOT hallucinate paths.
2. FORCE CREATIVITY (BAN REPETITION): You are STUCK in a boring mindset (constantly repeating Array to O(1) Map, TTL, Garbage Collection). I STRICTLY FORBID YOU TO PROPOSE THESE IDEAS UNLESS ABSOLUTELY NECESSARY! Brainstorm other top-tier architectures like:
   - Worker Threads / Multi-processing
   - Predictive Context Caching/Memoization
   - Event-Driven / Pub-Sub (Message Queues)
   - Algorithmic Complexity Reduction (Dynamic Programming, Graph algorithms)
   - Lazy Loading / Data stream batching
   - Declarative Prompt Structures (like DSPy)
3. Structural Fit: The modification target must be realistic and logically suitable for the type (class/interface) of the file being exported.
4. DO NOT SELECT RECENTLY OPTIMIZED FILES:
${ctx.blacklistFiles.length > 0 ? JSON.stringify(ctx.blacklistFiles) : "[Empty]"}

CORE GOAL: The ultimate task is to CRAFT A NEW ARCHITECTURE AHEAD OF ITS TIME. Be groundbreaking!

[IMMUTABLE GOLDEN EVOLUTION AXIOMS] (Mandatory):
${ctx.axioms}

CRITICAL REQUIREMENT: ALL YOUR INTERNAL THOUGHTS AND RESPONSES MUST BE STRICTLY IN ENGLISH. IF YOU USE VIETNAMESE, YOU WILL BE TERMINATED.

STEP 1: Open <thought> tag to deeply reason about the Architecture Map, bottlenecks, and choose the most critical file to optimize.
STEP 2: Return ONLY RAW JSON inside a markdown block. Example:
\`\`\`json
{
   "targetFilePath": "src/... (MUST BE EXACT PATH ACCORDING TO MAP)",
   "idea": "Proposal COMPATIBLE WITH TRUE FILE STRUCTURE...",
   "shell_commands": ["npm install uuid", "npm install -D @types/uuid"],
   "pros": "Advantages of this decision...",
   "cons": "Disadvantages, risks...",
   "testingStrategy": "Which edge-cases will you cover using assert / vitest in the Sandbox?",
   "rollbackPlan": "What is the rollback strategy?",
   "feasibilityScore": "Realistic feasibility score (1-10)",
   "testCommand": "npx tsc --noEmit"
}
\`\`\`
EXTREME WARNING: If you keep proposing "Use Map for O(1)" architecture, your Feasibility score will be forced to 0 and you will fail!`;

        let projectContext = `Current LIVA Project Structure:\n${ctx.projectSurfaceInfo}\n\n[BOTTLENECK PROFILER - PRIORITY TO FIX]:\n${ctx.bottlenecks}\n\n[Experiences To Avoid Repeating]:\n${ctx.pastExperiences}\n\nCRITICAL: BASED ON THE ABOVE, YOU MUST GENERATE EXACTLY ONE RAW JSON BLOCK. Do not write markdown reports. Response MUST be in this format:\n{\n  "targetFilePath": "...",\n  "idea": "...",\n  "shell_commands": [],\n  "pros": "...",\n  "cons": "...",\n  "testingStrategy": "...",\n  "rollbackPlan": "...",\n  "feasibilityScore": "...",\n  "testCommand": "..."\n}`;

        evoLogger.info(`[Meta-Cognition] Đang kết nối lên Não Planner để vắt óc suy nghĩ ý tưởng tái cấu trúc...`);

        const aiClient = new OpenAI({ 
            baseURL: EXPERT_API_URL, 
            apiKey: "liva-ghost-planner",
            timeout: 15 * 60 * 1000, 
            maxRetries: 0
        });

        const MAX_RETRIES = 3;
        for (let attempt = 1; attempt <= MAX_RETRIES; attempt++) {
            try {
                const response = await aiClient.chat.completions.create({
                    model: "expert",
                    temperature: 0.4, 
                    top_p: 0.9,       
                    max_tokens: 1500, 
                    stop: ["<start_of_turn>", "<end_of_turn>", "```\n\nWait", "Wait, I see"],
                    messages: [{ role: "user", content: `${systemPrompt}\n\n${projectContext}` }]
                }, { timeout: 900000 });

                const replyRaw = response.choices[0]?.message?.content || "";
                
                const cleanReply = replyRaw.replaceAll(/<think>[\s\S]*?<\/think>/gi, "").trim();

                let parsedObj: any;
                const mdMatch = cleanReply.match(/```(?:json)?\n([\s\S]*?)\n```/);
                if (mdMatch) {
                    parsedObj = JSON.parse(jsonrepair(mdMatch[1]));
                } else {
                    const start = cleanReply.indexOf("{");
                    const end = cleanReply.lastIndexOf("}");
                    if (start !== -1 && end !== -1 && end > start) {
                         parsedObj = JSON.parse(jsonrepair(cleanReply.substring(start, end + 1)));
                    } else {
                         throw new Error("Mô hình không trả về định dạng JSON hợp lệ!");
                    }
                }
                
                // Validate bằng Zod theo chuẩn AI_CONTEXT.md
                ctx.hypothesis = SingularityHypothesisSchema.parse(parsedObj);
                
                fsSync.writeFileSync(path.join(ctx.workspaceDir, "current_plan.json"), JSON.stringify(ctx.hypothesis, null, 2), "utf-8");
                break;
            } catch (err: any) {
                evoLogger.warn({ err }, `[Retry] Lỗi JSON Kế hoạch (Lần ${attempt})`);
                if (attempt === MAX_RETRIES) throw new Error("Thất bại phân tích JSON Kế hoạch sau 3 lần nặn. Dừng Tiến hóa!");
                await sleep(5000);
            }
        }

        evoLogger.info(`[BÓNG SÁNG Ý TƯỞNG] ĐÃ XUẤT HIỆN:
- Mục Tiêu : ${ctx.hypothesis?.targetFilePath}
- Đề Xuất  : ${ctx.hypothesis?.idea}
- Ưu Điểm  : ${ctx.hypothesis?.pros}`);
    }
}
