import fs from 'fs';

let content = fs.readFileSync('src/core/AgentLoop.ts', 'utf8');

// 1. ToolExecutionOrchestrator: heuristicSanitize
content = content.replace(
    /private async sanitize\([\s\S]*?\}\n    \}/,
    `public heuristicSanitize(data: string, maxLen = 2500): string {
        if (data.length <= maxLen) return data;
        
        const trimmedData = data.trim();
        if (trimmedData.startsWith("{") || trimmedData.startsWith("[")) {
            try {
                const parsed = JSON.parse(trimmedData);
                if (Array.isArray(parsed) && parsed.length > 20) {
                    const truncated = parsed.slice(0, 20);
                    return JSON.stringify(truncated) + "\\n\\n<truncated_output>... [System: Cắt xén do vượt quá kích thước. DO NOT PARSE THIS AS JSON. Vui lòng gọi tool khác hỗ trợ phân trang (pagination) nếu cần thêm dữ liệu] ...</truncated_output>\\n";
                }
            } catch (e) {}
        }
        
        const head = data.substring(0, maxLen / 2);
        const tail = data.substring(data.length - (maxLen / 2));
        return "\\n<truncated_output>\\n" + head + "\\n... [System: Cắt xén do vượt quá kích thước. DO NOT PARSE THIS AS JSON. Vui lòng gọi tool khác hỗ trợ phân trang (pagination) nếu cần thêm dữ liệu] ...\\n" + tail + "\\n</truncated_output>\\n";
    }`
);

content = content.replace(
    /if \(resultStr\.length > 2000\) \{\s*logger\.warn\(`Dữ liệu dài \(\$\{resultStr\.length\} chars\)\. Chuyển hướng chui qua \[Sanitizer Sub-Agent\]\.\.\.`\);\s*resultStr = await this\.sanitize\(resultStr\);\s*\}/,
    `if (resultStr.length > 2500) {
                logger.warn(\`Dữ liệu dài (\${resultStr.length} chars). Chuyển hướng qua Heuristic Sanitize O(1)...\`);
                resultStr = this.heuristicSanitize(resultStr, 2500);
            }`
);

// Add imports
if (!content.includes('HeraCompass')) {
    content = content.replace(
        /import \{ logger \} from "\.\.\/utils\/logger";/,
        `import { logger } from "../utils/logger";\nimport { HeraCompass } from "../memory/HeraCompass";`
    );
}

// Add #errorBacklog and queue processor in AgentLoop
content = content.replace(
    /export class AgentLoop \{/,
    `export class AgentLoop {
    #errorBacklog: Array<{ toolTarget: string, context: string, execErr: string }> = [];`
);

content = content.replace(
    /public handleUserInput\(userText: string\) \{/,
    `private processErrorBacklog() {
        if (this.#errorBacklog.length === 0) return;
        logger.info(\`[AgentLoop] Đang xử lý \${this.#errorBacklog.length} lỗi trong Backlog (HeraCompass)...\`);
        const queue = [...this.#errorBacklog];
        this.#errorBacklog = [];
        
        for (const err of queue) {
            HeraCompass.getInstance().learnFromError(this.#aiRouterClient as any, err.toolTarget, err.context, err.execErr).catch(e => logger.error("[HeraCompass] Async error:", e));
        }
    }

    public handleUserInput(userText: string) {`
);

// Call processErrorBacklog when IDLE
content = content.replace(
    /this\.setPhase\(AgentPhase\.IDLE\);\s*\/\/\s*Cắm cờ xả Lane/,
    `this.setPhase(AgentPhase.IDLE);
                this.processErrorBacklog(); // [HeraCompass] Process backlog when IDLE
                // Cắm cờ xả Lane`
);

// Inject HeraCompass getRelatedInsight
content = content.replace(
    /const aiMessages = await PromptBuilder\.prepareFullAiMessages\([\s\S]*?\);/,
    `const aiMessages = await PromptBuilder.prepareFullAiMessages(
                        userText,
                        this.#memory,
                        this.currentSystemLocation,
                        toolsDef
                    );

                    const insights = HeraCompass.getInstance().getRelatedInsight(userText, "", { limit: 2, minScore: 0 });
                    if (insights && insights.length > 0) {
                        const insightText = insights.map(i => \`- \${i.actionable_rule}\`).join('\\n');
                        aiMessages[0].content += \`\\n\\n[HeraCompass] BÀI HỌC KINH NGHIỆM TỪ NHỮNG LẦN THẤT BẠI TRƯỚC:\\n\${insightText}\`;
                    }`
);

// Push to errorBacklog and update utility score in tool execution loop (around executeWithReflection)
content = content.replace(
    /const \{ resultStr, valid, rawObj \} = await this\.#toolOrchestrator\.executeWithReflection\(callName, callArgs\);/,
    `const { resultStr, valid, rawObj } = await this.#toolOrchestrator.executeWithReflection(callName, callArgs);
                                
                                if (!valid) {
                                    this.#errorBacklog.push({ toolTarget: callName, context: \`Call: \${callName}(\${JSON.stringify(callArgs)})\`, execErr: resultStr });
                                } else {
                                    // Reward previous insights if task succeeds
                                    if (insights && insights.length > 0) {
                                        insights.forEach(i => HeraCompass.getInstance().updateUtilityScore(i.insight_id, true));
                                    }
                                }`
);

fs.writeFileSync('src/core/AgentLoop.ts', content);
console.log('Fixed AgentLoop.ts');
