import { jsonrepair } from "jsonrepair";
import { z } from "zod";
import LRUCache from "lru-cache";
import { EmbeddingService } from "../services/EmbeddingService";
import { logger } from "../utils/logger";
import { ObsidianVaultManager } from "./ObsidianVaultManager";

export const GraphEntitySchema = z.object({
    entities: z.array(z.object({
        name: z.string(),
        type: z.string(),
        description: z.string()
    })),
    relationships: z.array(z.object({
        source: z.string(),
        target: z.string(),
        relation: z.string()
    }))
});

export type GraphEntities = z.infer<typeof GraphEntitySchema>;

export class GraphWeaverDaemon {
    readonly #SIMILARITY_THRESHOLD = 0.92;
    // 🔒 [Audit C-6] Bounded cache to prevent OOM in 24/7 daemon
    readonly #existingEntities: LRUCache<string, number[]> = new LRUCache({ max: 5000, ttl: 1000 * 60 * 60 * 24 });
    
    constructor(private vaultManager: ObsidianVaultManager) {}

    /**
     * BẮT BUỘC: Defensive Parsing (jsonrepair + Zod)
     */
    public parseLLMOutput(rawText: string): GraphEntities | null {
        try {
            const first = rawText.indexOf('{');
            if (first === -1) return null;
            
            let toParse = rawText.substring(first);
            // Loại bỏ markdown codeblock đóng nếu có
            toParse = toParse.replace(/```[a-z]*\s*$/i, '');
            toParse = toParse.replace(/`+$/, '').trim();
            const jsonStr = jsonrepair(toParse);
            const parsed = JSON.parse(jsonStr);
            
            // Validate bằng Zod
            return GraphEntitySchema.parse(parsed);
        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            logger.error({ err: errMsg }, "GraphWeaver: Lỗi parse LLM JSON Output");
            return null;
        }
    }

    /**
     * Nạp dữ liệu mô phỏng cho DB hiện tại (Dùng cho Testing)
     */
    public seedExistingEntity(name: string, embedding: number[]) {
        this.#existingEntities.set(name, embedding);
    }

    /**
     * BẮT BUỘC: Semantic Entity Disambiguation (Khử rác Entity)
     */
    public async disambiguateEntity(entityName: string): Promise<string> {
        const embeddingService = EmbeddingService.getInstance();
        const newEmbedding = await embeddingService.embed(entityName);
        
        let bestMatch = entityName;
        let highestScore = 0;

        for (const [existingName, existingEmbedding] of this.#existingEntities.entries()) {
            const score = this.#cosineSimilarity(newEmbedding, existingEmbedding);
            if (score > highestScore) {
                highestScore = score;
                bestMatch = existingName;
            }
        }

        if (highestScore > this.#SIMILARITY_THRESHOLD) {
            logger.info({ original: entityName, matched: bestMatch, score: highestScore }, "GraphWeaver: Bẻ lái Entity (Alias)");
            // Nếu Entity giống hệt nhau (Score ~ 1.0) thì không cần Alias
            if (highestScore >= 0.99) return bestMatch;
            
            // Obsidian Alias format: Bài gốc|Alias
            return `${bestMatch}|${entityName}`;
        }

        // Đăng ký Entity mới
        this.#existingEntities.set(entityName, newEmbedding);
        return entityName;
    }

    #cosineSimilarity(vecA: number[], vecB: number[]): number {
        let dotProduct = 0;
        let normA = 0;
        let normB = 0;
        for (let i = 0; i < vecA.length; i++) {
            dotProduct += vecA[i] * vecB[i];
            normA += vecA[i] * vecA[i];
            normB += vecB[i] * vecB[i];
        }
        if (normA === 0 || normB === 0) return 0;
        return dotProduct / (Math.sqrt(normA) * Math.sqrt(normB));
    }
}
