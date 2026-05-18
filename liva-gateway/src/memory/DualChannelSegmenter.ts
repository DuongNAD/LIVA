import { EmbeddingService } from "../services/EmbeddingService";
import OpenAI from "openai";
import { logger } from "../utils/logger";
import { cosineSimilarity } from "../utils/VectorMath";

/**
 * DualChannelSegmenter — Topic-Aware Episode Boundary Detection (H-MEM v18)
 * ===========================================================================
 * Replaces the mechanical batched extraction with intelligent, context-aware
 * segmentation. Uses a 3-layer filter architecture to minimize LLM token usage.
 *
 * Architecture:
 *   Channel 1 — Topic-Shift (zero-cost, pure math): Cosine similarity
 *   First-Pass Filter (zero-cost, regex heuristic): Named Entity novelty
 *   Channel 2 — Surprise (LLM call, gated): LLM judge with budget 300 tokens
 *
 * Safety Features:
 *   - Anti-ReDoS: Input sanitized to 1000 chars before complex regex
 *   - Smart Circuit Breaker: MAX_TURNS_PER_EPISODE with role-aware cutoff
 *   - Stop-words filter: Prevents Vietnamese sentence-initial capitals from triggering Ch2
 *   - First-word heuristic: Ignores single capitalized words at sentence start
 *
 * @module DualChannelSegmenter
 */

const TOPIC_SHIFT_THRESHOLD = 0.65;
const SURPRISE_TRIGGER_SCORE = 7;
const MAX_CLUSTER_EMBEDDINGS = 10;
const MAX_TURNS_PER_EPISODE = 10; // Cầu dao ngắt mạch cơ học chống Context Overflow

// Vietnamese stop-words that commonly appear capitalized at sentence start
const VIETNAMESE_STOP_WORDS = new Set([
    "Tuy", "Nhưng", "Và", "Vậy", "Thì", "Là", "Có", "Không", "Để", "Khi",
    "Hôm", "Nếu", "Vì", "Hoặc", "Với", "Trong", "Từ", "Sau", "Trước",
    "Đã", "Đang", "Sẽ", "Rất", "Cũng", "Được", "Bởi", "Theo", "Mà",
]);

export class DualChannelSegmenter {
    // AI_CONTEXT Rule 4.2: True private (#) for all background state
    readonly #embeddingService: EmbeddingService;
    readonly #aiClient: OpenAI;
    #currentClusterEmbeddings: number[][] = [];
    #recentEntities: Set<string> = new Set();

    constructor(embeddingService: EmbeddingService, aiClient: OpenAI) {
        this.#embeddingService = embeddingService;
        this.#aiClient = aiClient;
    }

    /**
     * Channel 1: Topic-shift detection via cosine distance.
     * Pure math — zero LLM cost.
     */
    async detectTopicShift(newMsgEmbedding: number[]): Promise<boolean> {
        if (this.#currentClusterEmbeddings.length === 0) {
            this.#currentClusterEmbeddings.push(newMsgEmbedding);
            return false;
        }

        const clusterAvg = this.#computeAverageEmbedding();
        const similarity = cosineSimilarity(newMsgEmbedding, clusterAvg);

        if (similarity < TOPIC_SHIFT_THRESHOLD) {
            logger.debug(`[Segmenter/Ch1] Topic shift detected (sim=${similarity.toFixed(3)})`);
            return true;
        }

        // Add to current cluster (sliding window)
        this.#currentClusterEmbeddings.push(newMsgEmbedding);
        if (this.#currentClusterEmbeddings.length > MAX_CLUSTER_EMBEDDINGS) {
            this.#currentClusterEmbeddings.shift();
        }
        return false;
    }

    /**
     * First-Pass Filter: Entity-based heuristic to prevent excessive LLM calls.
     * Detects capitalized words, tech terms, URLs, @mentions, etc.
     */
    #hasNewEntities(newMsg: string): boolean {
        // Anti-ReDoS Sanitization: Limit length before applying complex Unicode Regex
        const safeMsg = smartTruncate(newMsg, 1000);
        const extracted = this.#extractBasicEntities(safeMsg);
        const hasNovelty = extracted.some(e => !this.#recentEntities.has(e));
        extracted.forEach(e => this.#recentEntities.add(e));
        // Cap recent entities set to prevent unbounded growth
        if (this.#recentEntities.size > 200) {
            const arr = [...this.#recentEntities];
            this.#recentEntities = new Set(arr.slice(-100));
        }
        return hasNovelty;
    }

    /**
     * I18n: Unicode regex to capture Vietnamese proper nouns ("Hà Nội", "Tiến Đạt").
     * Includes Stop-words Filter and First-Word Heuristic Gap fix.
     */
    #extractBasicEntities(text: string): string[] {
        // Regex bắt Capitalized Words (including Vietnamese diacritics)
        const rawCapitalWords = text.match(/(?:\b\p{Lu}\p{L}*(?:\s+\p{Lu}\p{L}*)*\b)/gu) || [];

        // The First-Word Heuristic Gap & Stop-words Filter
        const capitalWords = rawCapitalWords.filter(word => {
            if (VIETNAMESE_STOP_WORDS.has(word)) return false;

            // Bỏ qua chữ viết hoa đơn độc nếu nó đứng ở đầu câu (vị trí 0 hoặc sau dấu chấm)
            // Chỉ giữ lại khi nó là từ ghép (có dấu cách, vd "Tiến Đạt") hoặc nằm giữa câu.
            const isSingleWord = !word.includes(" ");
            const index = text.indexOf(word);
            const isAtSentenceStart = index === 0 || (index > 2 && text.substring(index - 2, index).match(/[.?!]\s/));

            if (isSingleWord && isAtSentenceStart) return false;
            return true;
        });

        const techTerms = text.match(
            /\b(?:API|SDK|GPU|VRAM|LLM|gRPC|Docker|Python|TypeScript|React|Node|Spring Boot|JavaFX|SQLite|sqlite-vec)\b/gi
        ) || [];
        const urls = text.match(/https?:\/\/\S+/g) || [];
        return [...new Set([...capitalWords, ...techTerms, ...urls])];
    }

    /**
     * Channel 2: Surprise detection via LLM judge.
     * Only triggered when first-pass filter detects novel entities.
     * Budget: 300 tokens max.
     */
    async detectSurprise(newMsg: string, recentContext: string): Promise<number> {
        if (!this.#hasNewEntities(newMsg)) {
            return 0; // Low novelty → bypass LLM, save tokens
        }

        try {
            const response = await this.#aiClient.chat.completions.create({
                model: "router",
                messages: [{
                    role: "system",
                    content: `Rate how SURPRISING this new message is compared to the recent context.
Score 1-10 (1=expected continuation, 10=completely new/contradictory information).
Reply with ONLY a single integer.`
                }, {
                    role: "user",
                    // Expanded context to 800 chars (head + tail) to retain stack traces / code blocks
                    content: `Recent context: ${smartTruncate(recentContext, 800)}
New message: ${smartTruncate(newMsg, 800)}`
                }],
                temperature: 0.0,
                max_tokens: 5,
            });

            const score = parseInt(
                response.choices[0]?.message?.content?.trim() || "0"
            );
            if (score >= SURPRISE_TRIGGER_SCORE) {
                logger.debug(`[Segmenter/Ch2] Surprise detected (score=${score})`);
            }
            return isNaN(score) ? 0 : score;
        } catch {
            return 0; // Fail-safe: never block on surprise detection
        }
    }

    /**
     * Determines if a new episode boundary should be created.
     * Integrates both channels for a final decision.
     * @param role The role of the message sender ('user' or 'ai')
     */
    async shouldCreateNewEpisode(
        newMsg: string,
        newMsgEmbedding: number[],
        recentContext: string,
        role: 'user' | 'ai'
    ): Promise<boolean> {
        // Smart Circuit Breaker: Ngắt mạch an toàn, không cắt ngang cặp Question-Answer
        if (this.#currentClusterEmbeddings.length >= MAX_TURNS_PER_EPISODE && role === 'ai') {
            return true;
        }

        // Channel 1: Pure math (always runs)
        const isTopicShifted = await this.detectTopicShift(newMsgEmbedding);
        if (isTopicShifted) return true;

        // Channel 2: LLM judge (only if entity first-pass filter passes)
        const surpriseScore = await this.detectSurprise(newMsg, recentContext);
        return surpriseScore >= SURPRISE_TRIGGER_SCORE;
    }

    /** Reset cluster when new episode starts */
    public resetCluster(seedEmbedding: number[]): void {
        this.#currentClusterEmbeddings = [seedEmbedding];
    }

    // --- Math helpers (true private) ---

    #computeAverageEmbedding(): number[] {
        const dim = this.#currentClusterEmbeddings[0].length;
        const avg = new Array(dim).fill(0);
        for (const emb of this.#currentClusterEmbeddings) {
            for (let i = 0; i < dim; i++) avg[i] += emb[i];
        }
        const n = this.#currentClusterEmbeddings.length;
        for (let i = 0; i < dim; i++) avg[i] /= n;
        return avg;
    }
}

/** Utility: Smart truncation — keeps head (context) + tail (conclusion) */
export function smartTruncate(text: string, maxLen: number): string {
    if (text.length <= maxLen) return text;
    const half = Math.floor(maxLen / 2);
    return text.substring(0, half) + " [...] " + text.substring(text.length - half);
}
