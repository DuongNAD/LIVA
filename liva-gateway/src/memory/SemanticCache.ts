import LRUCache from 'lru-cache';
import * as levenshtein from 'fast-levenshtein';
import { logger } from '../utils/logger';

export interface SemanticCacheEntry {
    normalizedQuery: string;
    originalQuery: string;
    response: string;
    action?: any; // Shortcut action
    timestamp: number;
}

export class SemanticCache {
    private cache: LRUCache<string, SemanticCacheEntry>;
    private static readonly MAX_WORDS = 20;
    private static readonly SIMILARITY_THRESHOLD = 0.95;

    constructor(maxItems: number = 500) {
        this.cache = new LRUCache({
            max: maxItems,
            ttl: 1000 * 60 * 60 * 24, // 24 hours
        });
    }

    /**
     * Chuẩn hóa chuỗi để tăng tỷ lệ Cache Hit
     */
    public normalize(text: string): string {
        return text
            .toLowerCase()
            .replace(/[.,/#!$%^&*;:{}=\-_`~()?"']/g, "") // Loại bỏ dấu câu
            .replace(/\s{2,}/g, " ") // Xóa khoảng trắng thừa
            .trim();
    }

    /**
     * Đếm số từ trong chuỗi
     */
    private wordCount(text: string): number {
        return text.trim().split(/\s+/).length;
    }

    /**
     * Tính độ tương đồng giữa 2 chuỗi (0.0 đến 1.0)
     */
    private getSimilarity(str1: string, str2: string): number {
        if (str1 === str2) return 1.0;
        const maxLen = Math.max(str1.length, str2.length);
        if (maxLen === 0) return 1.0;
        
        const levGet = levenshtein.get || (levenshtein as any).default?.get;
        const distance = levGet(str1, str2);
        return 1.0 - (distance / maxLen);
    }

    /**
     * Tìm kết quả cache phù hợp nhất (Fuzzy Match)
     */
    public get(query: string): SemanticCacheEntry | null {
        if (this.wordCount(query) > SemanticCache.MAX_WORDS) {
            return null; // Bypass cho câu lệnh phức tạp
        }

        const normalized = this.normalize(query);
        
        // Exact match trước (O(1))
        const exactMatch = this.cache.get(normalized);
        if (exactMatch) {
            logger.info(`[SemanticCache] Exact hit for: "${query}"`);
            return exactMatch;
        }

        // Fuzzy match qua toàn bộ keys (O(N))
        // Vì max items = 500, quá trình này rất nhanh trên RAM
        let bestMatch: SemanticCacheEntry | null = null;
        let highestSimilarity = 0;

        for (const [key, entry] of this.cache.entries()) {
            const similarity = this.getSimilarity(normalized, key);
            if (similarity > highestSimilarity && similarity >= SemanticCache.SIMILARITY_THRESHOLD) {
                highestSimilarity = similarity;
                bestMatch = entry;
                
                // Nếu tìm thấy một match đủ tốt (gần 1.0), có thể break sớm để tối ưu
                if (similarity >= 0.99) break;
            }
        }

        if (bestMatch) {
            logger.info(`[SemanticCache] Fuzzy hit (${highestSimilarity.toFixed(2)}) for: "${query}" matched with "${bestMatch.originalQuery}"`);
            // Cập nhật lại thời gian truy cập (LRU) bằng cách get lại theo key gốc của cache
            this.cache.get(bestMatch.normalizedQuery);
            return bestMatch;
        }

        return null;
    }

    /**
     * Thêm vào cache
     */
    public set(query: string, response: string, action?: any): void {
        if (this.wordCount(query) > SemanticCache.MAX_WORDS) {
            return; // Không cache câu dài
        }

        const normalized = this.normalize(query);
        this.cache.set(normalized, {
            normalizedQuery: normalized,
            originalQuery: query,
            response,
            action,
            timestamp: Date.now()
        });
        
        logger.debug(`[SemanticCache] Cached: "${query}"`);
    }

    /**
     * Xóa bộ nhớ cache
     */
    public clear(): void {
        this.cache.clear();
    }
}
