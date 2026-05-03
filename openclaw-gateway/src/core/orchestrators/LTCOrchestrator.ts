import { MemoryManager } from "../../MemoryManager";
import OpenAI from "openai";
import { logger } from "../../utils/logger";

export class LTCOrchestrator {
    #memory: MemoryManager;
    #aiRouterClient: OpenAI;
    private logger: any;

    constructor(memory: MemoryManager, routerClient: OpenAI) {
        this.#memory = memory;
        this.#aiRouterClient = routerClient;
        this.logger = logger.child({ component: 'LTCOrchestrator' });
    }

    async summarizeAndStore(userQuery: string, finalReply: string) {
        try {
            const summaryPrompt = `Extract 1 OR MAXIMUM 2 core FACTS/DECISIONS from this chat snippet. Format as brief observations (e.g., "User provided X", "Agreed to do Y"). Max 15 words. If it is just a casual greeting with no new information, respond EXACTLY with 'NONE'.\n\nUser: ${userQuery}\nLIVA: ${finalReply}`;

            const reflection = await this.#aiRouterClient.chat.completions.create({
                model: "router",
                messages: [{ role: "user", content: summaryPrompt }],
                temperature: 0.1,
            });

            const fact = reflection.choices[0].message?.content?.trim();
            if (fact && fact.length > 3 && !fact.toUpperCase().includes("NONE")) {
                this.logger.info(`[LTC Engine] Đang đúc kết quy luật vào Ký Ức Dài Hạn: ${fact.substring(0, 50)}...`);
                await this.#memory.updateLongTermMemory("Working Concepts", [fact]);
            }
        } catch (e: any) {
            this.logger.error("[LTC Engine] Không thể trích xuất Concept:", e.message);
        }
    }
}
