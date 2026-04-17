import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { jsonrepair } from "jsonrepair";

export interface QualityAssessment {
    pass: boolean;
    feedback: string;
}

export class QualityChecker {
    private aiClient: OpenAI;
    private model: string;

    constructor(baseUrl: string, apiKey: string, model: string) {
        this.aiClient = new OpenAI({ baseURL: baseUrl, apiKey: apiKey });
        this.model = model;
    }

    /**
     * Dùng Não LLM đóng vai Senior Reviewer để chấm điểm logic đoạn code đột biến.
     */
    public async evaluateCodeQuality(
        goal: string,
        sandboxRoot: string
    ): Promise<QualityAssessment> {
        let diffCode = "";
        try {
            const workspaceSrc = path.join(process.cwd(), "src");
            const sandboxSrc = path.join(sandboxRoot, "src");
            // Git v2.28+ will diff directories even outside git repo
            const { execSync } = require("child_process");
            try {
                // Return code 1 means differences found
                execSync(`git diff --no-index ${workspaceSrc} ${sandboxSrc}`, { encoding: "utf8", stdio: "pipe" });
                diffCode = "No changes detected.";
            } catch (diffErr: any) {
                diffCode = diffErr.stdout || "";
            }
            if (diffCode.length > 20000) {
                 diffCode = diffCode.substring(0, 20000) + "\\n//... (Diff cut off to prevent OOM)";
            }
        } catch (e) {
            return { pass: false, feedback: "Không thể trích xuất Git Diff từ Sandbox." };
        }

        const reviewerPrompt = `
You are the Strict Senior Code Reviewer (LIVA V6).
Your job is to strictly evaluate the new MUTATED TypeScript code to ensure it meets the GOAL without being destructive, doing something hallucinated, or containing logic flaws/infinite loops.

Goal of the mutation: ${goal}

Mutated Code (Unified Git Diff in Sandbox src/):
\`\`\`diff
${diffCode}
\`\`\`

REQUIREMENTS:
1. Validate if the mutation conceptually fulfills the Goal.
2. Ensure there are no glaring anti-patterns or code obfuscation.
3. If it looks acceptable or better, return "pass": true. If it is bad/malicious/hallucinated, return "pass": false with a precise "feedback" explaining why so the Coder can fix it in the next cycle.
4. ABSOLUTELY NO CONVERSATIONAL TEXT. DO NOT output thinking blocks. DO NOT output markdown \`\`\`json. Return ONLY the raw JSON object.

EXPECTED JSON SCHEMA:
{
  "pass": true,
  "feedback": "Your detailed reasoning here..."
}
        `.trim();

        try {
            console.log("   [Quality Checker] Đang vắt óc suy nghĩ và chấm điểm...");
            const streamRes = await this.aiClient.chat.completions.create({
                model: this.model,
                messages: [{ role: "user", content: reviewerPrompt }],
                temperature: 0.1, // Nhiệt độ cực thấp để chấm bài cực khắt khe và công tâm
            });

            const textContent = streamRes.choices[0]?.message?.content || "";
            const firstBrace = textContent.indexOf('{');
            const lastBrace = textContent.lastIndexOf('}');
            const extractedJson = (firstBrace !== -1 && lastBrace !== -1 && lastBrace >= firstBrace) 
                ? textContent.substring(firstBrace, lastBrace + 1)
                : textContent;

            let result: any = null;
            try {
                result = JSON.parse(extractedJson);
            } catch (e) {
                try {
                    result = JSON.parse(jsonrepair(extractedJson));
                } catch (err: any) {
                    throw new Error(`Syntax Error: ${err.message}\n--- RAW LLM OUTPUT ---\n${textContent.slice(0, 1000)}`);
                }
            }

            return {
                pass: !!result.pass,
                feedback: result.feedback || (result.pass ? "Code logic is solid." : "Unspecified rejection.")
            };

        } catch (error: any) {
            const errMsg = error.message || "";
            if (errMsg.includes("maximum context length") || errMsg.includes("tokens")) {
                return { pass: false, feedback: "OOM Context (Too many tokens to review). Please simplify the problem." };
            }
            return { pass: false, feedback: `API Lỗi trong lúc Review: ${errMsg}` };
        }
    }
}
