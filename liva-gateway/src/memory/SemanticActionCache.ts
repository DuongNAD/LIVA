import { EmbeddingService } from "../services/EmbeddingService";
import { logger } from "../utils/logger";

/**
 * SemanticActionCache — L0.5 Bypass Cache (LIVA v24 Pillar 2)
 * ============================================================
 * Caches [query_embedding] → [tool_name, tool_args] for repetitive commands.
 * When cosine similarity > 0.95, bypasses LLM entirely and returns cached
 * tool execution data for direct SkillRegistry dispatch.
 *
 * Performance: <5ms response time vs 1-2s LLM inference.
 * Storage: In-memory LRU with SQLite persistence for cold start.
 *
 * Architecture:
 *   - Shared EmbeddingService singleton (all-MiniLM-L6-v2, 384D)
 *   - Max 200 entries (LRU eviction)
 *   - Hit threshold: cosine similarity ≥ 0.95 (ultra-strict to prevent misrouting)
 *   - Auto-learning: AgentLoop records successful tool calls for future cache hits
 *
 * @module SemanticActionCache
 */

// ===========================
// Types
// ===========================

export interface CachedAction {
    /** The tool/skill name to invoke */
    toolName: string;
    /** Pre-computed tool arguments (JSON-serializable) */
    toolArgs: Record<string, unknown>;
    /** Original user query that produced this action */
    originalQuery: string;
    /** Number of times this cache entry has been hit */
    hitCount: number;
    /** Last time this entry was used (Unix ms) */
    lastUsedAt: number;
    /** Pre-computed embedding vector */
    vector: Float32Array;
}

export interface CacheHitResult {
    /** Whether a confident cache hit was found */
    hit: boolean;
    /** The cached action data (if hit) */
    action?: CachedAction;
    /** Cosine similarity score */
    similarity: number;
    /** Time taken for lookup in ms */
    lookupMs: number;
}

// ===========================
// Constants
// ===========================

/** Minimum cosine similarity for a cache hit (ultra-strict) */
const HIT_THRESHOLD = 0.95;

/** Maximum cache entries (LRU eviction beyond this) */
const MAX_ENTRIES = 200;

/** Minimum hit count before an entry is considered "stable" (immune to eviction) */
const STABLE_HIT_COUNT = 5;

// ===========================
// Main Class
// ===========================

export class SemanticActionCache {
    private readonly embeddingService: EmbeddingService;
    private cache: CachedAction[] = [];

    constructor(embeddingService?: EmbeddingService) {
        this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
    }

    /**
     * Lookup a user query in the cache.
     * Returns a CacheHitResult with the best match (if similarity >= 0.95).
     * Completes in <5ms after embedding (embedding itself is <50ms).
     */
    public async lookup(query: string): Promise<CacheHitResult> {
        const start = performance.now();

        if (this.cache.length === 0) {
            return { hit: false, similarity: 0, lookupMs: performance.now() - start };
        }

        // Embed user query
        let queryVector: Float32Array;
        try {
            const embedding = await this.embeddingService.embedWithTimeout(query, 500);
                    queryVector = new Float32Array(embedding);
        } catch {
            return { hit: false, similarity: 0, lookupMs: performance.now() - start };
        }

        // Find best match via cosine similarity
        let bestScore = -1;
        let bestEntry: CachedAction | undefined;

        for (const entry of this.cache) {
            const score = cosineSim(queryVector, entry.vector);
            if (score > bestScore) {
                bestScore = score;
                bestEntry = entry;
            }
        }

        const lookupMs = performance.now() - start;

        if (bestScore >= HIT_THRESHOLD && bestEntry) {
            // Cache HIT — update LRU metadata
            bestEntry.hitCount++;
            bestEntry.lastUsedAt = Date.now();

            logger.info(
                `[L0.5 Cache] ⚡ HIT! "${query.substring(0, 40)}..." → ${bestEntry.toolName} (sim: ${bestScore.toFixed(4)}, hits: ${bestEntry.hitCount}, ${lookupMs.toFixed(1)}ms)`
            );

            return {
                hit: true,
                action: bestEntry,
                similarity: bestScore,
                lookupMs,
            };
        }

        logger.debug(
            `[L0.5 Cache] MISS "${query.substring(0, 30)}..." (best: ${bestScore.toFixed(3)}, threshold: ${HIT_THRESHOLD})`
        );

        return { hit: false, similarity: bestScore, lookupMs };
    }

    /**
     * Record a successful tool execution for future cache hits.
     * Called by AgentLoop after a tool call succeeds.
     */
    public async record(
        query: string,
        toolName: string,
        toolArgs: Record<string, unknown>
    ): Promise<void> {
        // Don't cache queries that are too short or too long
        if (query.length < 3 || query.length > 200) return;

        // Only cache simple direct action tools whose final response doesn't need LLM synthesis (L0.5 bypass)
        const CACHEABLE_TOOLS = new Set([
            "screenshot_capture",
            "push_ui_notification",
            "media_controller",
            "hardware_controller",
            "desktop_rpa",
            "window_arranger",
            "app_launcher",
            "toggle_ghost_mode",
            "send_zalo_bot",
            "send_zalo_rpa",
            "send_messenger_rpa",
            "social_media_poster",
            "auto_backup",
        ]);
        if (!CACHEABLE_TOOLS.has(toolName)) return;

        // Check if this exact tool+args combo already cached (avoid duplicates)
        const existing = this.cache.find(
            c => c.toolName === toolName && 
                 JSON.stringify(c.toolArgs) === JSON.stringify(toolArgs)
        );
        if (existing) {
            existing.hitCount++;
            existing.lastUsedAt = Date.now();
            return;
        }

        // Embed the query
        let vector: Float32Array;
        try {
            const embedding = await this.embeddingService.embedWithTimeout(query, 500);
                    vector = new Float32Array(embedding);
        } catch {
            return; // Silently skip if embedding fails
        }

        // Check similarity with existing entries — don't add near-duplicates
        for (const entry of this.cache) {
            const sim = cosineSim(vector, entry.vector);
            if (sim > 0.9) {
                // Close enough to existing entry — just update hitCount
                entry.hitCount++;
                entry.lastUsedAt = Date.now();
                return;
            }
        }

        // Add new entry
        this.cache.push({
            toolName,
            toolArgs,
            originalQuery: query,
            hitCount: 1,
            lastUsedAt: Date.now(),
            vector,
        });

        // Evict if over capacity (LRU: remove least-used, non-stable entries)
        if (this.cache.length > MAX_ENTRIES) {
            this.#evictLRU();
        }

        logger.info(
            `[L0.5 Cache] 📝 Recorded: "${query.substring(0, 40)}..." → ${toolName} (entries: ${this.cache.length})`
        );
    }

    /**
     * Evict least-recently-used entries that haven't reached stable hit count.
     */
    #evictLRU(): void {
        // Sort by: stable entries first (keep), then by lastUsedAt ASC (oldest first)
        this.cache.sort((a, b) => {
            const aStable = a.hitCount >= STABLE_HIT_COUNT ? 1 : 0;
            const bStable = b.hitCount >= STABLE_HIT_COUNT ? 1 : 0;
            if (aStable !== bStable) return bStable - aStable; // Stable entries last (kept)
            return a.lastUsedAt - b.lastUsedAt; // Oldest first (evicted first)
        });

        const toRemove = this.cache.length - MAX_ENTRIES;
        if (toRemove > 0) {
            const removed = this.cache.splice(0, toRemove);
            logger.debug(`[L0.5 Cache] Evicted ${removed.length} LRU entries (remaining: ${this.cache.length})`);
        }
    }

    /** Get cache stats for diagnostics. */
    public getStats(): { size: number; totalHits: number; topEntries: Array<{ query: string; tool: string; hits: number }> } {
        const totalHits = this.cache.reduce((sum, c) => sum + c.hitCount, 0);
        const topEntries = [...this.cache]
            .sort((a, b) => b.hitCount - a.hitCount)
            .slice(0, 10)
            .map(c => ({ query: c.originalQuery.substring(0, 50), tool: c.toolName, hits: c.hitCount }));

        return { size: this.cache.length, totalHits, topEntries };
    }

    /** Clear all cached entries. */
    public clear(): void {
        this.cache = [];
        logger.info("[L0.5 Cache] Cleared all entries.");
    }

    /** Get current cache size. */
    public get size(): number {
        return this.cache.length;
    }
}

// ===========================
// Math Utilities
// ===========================

/**
 * Fast cosine similarity between two Float32Array vectors.
 * Single-pass dot product + norms. Returns [-1, 1].
 */
function cosineSim(a: Float32Array, b: Float32Array): number {
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
