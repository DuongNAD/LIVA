const fs = require('fs');

let content = fs.readFileSync('src/core/AgentLoop.ts', 'utf8');

// Add imports
if (!content.includes('HeraCompass')) {
    content = content.replace(
        /import \{ logger \} from "\.\.\/utils\/logger";/,
        'import { logger } from "../utils/logger";\nimport { HeraCompass } from "../memory/HeraCompass";'
    );
}

// Add #errorBacklog and processErrorBacklog
if (!content.includes('#errorBacklog: Array')) {
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
            HeraCompass.getInstance().learnFromError(this.#aiRouterClient as any, err.toolTarget, err.context, err.execErr).catch(e => logger.error(e, "[HeraCompass] Async error:"));
        }
    }

    public handleUserInput(userText: string) {`
    );
}

// Process backlog when going IDLE in dispatch?
// The user blueprint says: "Khi FSM chuyển về trạng thái IDLE mới kích hoạt luồng quét lỗi"
// In `AgentLoop.ts`, we transition to IDLE in `transitionTo(phase: AgentPhase...` or somewhere else?
// Wait, `this.#currentPhase = AgentPhase.IDLE` is set somewhere.
// Let's just hook it to `transitionTo`.
if (!content.includes('this.processErrorBacklog()')) {
    content = content.replace(
        /this\.#currentPhase = phase;/,
        `this.#currentPhase = phase;\n        if (phase === AgentPhase.IDLE) this.processErrorBacklog();`
    );
}

// Inject HeraCompass Insights
if (!content.includes('HeraCompass.getInstance().getRelatedInsight')) {
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
}

// executeWithReflection - error backlog and utility score
if (!content.includes('this.#errorBacklog.push')) {
    content = content.replace(
        /const \{ resultStr, valid, rawObj \} = await this\.#toolOrchestrator\.executeWithReflection\(callName, callArgs\);/,
        `const { resultStr, valid, rawObj } = await this.#toolOrchestrator.executeWithReflection(callName, callArgs);
                                
                                if (!valid) {
                                    this.#errorBacklog.push({ toolTarget: callName, context: \`Call: \${callName}(\${JSON.stringify(callArgs)})\`, execErr: resultStr });
                                } else {
                                    if (insights && insights.length > 0) {
                                        insights.forEach(i => HeraCompass.getInstance().updateUtilityScore(i.insight_id, true));
                                    }
                                }`
    );
}

fs.writeFileSync('src/core/AgentLoop.ts', content);
console.log('Fixed AgentLoop.ts part 2');
