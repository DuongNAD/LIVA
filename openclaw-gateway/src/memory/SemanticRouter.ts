import { EmbeddingService } from "../services/EmbeddingService";
import { cosineSimilarityF32 } from "../utils/VectorMath";
import { logger } from "../utils/logger";

/**
 * SemanticRouter — Cosine Similarity Intent Router (<100ms)
 * ==========================================================
 * Routes user queries to the appropriate memory retrieval tier
 * WITHOUT calling LLM. Uses pre-computed route vectors and cosine
 * similarity for near-instant classification.
 *
 * Routes:
 *   - chitchat:        Social greetings → skip heavy RAG, minimal context
 *   - factual_recall:  Specific info retrieval → L2 Vector Search
 *   - deep_reasoning:  Complex analysis → Full pipeline (L1+L2+L3)
 *   - system_command:  Direct action/skill → bypass RAG, straight to SkillRegistry
 *
 * Architecture:
 *   - Shared EmbeddingService singleton (all-MiniLM-L6-v2, 384D)
 *   - Pre-computed route anchor vectors (computed once at init)
 *   - Cosine similarity threshold: 0.45 (fallback to deep_reasoning)
 *   - Zero LLM calls — pure vector math
 *
 * @module SemanticRouter
 */

// ===========================
// Types
// ===========================

export type MemoryRoute = "chitchat" | "factual_recall" | "deep_reasoning" | "system_command" | "tool_recall";
export type SkillKit = "OBSIDIAN_KIT" | "DATA_KIT" | "DEVOPS_KIT" | "SOCIAL_KIT" | "GENERAL_KIT" | null;

export interface RouteResult {
    route: MemoryRoute;
    confidence: number;
    activeKit?: SkillKit;
}

interface RouteAnchor {
    route: MemoryRoute | NonNullable<SkillKit>;
    vectors: Float32Array[];  // Multiple anchor vectors per route for coverage
}

// ===========================
// Route Definitions (Vietnamese + English)
// ===========================

/**
 * Each route has multiple representative utterances.
 * At init time, each utterance is embedded into a 384D vector.
 * At query time, the user's query is embedded and compared against all anchors.
 */
const ROUTE_UTTERANCES: Record<MemoryRoute, string[]> = {
    chitchat: [
        "chào bạn",
        "xin chào",
        "hello",
        "hi",
        "tạm biệt",
        "cảm ơn bạn nhé",
        "bạn khỏe không",
        "hôm nay thế nào",
        "good morning",
        "thank you",
        "bye bye",
        "bạn tên gì",
        "kể chuyện cười đi",
        "bạn có vui không",
    ],
    factual_recall: [
        "ai là người",
        "cái gì",
        "ở đâu",
        "bao giờ",
        "cho tôi biết",
        "tra cứu thông tin",
        "tìm kiếm",
        "nhớ lại",
        "thông tin về",
        "tôi đã nói gì",
        "hôm qua tôi làm gì",
        "mẹ tôi tên gì",
        "lịch sử cuộc trò chuyện",
        "what is",
        "who is",
        "when did",
        "tell me about",
    ],
    deep_reasoning: [
        "tại sao",
        "giải thích cho tôi",
        "phân tích",
        "so sánh",
        "viết code",
        "tạo kế hoạch",
        "lập trình",
        "thiết kế hệ thống",
        "đánh giá",
        "why does",
        "explain how",
        "write a program",
        "analyze this",
        "create a plan",
        "review this code",
        "giúp tôi debug",
        "nghiên cứu về",
    ],
    system_command: [
        "chụp màn hình",
        "tắt nhạc",
        "bật nhạc",
        "xóa file",
        "mở file",
        "dọn dẹp bộ nhớ",
        "dừng lại",
        "thoát",
        "chạy lệnh",
        "gửi tin nhắn zalo",
        "gửi email",
        "tìm kiếm trên web",
        "mở trình duyệt",
        "đọc file",
        "ghi file",
        "execute command",
        "take screenshot",
        "send message",
        "open browser",
        "search the web",
    ],
    // [v4.0] G-3: Tool recall route — re-invoke previous actions
    tool_recall: [
        "dùng lại tool hôm qua",
        "chạy lại lệnh đó",
        "lần trước dùng gì",
        "repeat last action",
        "do it again",
        "run that again",
        "làm lại cái vừa nãy",
        "thử lại đi",
    ],
};

const KIT_UTTERANCES: Record<NonNullable<SkillKit>, string[]> = {
    OBSIDIAN_KIT: [
        "tạo note obsidian",
        "ghi chép vào vault",
        "tìm kiếm backlink",
        "liên kết ghi chú",
        "quản lý note",
        "tạo file markdown obsidian"
    ],
    DATA_KIT: [
        "phân tích file dữ liệu",
        "đọc file csv",
        "xử lý excel",
        "thống kê data",
        "parse báo cáo pdf",
        "tổng hợp số liệu báo cáo",
        "phân tích bảng dữ liệu"
    ],
    DEVOPS_KIT: [
        "kiểm tra git status",
        "tạo commit và push",
        "chạy docker container",
        "xem file log hệ thống",
        "phân tích exception log",
        "chạy test coverage",
        "quản lý server"
    ],
    SOCIAL_KIT: [
        "lên lịch họp google calendar",
        "đăng bài lên linkedin",
        "tạo task jira",
        "cập nhật linear ticket",
        "gửi thông báo mạng xã hội"
    ],
    GENERAL_KIT: [
        "tìm kiếm web",
        "mở trình duyệt",
        "chạy lệnh terminal",
        "đọc nội dung file",
        "chụp màn hình",
        "tính toán bình thường",
        "gửi tin nhắn"
    ]
};

// ===========================
// Constants
// ===========================

/**
 * [P1-2.2] Adaptive confidence threshold based on query length.
 * all-MiniLM-L6-v2 produces lower cosine scores for short Vietnamese text,
 * so we relax the threshold for short queries and tighten for long ones.
 */
function getConfidenceThreshold(queryLength: number): number {
    if (queryLength < 20) return 0.35;   // Short queries ("tại sao?") — relax
    if (queryLength > 100) return 0.55;  // Long queries — stricter
    return 0.45;                         // Default
}

/** Default route when confidence is too low */
const FALLBACK_ROUTE: MemoryRoute = "deep_reasoning";

// ===========================
// Main Class
// ===========================

export class SemanticRouter {
    private readonly embeddingService: EmbeddingService;
    private routeAnchors: RouteAnchor[] = [];
    private kitAnchors: RouteAnchor[] = [];
    private isInitialized = false;
    private initPromise: Promise<void> | null = null;

    constructor(embeddingService?: EmbeddingService) {
        this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
    }

    /**
     * Initialize route vectors. Safe to call multiple times (Promise Lock).
     * Pre-embeds all route utterances into anchor vectors.
     */
    public async initialize(): Promise<void> {
        if (this.isInitialized) return;
        if (this.initPromise) return this.initPromise;
        this.initPromise = this._buildAnchors();
        return this.initPromise;
    }

    private async _buildAnchors(): Promise<void> {
        try {
            await this.embeddingService.ensureReady();

            // ⚡ [P0-2.1] Batch embedding — collect all utterances into a flat array,
            // embed in a single pipeline call, then reassemble into route anchor structures.
            // Before: 60 sequential embed() calls ≈ 3s
            // After:  2 batch embedBatch() calls ≈ 300ms (10x speedup)

            // --- Route Anchors (batch) ---
            const routeEntries = Object.entries(ROUTE_UTTERANCES);
            const allRouteTexts: string[] = [];
            const routeMapping: Array<{ routeName: MemoryRoute; startIdx: number; count: number }> = [];

            for (const [routeName, utterances] of routeEntries) {
                routeMapping.push({
                    routeName: routeName as MemoryRoute,
                    startIdx: allRouteTexts.length,
                    count: utterances.length,
                });
                allRouteTexts.push(...utterances);
            }

            const routeEmbeddings = await this.embeddingService.embedBatch(allRouteTexts);

            for (const mapping of routeMapping) {
                const vectors: Float32Array[] = [];
                for (let i = 0; i < mapping.count; i++) {
                    const vec = routeEmbeddings[mapping.startIdx + i];
                    if (vec && vec.some(v => v !== 0.01)) { // Skip dummy vectors
                        vectors.push(new Float32Array(vec));
                    }
                }
                if (vectors.length > 0) {
                    this.routeAnchors.push({ route: mapping.routeName, vectors });
                }
            }

            // --- Kit Anchors (batch) ---
            const kitEntries = Object.entries(KIT_UTTERANCES);
            const allKitTexts: string[] = [];
            const kitMapping: Array<{ kitName: NonNullable<SkillKit>; startIdx: number; count: number }> = [];

            for (const [kitName, utterances] of kitEntries) {
                kitMapping.push({
                    kitName: kitName as NonNullable<SkillKit>,
                    startIdx: allKitTexts.length,
                    count: utterances.length,
                });
                allKitTexts.push(...utterances);
            }

            const kitEmbeddings = await this.embeddingService.embedBatch(allKitTexts);

            for (const mapping of kitMapping) {
                const vectors: Float32Array[] = [];
                for (let i = 0; i < mapping.count; i++) {
                    const vec = kitEmbeddings[mapping.startIdx + i];
                    if (vec && vec.some(v => v !== 0.01)) {
                        vectors.push(new Float32Array(vec));
                    }
                }
                if (vectors.length > 0) {
                    this.kitAnchors.push({ route: mapping.kitName, vectors });
                }
            }

            this.isInitialized = true;
            const totalAnchors = this.routeAnchors.reduce((sum, r) => sum + r.vectors.length, 0);
            const totalKitAnchors = this.kitAnchors.reduce((sum, r) => sum + r.vectors.length, 0);
            logger.info(`[SemanticRouter] ✅ Initialized with ${totalAnchors} route anchors and ${totalKitAnchors} kit anchors (batch mode).`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[SemanticRouter] ❌ Init failed: ${errMsg}`);
            // Mark as initialized anyway — route() will use fallback
            this.isInitialized = true;
        }
    }

    /**
     * Route a user query to the best memory tier.
     * Returns route name + confidence score.
     * Completes in <100ms after initialization.
     */
    public async route(query: string): Promise<RouteResult> {
        await this.initialize();

        // ===========================
        // LAYER 1: Regex Fast-Track (<1ms)
        // Compensates for all-MiniLM-L6-v2's weakness with short Vietnamese text.
        // Catches obvious keyword patterns before hitting the embedding model.
        // ===========================
        const fastTrack = this.regexFastTrack(query);
        if (fastTrack) return fastTrack;

        // ===========================
        // LAYER 2: Cosine Similarity (embedding-based)
        // ===========================

        // If no anchors loaded (embedding failed), always fallback
        if (this.routeAnchors.length === 0) {
            return { route: FALLBACK_ROUTE, confidence: 0 };
        }

        // Embed the user query
        let queryVector: Float32Array;
        try {
            const embedding = await this.embeddingService.embedWithTimeout(query, 500);
            queryVector = new Float32Array(embedding);
        } catch {
            return { route: FALLBACK_ROUTE, confidence: 0 };
        }

        // Find best matching route via max cosine similarity
        let bestRoute: MemoryRoute = FALLBACK_ROUTE;
        let bestScore = -1;

        for (const anchor of this.routeAnchors) {
            for (const anchorVec of anchor.vectors) {
                const score = cosineSimilarity(queryVector, anchorVec);
                if (score > bestScore) {
                    bestScore = score;
                    bestRoute = anchor.route;
                }
            }
        }

        // Apply adaptive confidence threshold — if too low, fallback to deep_reasoning
        const confidenceThreshold = getConfidenceThreshold(query.length);
        if (bestScore < confidenceThreshold) {
            logger.debug(`[SemanticRouter] Low confidence (${bestScore.toFixed(3)} < ${confidenceThreshold}) for "${query.substring(0, 30)}..." → fallback to ${FALLBACK_ROUTE}`);
            // Also no confident activeKit
            return { route: FALLBACK_ROUTE, confidence: bestScore, activeKit: "GENERAL_KIT" };
        }

        // ===========================
        // KIT Classification
        // ===========================
        let bestKit: NonNullable<SkillKit> = "GENERAL_KIT";
        let bestKitScore = -1;
        for (const anchor of this.kitAnchors) {
            for (const anchorVec of anchor.vectors) {
                const score = cosineSimilarity(queryVector, anchorVec);
                if (score > bestKitScore) {
                    bestKitScore = score;
                    bestKit = anchor.route as NonNullable<SkillKit>;
                }
            }
        }
        
        // If the query is related to data analysis, kit detection should be confident enough.
        // We use a lower threshold for kits to ensure we capture intents, default to GENERAL_KIT
        const KIT_THRESHOLD = 0.40;
        const activeKit = bestKitScore >= KIT_THRESHOLD ? bestKit : "GENERAL_KIT";

        logger.debug(`[SemanticRouter] Routed "${query.substring(0, 30)}..." → ${bestRoute} (confidence: ${bestScore.toFixed(3)}, kit: ${activeKit}@${bestKitScore.toFixed(3)})`);
        return { route: bestRoute, confidence: bestScore, activeKit };
    }

    /**
     * Regex Fast-Track — keyword-based instant classification (<1ms).
     * Compensates for all-MiniLM-L6-v2's reduced accuracy on short Vietnamese text.
     * Returns null if no confident keyword match (falls through to embedding layer).
     */
    private regexFastTrack(query: string): RouteResult | null {
        const q = query.trim().toLowerCase();
        if (!q) return null;

        // Chitchat — greetings, social, pleasantries
        if (/^(chào|xin chào|hello|hi\b|hey\b|tạm biệt|bye|cảm ơn|thank|khỏe không|good morning|good night)/i.test(q)) {
            return { route: "chitchat", confidence: 1.0 };
        }

        // System command — direct actions, tool invocations
        if (/^(xóa|bật|tắt|dừng|mở |thoát|chụp|gửi |ghi |đọc |chạy lệnh|execute|open |send |take |search the|screenshot)/i.test(q)) {
            return { route: "system_command", confidence: 1.0 };
        }

        // No confident keyword match → fall through to embedding layer
        return null;
    }

    /**
     * Get all available routes (for diagnostics).
     */
    public getRoutes(): MemoryRoute[] {
        return this.routeAnchors.map(a => a.route);
    }

    /**
     * Check if router is ready.
     */
    public get ready(): boolean {
        return this.isInitialized;
    }
}

// ===========================
// Math Utilities
// ===========================

/**
 * Cosine similarity between two Float32Array vectors.
 * Returns value in [-1, 1]. Higher = more similar.
 * Optimized: single-pass dot product + norms.
 */
function cosineSimilarity(a: Float32Array, b: Float32Array): number {
    if (a.length !== b.length) return 0;

    let dot = 0;
    let normA = 0;
    let normB = 0;

    for (let i = 0; i < a.length; i++) {
        dot += a[i] * b[i];
        normA += a[i] * a[i];
        normB += b[i] * b[i];
    }

    if (normA === 0 || normB === 0) return 0;
    return dot / (Math.sqrt(normA) * Math.sqrt(normB));
}
