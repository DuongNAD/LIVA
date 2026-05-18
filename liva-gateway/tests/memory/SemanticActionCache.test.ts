import { describe, it, expect, vi, beforeEach } from "vitest";

const { mockEmbedWithTimeout } = vi.hoisted(() => ({
    mockEmbedWithTimeout: vi.fn(),
}));

vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: vi.fn().mockReturnValue({
            embedWithTimeout: mockEmbedWithTimeout,
            ensureReady: vi.fn().mockResolvedValue(undefined),
        }),
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { SemanticActionCache } from "../../src/memory/SemanticActionCache";

function createMockVector(seed: number, dim: number = 384): Float32Array {
    const vec = new Float32Array(dim);
    for (let i = 0; i < dim; i++) {
        vec[i] = Math.sin(seed * (i + 1)) * 0.5;
    }
    let norm = 0;
    for (let i = 0; i < dim; i++) norm += vec[i] * vec[i];
    norm = Math.sqrt(norm);
    for (let i = 0; i < dim; i++) vec[i] /= norm;
    return vec;
}

describe("SemanticActionCache", () => {
    let cache: SemanticActionCache;

    beforeEach(() => {
        vi.clearAllMocks();
        cache = new SemanticActionCache();
    });

    it("should return miss on empty cache", async () => {
        const result = await cache.lookup("open chrome");
        expect(result.hit).toBe(false);
        expect(result.similarity).toBe(0);
    });

    it("should record and lookup a cached action", async () => {
        const vec = createMockVector(42);
        mockEmbedWithTimeout.mockResolvedValue(Array.from(vec));

        await cache.record("mở chrome cho anh", "app_launcher", { appName: "chrome" });
        expect(cache.size).toBe(1);

        const result = await cache.lookup("mở chrome cho anh");
        expect(result.hit).toBe(true);
        expect(result.action?.toolName).toBe("app_launcher");
        expect(result.similarity).toBeGreaterThanOrEqual(0.95);
    });

    it("should miss when similarity is below threshold", async () => {
        const vec1 = createMockVector(42);
        const vec2 = createMockVector(999);

        mockEmbedWithTimeout
            .mockResolvedValueOnce(Array.from(vec1))
            .mockResolvedValueOnce(Array.from(vec2));

        await cache.record("mở chrome cho anh", "app_launcher", { appName: "chrome" });
        const result = await cache.lookup("phân tích dữ liệu kinh doanh");
        expect(result.hit).toBe(false);
    });

    it("should not cache dangerous tools", async () => {
        mockEmbedWithTimeout.mockResolvedValue(Array.from(createMockVector(1)));
        await cache.record("xóa file quan trọng đi", "delete_file", { path: "/etc" });
        expect(cache.size).toBe(0);
    });

    it("should not cache very short queries", async () => {
        mockEmbedWithTimeout.mockResolvedValue(Array.from(createMockVector(1)));
        await cache.record("hi", "chitchat", {});
        expect(cache.size).toBe(0);
    });

    it("should increment hitCount on duplicate tool+args", async () => {
        mockEmbedWithTimeout.mockResolvedValue(Array.from(createMockVector(42)));

        await cache.record("mở chrome cho anh", "app_launcher", { appName: "chrome" });
        await cache.record("bật chrome đi ngay", "app_launcher", { appName: "chrome" });

        const stats = cache.getStats();
        expect(stats.size).toBe(1);
        expect(stats.totalHits).toBeGreaterThanOrEqual(2);
    });

    it("should evict LRU entries when over capacity", async () => {
        for (let i = 0; i < 210; i++) {
            const vec = createMockVector(i * 100);
            mockEmbedWithTimeout.mockResolvedValueOnce(Array.from(vec));
            await cache.record(`unique command number ${i} is special`, "app_launcher", { id: i });
        }

        expect(cache.size).toBeLessThanOrEqual(200);
    });

    it("should handle embedding failure on lookup", async () => {
        mockEmbedWithTimeout.mockResolvedValueOnce(Array.from(createMockVector(42)));
        await cache.record("mở chrome cho anh", "app_launcher", { appName: "chrome" });

        mockEmbedWithTimeout.mockRejectedValueOnce(new Error("Embedding failed"));
        const result = await cache.lookup("test query");
        expect(result.hit).toBe(false);
    });

    it("should handle embedding failure on record", async () => {
        mockEmbedWithTimeout.mockRejectedValueOnce(new Error("Embedding failed"));
        await cache.record("test query valid length text", "app_launcher", {});
        expect(cache.size).toBe(0);
    });

    it("should clear all entries", async () => {
        mockEmbedWithTimeout.mockResolvedValue(Array.from(createMockVector(42)));
        await cache.record("mở chrome cho anh", "app_launcher", { appName: "chrome" });
        expect(cache.size).toBe(1);
        cache.clear();
        expect(cache.size).toBe(0);
    });

    it("should return diagnostic stats", async () => {
        mockEmbedWithTimeout.mockResolvedValue(Array.from(createMockVector(42)));
        await cache.record("mở chrome cho anh", "app_launcher", { appName: "chrome" });

        const stats = cache.getStats();
        expect(stats.size).toBe(1);
        expect(stats.topEntries.length).toBe(1);
        expect(stats.topEntries[0].tool).toBe("app_launcher");
    });
});
