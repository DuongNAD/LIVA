import OpenAI from "openai";
import * as fs from "fs";
import * as path from "path";
import { extractAndValidate, QualityAssessmentSchema, type QualityAssessmentPayload } from "./StructuredExtractor.js";

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
     * Evaluate code quality using Senior AI Reviewer.
     * Uses Structured Extraction + Zod validation for robust output parsing.
     */
    public async evaluateCodeQuality(
        goal: string,
        sandboxRoot: string
    ): Promise<QualityAssessment> {
        let diffCode = "";
        try {
            const workspaceSrc = path.join(process.cwd(), "src");
            const sandboxSrc = path.join(sandboxRoot, "src");
            const { execSync } = require("child_process");
            try {
                execSync(`git diff --no-index ${workspaceSrc} ${sandboxSrc}`, { encoding: "utf8", stdio: "pipe" });
                diffCode = "No changes detected.";
            } catch (diffErr: any) {
                diffCode = diffErr.stdout || "";
            }
            if (diffCode.length > 20000) {
                 diffCode = diffCode.substring(0, 20000) + "\n//... (Diff truncated to prevent OOM)";
            }
        } catch (e) {
            return { pass: false, feedback: "Cannot extract Git Diff from Sandbox." };
        }

        const reviewerPrompt = `
You are the Strict Senior Code Reviewer (LIVA V7).
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
4. ABSOLUTELY NO CONVERSATIONAL TEXT. Return ONLY the raw JSON object.

EXPECTED JSON SCHEMA:
{
  "pass": true,
  "feedback": "Your detailed reasoning here..."
}
        `.trim();

        try {
            console.log("   [Quality Checker] Evaluating code logic...");
            const streamRes = await this.aiClient.chat.completions.create({
                model: this.model,
                messages: [{ role: "user", content: reviewerPrompt }],
                temperature: 0.1,
                response_format: { type: "json_object" },
            });

            const textContent = streamRes.choices[0]?.message?.content || "";
            
            // Use StructuredExtractor + Zod validation
            const extraction = extractAndValidate(textContent, QualityAssessmentSchema);
            
            if (extraction.success && extraction.data) {
                return {
                    pass: extraction.data.pass,
                    feedback: extraction.data.feedback,
                };
            }

            // Extraction failed — treat as rejection with diagnostic info
            console.error(`[QualityChecker] Structured extraction failed:`, extraction.errors);
            return {
                pass: false,
                feedback: `Reviewer output failed validation: ${extraction.errors.join("; ")}`,
            };

        } catch (error: any) {
            const errMsg = error.message || "";
            if (errMsg.includes("maximum context length") || errMsg.includes("tokens")) {
                return { pass: false, feedback: "OOM Context (Too many tokens to review). Simplify the problem." };
            }
            return { pass: false, feedback: `API Error during review: ${errMsg}` };
        }
    }
}
