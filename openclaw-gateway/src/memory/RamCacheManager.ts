/**
 * RamCacheManager — Sprint 4 Task 4.2
 *
 * Extracted from MemoryManager's in-memory `memCache: ChatMessage[]`.
 * Manages a bounded FIFO cache of recent conversation messages in RAM.
 *
 * Features:
 *   - Bounded capacity (max 200 entries, evicts oldest 100 on overflow)
 *   - Zero-disk-I/O reads (returns directly from RAM)
 *   - Cross-session warm-up injection support
 *   - GDPR purge (reset to empty)
 *
 * IMPORTANT: This class does NOT own any timers or intervals.
 * No `dispose()` needed — garbage collected when MemoryManager drops the reference.
 */

import { logger } from "../utils/logger";

// ===========================
// Types (re-exported from MemoryManager for backward compat)
// ===========================

export interface ChatMessage {
    role: "user" | "assistant" | "system";
    content: string;
    timestamp: number;
}

// ===========================
// Constants
// ===========================

const MAX_CACHE_SIZE = 200;
const EVICTION_KEEP = 100;

// ===========================
// RamCacheManager
// ===========================

export class RamCacheManager {
    #cache: ChatMessage[] = [];

    /**
     * Push a message into the RAM cache.
     * Automatically evicts the oldest entries if the cache exceeds MAX_CACHE_SIZE.
     */
    push(msg: ChatMessage): void {
        this.#cache.push(msg);

        // V14: Lưỡi Hái Tử Thần - Chống phình to lõi RAM Zalo
        if (this.#cache.length > MAX_CACHE_SIZE) {
            this.#cache = this.#cache.slice(-EVICTION_KEEP);
            logger.info(`[Memory GC] Đã chặt bỏ ${MAX_CACHE_SIZE - EVICTION_KEEP} tin nhắn cũ khỏi RAM Cache ngầm bảo vệ Zalo!`);
        }
    }

    /**
     * Get all cached messages (zero-copy reference — DO NOT mutate).
     */
    getAll(): ChatMessage[] {
        return this.#cache;
    }

    /**
     * Get the number of cached messages.
     */
    get length(): number {
        return this.#cache.length;
    }

    /**
     * Load initial cache from parsed JSONL lines.
     * Used during MemoryManager.initialize() to hydrate from disk.
     */
    hydrate(lines: string[]): void {
        try {
            this.#cache = lines.map((line) => {
                const parsed = JSON.parse(line);
                return {
                    role: parsed.role,
                    content: parsed.content,
                    timestamp: parsed.timestamp || Date.now(),
                };
            });
        } catch {
            this.#cache = [];
        }
    }

    /**
     * Inject a cross-session warm-up message.
     * Used during MemoryManager.initialize() for anti-hallucination context.
     */
    injectWarmup(content: string): void {
        this.#cache.push({
            role: "system",
            content,
            timestamp: Date.now(),
        });
    }

    /**
     * GDPR Purge — reset cache to empty.
     */
    purge(): void {
        this.#cache = [];
    }
}
