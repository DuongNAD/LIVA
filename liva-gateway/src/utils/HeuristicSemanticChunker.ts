import { EmbeddingService } from "../services/EmbeddingService";
import { cosineSimilarity } from "./VectorMath";
import { logger } from "./logger";

export class HeuristicSemanticChunker {
    /**
     * Splits a text into semantic chunks.
     * Heuristic-first: cuts text into sentences/paragraphs first.
     * Only calls EmbeddingService when chunk size exceeds softLimit (150 tokens).
     * Hard limits at hardLimit (256 tokens) to prevent ONNX sequence truncation.
     */
    public static async chunk(
        text: string, 
        embeddingService: EmbeddingService, 
        softLimit: number = 150, 
        hardLimit: number = 256
    ): Promise<string[]> {
        if (!text || text.trim().length === 0) return [];

        // 1. Cut into sentences/paragraphs using regex
        const rawSentences = text.split(/(?<=[.?!])\s+|\n+/);
        const sentences = rawSentences.map(s => s.trim()).filter(s => s.length > 0);

        if (sentences.length === 0) return [];

        const chunks: string[] = [];
        let currentChunkSentences: string[] = [];
        let currentChunkTokens = 0;
        // [Perf] Cache the embedding of the current chunk so we don't re-embed it
        // on every boundary. Invalidated (null) whenever the chunk text grows via
        // append; on a topic-shift split the freshly-computed sentence vector is
        // carried forward as the new chunk's seed vector (avoids one re-embed).
        let currentChunkVec: number[] | null = null;

        const getApproxTokens = (t: string) => {
            const words = t.trim().split(/\s+/).filter(w => w.length > 0).length;
            return Math.ceil(words * 1.5);
        };

        for (const sentence of sentences) {
            const sentenceTokens = getApproxTokens(sentence);

            // If appending the sentence exceeds the hardLimit, we must finalize the current chunk first
            if (currentChunkSentences.length > 0 && currentChunkTokens + sentenceTokens > hardLimit) {
                chunks.push(currentChunkSentences.join(" "));
                currentChunkSentences = [sentence];
                currentChunkTokens = sentenceTokens;
                currentChunkVec = null; // new chunk seed has no embedding yet
                continue;
            }

            // If we are below the softLimit, we simply append (Heuristic-First)
            if (currentChunkTokens < softLimit) {
                currentChunkSentences.push(sentence);
                currentChunkTokens += sentenceTokens;
                currentChunkVec = null; // chunk grew → cached vector is stale
                continue;
            }

            // If we are between softLimit and hardLimit, we check for semantic Topic Shift
            try {
                const currentChunkText = currentChunkSentences.join(" ");
                // Reuse cached chunk vector when warm; otherwise embed both in
                // parallel (Promise.all also guarantees both are awaited in this
                // try block — no dangling rejected promise on embed failure).
                let vec1: number[];
                let vec2: number[];
                if (currentChunkVec) {
                    vec1 = currentChunkVec;
                    vec2 = await embeddingService.embed(sentence);
                } else {
                    [vec1, vec2] = await Promise.all([
                        embeddingService.embed(currentChunkText),
                        embeddingService.embed(sentence),
                    ]);
                }

                const similarity = cosineSimilarity(vec1, vec2);
                if (similarity < 0.65) {
                    // Topic shift detected -> split!
                    logger.debug(`[HeuristicSemanticChunker] Topic shift detected (similarity=${similarity.toFixed(3)}). Splitting chunk.`);
                    chunks.push(currentChunkText);
                    currentChunkSentences = [sentence];
                    currentChunkTokens = sentenceTokens;
                    currentChunkVec = vec2; // carry forward: sentence becomes new chunk seed
                } else {
                    // Same topic -> append
                    currentChunkSentences.push(sentence);
                    currentChunkTokens += sentenceTokens;
                    currentChunkVec = null; // chunk grew → cached vector is stale
                }
            } catch (err: unknown) {
                // Fail-safe: if embedding fails, fall back to simple greedy append
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[HeuristicSemanticChunker] Embedding failed, falling back to greedy append: ${msg}`);
                currentChunkSentences.push(sentence);
                currentChunkTokens += sentenceTokens;
                currentChunkVec = null;
            }
        }

        // Add the remaining sentences if any
        if (currentChunkSentences.length > 0) {
            chunks.push(currentChunkSentences.join(" "));
        }

        return chunks;
    }
}
