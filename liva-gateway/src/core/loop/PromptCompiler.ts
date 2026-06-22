import type { ChatCompletionMessageParam } from "openai/resources/chat/completions";
import { PromptBuilder } from "../PromptBuilder";
import { ConfigManager } from "../config/ConfigManager";
import { logger } from "../../utils/logger";
import type { MemoryManager } from "../../MemoryManager";
import type { SkillRegistry } from "../../SkillRegistry";
import type { SemanticRouter, MemoryRoute, SkillKit } from "../../memory/SemanticRouter";
import type { AgentSkill } from "../../skills/SkillMetadata";

export interface SpeculativeCache {
    partialText?: string;
    route?: MemoryRoute;
    activeKit?: SkillKit;
    skills?: AgentSkill[];
    aiMessages?: ChatCompletionMessageParam[];
    dynamicContextBlock?: string;
}

export class PromptCompiler {
    public currentSystemLocation = "Vị trí chưa xác định";
    public currentSystemTimezone = "Asia/Ho_Chi_Minh";
    
    public speculativeCache: SpeculativeCache | null = null;
    
    // [PERF H2] Cache social context result (10s TTL) to avoid repeated DB reads
    private socialContextCache: { value: boolean; expiry: number } = { value: false, expiry: 0 };

    public setSystemLocation(loc: string, tz: string = "Asia/Ho_Chi_Minh") {
        this.currentSystemLocation = loc;
        this.currentSystemTimezone = tz;
    }

    public async isInSocialContext(memory: MemoryManager): Promise<boolean> {
        const now = Date.now();
        if (now < this.socialContextCache.expiry) {
            return this.socialContextCache.value;
        }
        try {
            const history = await memory.getShortTermHistory();
            if (!history || history.length === 0) {
                this.socialContextCache = { value: false, expiry: now + 10_000 };
                return false;
            }
            // Inspect last 3 messages
            const recent = history.slice(-3);
            const result = recent.some(msg => {
                const text = msg.content.toLowerCase();
                return text.includes("zalo") || 
                       text.includes("messenger") || 
                       text.includes("tin nhắn") || 
                       text.includes("email") || 
                       text.includes("mail") ||
                       text.includes("gửi");
            });
            this.socialContextCache = { value: result, expiry: now + 10_000 };
            return result;
        } catch {
            this.socialContextCache = { value: false, expiry: now + 10_000 };
            return false;
        }
    }

    public async speculativeWarm(
        partialText: string,
        memory: MemoryManager,
        registry: SkillRegistry,
        semanticRouter: SemanticRouter
    ): Promise<void> {
        try {
            const inSocial = await this.isInSocialContext(memory);
            const routerResult = await semanticRouter.route(partialText, inSocial);
            const skills = await registry.getSemanticTopK(partialText, routerResult.activeKit, 3);
            
            const toolsDef = skills.map((skill) => ({
                name: skill.name,
                description: skill.description,
                parameters: skill.parameters,
            }));
            
            // [v26.1] Hydrate PromptBuilder using partial text
            const { aiMessages, dynamicContextBlock } = await PromptBuilder.prepareFullAiMessages(
                partialText,
                memory,
                {
                    location: this.currentSystemLocation,
                    timezone: this.currentSystemTimezone
                },
                toolsDef,
                routerResult.route,
                routerResult.queryEmbedding // [PERF C2] Reuse cached embedding
            );
            
            this.speculativeCache = {
                partialText,
                route: routerResult.route,
                activeKit: routerResult.activeKit,
                skills,
                aiMessages,
                dynamicContextBlock
            };
            logger.debug(`[v26.1 Speculative] 🔮 Cache hydrated: route=${routerResult.route}, skills=${skills.length}, promptReady=true (TTFT ~ 0ms)`);
        } catch {
            // Silently ignore — speculative warming is best-effort
            this.speculativeCache = null;
        }
    }

    public clearSpeculativeCache(): void {
        this.speculativeCache = null;
        logger.debug("[v26.1 Speculative] 🔮 Cache cleared due to user typing cancellation");
    }

    public async compilePrompt(
        userText: string,
        isHeartbeat: boolean,
        isDryRun: boolean,
        memory: MemoryManager,
        registry: SkillRegistry,
        semanticRouter: SemanticRouter
    ) {
        // [v23 Pillar 2] Check speculative cache — skip route() if already pre-warmed
        let routerResult;
        let activeKit;
        let cachedSkills: AgentSkill[] | undefined;
        let hydratedMessages: ChatCompletionMessageParam[] | undefined;
        let cachedDynamicContextBlock: string | undefined;
        
        if (this.speculativeCache?.route && this.speculativeCache.partialText && userText.startsWith(this.speculativeCache.partialText)) {
            routerResult = { route: this.speculativeCache.route, activeKit: this.speculativeCache.activeKit };
            activeKit = this.speculativeCache.activeKit;
            cachedSkills = this.speculativeCache.skills;
            hydratedMessages = this.speculativeCache.aiMessages;
            cachedDynamicContextBlock = this.speculativeCache.dynamicContextBlock;
            logger.info(`[v23 Speculative] ⚡ Using pre-warmed route: ${routerResult.route} (0ms latency)`);
        } else {
            this.speculativeCache = null;
            // [Dynamic Gating] Tiết lộ lũy tiến bằng SemanticRouter
            const inSocial = await this.isInSocialContext(memory);
            routerResult = await semanticRouter.route(userText, inSocial);
            activeKit = routerResult.activeKit;
        }
        this.speculativeCache = null; // Consume cache

        // [Bypass] Ép bỏ qua gọi Tools đối với các luồng phiếm chỉ/chào hỏi
        let filteredSkills = cachedSkills
            || (routerResult.route === "chitchat" ? [] : await registry.getSemanticTopK(userText, activeKit, 3));
        
        if (isDryRun) {
            const match = userText.match(/mang tên "([^"]+)"/);
            if (match && match[1]) {
                const targetTool = registry.getAllSkills().find(s => s.name === match[1]);
                if (targetTool) {
                    filteredSkills = [targetTool];
                }
            }
        }
        const toolsDef = filteredSkills.map((skill) => ({
            name: skill.name,
            description: skill.description,
            parameters: skill.parameters,
        }));

        let aiMessages: ChatCompletionMessageParam[];
        let dynamicContextBlock: string;
        if (hydratedMessages) {
            aiMessages = hydratedMessages;
            dynamicContextBlock = cachedDynamicContextBlock || "";
        } else {
            const result = await PromptBuilder.prepareFullAiMessages(
                userText,
                memory,
                {
                    location: this.currentSystemLocation,
                    timezone: this.currentSystemTimezone
                },
                toolsDef,
                routerResult.route, // Pass route to optimize context
                routerResult.queryEmbedding // [PERF C2] Reuse cached embedding
            );
            aiMessages = result.aiMessages;
            dynamicContextBlock = result.dynamicContextBlock;
        }

        return {
            aiMessages,
            dynamicContextBlock,
            route: routerResult.route,
            queryEmbedding: routerResult.queryEmbedding,
            cachedAction: routerResult.cachedAction, // For fast-path cached action!
        };
    }

    public runTokenGuard(executionMessages: ChatCompletionMessageParam[]): void {
        // [v28] TokenGuard — Safety net: trim if total prompt exceeds context window
        const ctxLimit = ConfigManager.getInstance().contextWindowTokens;
        const maxResp = 2500; // max_tokens for response
        const safetyMargin = 256; // buffer for encoding overhead
        const hardLimitChars = (ctxLimit - maxResp - safetyMargin) * 4;
        const totalChars = executionMessages.reduce((sum: number, m: ChatCompletionMessageParam) => sum + ((m.content as string)?.length || 0), 0);
        
        if (totalChars > hardLimitChars) {
            logger.warn(`[TokenGuard] ⚠️ Prompt ~${Math.ceil(totalChars / 4)} tokens exceeds safe limit ${ctxLimit - maxResp - safetyMargin}. Trimming last user message...`);
            
            const lastMsgIndex = executionMessages.length - 1;
            const lastMsg = executionMessages[lastMsgIndex];
            
            if (lastMsg?.role === "user" && typeof lastMsg.content === "string" && lastMsg.content.length > hardLimitChars * 0.5) {
                // CRITICAL FIX: Native Deep Clone (Node.js >= 17) để bảo toàn tuyệt đối tham chiếu gốc
                const clonedMsg = structuredClone(lastMsg);
                const excess = totalChars - hardLimitChars;
                const contentStr = clonedMsg.content as string;
                clonedMsg.content = contentStr.substring(0, contentStr.length - excess - 100) + "\n[...context trimmed by TokenGuard]";
                executionMessages[lastMsgIndex] = clonedMsg;
            }
        }
    }
}
