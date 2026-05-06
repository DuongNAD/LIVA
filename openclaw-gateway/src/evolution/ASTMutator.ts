import OpenAI from "openai";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { ASTActuator, FileMutation } from "../core/ASTActuator";
import { EvolutionContext } from "./types";
import { evoLogger } from "./EvolutionLogger";
import { jsonrepair } from "jsonrepair";

export class ASTMutator {
    static async apply(ctx: EvolutionContext) {
        if (!ctx.hypothesis?.targetFilePath) return;

        evoLogger.info(`[ASTMutator] Đang chuyển hóa Ý Tưởng thành AST Patch...`);
        
        let sourceCode = "";
        try {
            sourceCode = await fs.readFile(ctx.hypothesis.targetFilePath, "utf8");
        } catch {
            evoLogger.warn(`[ASTMutator] File ${ctx.hypothesis.targetFilePath} không tồn tại.`);
        }

        const prompt = `You are a Senior TypeScript Architect.
Your task is to implement this optimization idea:
Idea: ${ctx.hypothesis.idea}
Target File: ${ctx.hypothesis.targetFilePath}

Current code:
\`\`\`typescript
${sourceCode}
\`\`\`

You must return EXACTLY ONE JSON array of mutations. Format:
[
  {
    "type": "modify",
    "filePath": "${ctx.hypothesis.targetFilePath}",
    "code": "<<<< SEARCH\\n[exact old lines]\\n====\\n>>>> REPLACE\\n[new lines]"
  }
]
Return ONLY JSON. Do not write markdown.`;

        const aiClient = new OpenAI({ baseURL: "http://127.0.0.1:8001/v1", apiKey: "liva-ghost-coder" });
        const response = await aiClient.chat.completions.create({
            model: "expert",
            temperature: 0.1,
            max_tokens: 2000,
            messages: [{ role: "user", content: prompt }]
        });

        const reply = response.choices[0]?.message?.content || "";
        let cleanReply = reply.replaceAll(/<think>[\s\S]*?<\/think>/gi, "").trim();
        const mdMatch = cleanReply.match(/```(?:json)?\n([\s\S]*?)\n```/);
        if (mdMatch) cleanReply = mdMatch[1];
        
        let mutations: FileMutation[];
        try {
            mutations = JSON.parse(jsonrepair(cleanReply));
        } catch {
            throw new Error(`[ASTMutator] Lỗi parse JSON Mutation.`);
        }

        // Tuyệt đối không dùng SkillRegistry, gọi thẳng ASTActuator theo rule
        const actuator = new ASTActuator(process.cwd());
        const candidateId = `singularity_epoch_${ctx.iteration}`;
        const result = await actuator.actuateCandidateBatch(candidateId, mutations);
        
        if (!result.success || !result.sandboxRoot) {
            throw new Error(`[ASTMutator] Cập nhật AST thất bại: ${result.asi}`);
        }
        
        // Đồng bộ file từ Sandbox ra Host (bởi vì RollbackManager đã backup host)
        for (const m of mutations) {
            const relativePath = path.isAbsolute(m.filePath) ? path.relative(process.cwd(), m.filePath) : m.filePath;
            const sandboxFilePath = path.join(result.sandboxRoot, relativePath);
            const hostFilePath = path.join(process.cwd(), relativePath);
            
            try {
                await fs.access(sandboxFilePath);
                await fs.copyFile(sandboxFilePath, hostFilePath);
                evoLogger.info(`[ASTMutator] Đã tiêm code thành công từ Sandbox ra Host: ${hostFilePath}`);
            } catch {
                // file does not exist, ignore
            }
        }
    }
}
