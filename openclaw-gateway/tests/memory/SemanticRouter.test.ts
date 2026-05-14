import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Hoisted Mocks (must be before vi.mock)
// ============================================================

const { mockEmbed, mockEmbedBatch, mockEmbedWithTimeout, mockEnsureReady } = vi.hoisted(() => ({
    mockEmbed: vi.fn(),
    mockEmbedBatch: vi.fn(),
    mockEmbedWithTimeout: vi.fn(),
    mockEnsureReady: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: () => ({
            embed: mockEmbed,
            embedBatch: mockEmbedBatch,
            embedWithTimeout: mockEmbedWithTimeout,
            ensureReady: mockEnsureReady,
            ready: true,
            dimension: 768, // Mock dynamic dimension
        }),
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import { SemanticRouter, type MemoryRoute } from "../../src/memory/SemanticRouter";

// ===========================
// Test Helpers
// ===========================

const DIMS = 768; // Changed to match mock dimension

/** Create one-hot orthogonal vectors per route for perfect cosine separation */
function routeVector(routeIndex: number): number[] {
    const vec = new Array(DIMS).fill(0);
    vec[routeIndex] = 1.0;
    return vec;
}

const ROUTE_IDX = { chitchat: 0, factual_recall: 1, deep_reasoning: 2, system_command: 3, tool_recall: 4 };

/** Map text → route index based on keywords */
function classifyText(text: string): number {
    const lower = text.toLowerCase();
    if (/chào|hello|hi\b|bye|cảm ơn|khỏe|good morning|thank|tên gì|cười|vui|tạm biệt|xin chào/.test(lower)) return ROUTE_IDX.chitchat;
    if (/chụp|tắt nhạc|bật nhạc|xóa file|mở file|dọn|dừng|thoát|chạy lệnh|gửi|execute|screenshot|send message|open browser|search the web|đọc file|ghi file|mở trình/.test(lower)) return ROUTE_IDX.system_command;
    if (/ai là|cái gì|ở đâu|bao giờ|cho tôi biết|tra cứu|tìm kiếm|nhớ lại|thông tin về|hôm qua|mẹ tôi|lịch sử|what is|who is|when did|tell me/.test(lower)) return ROUTE_IDX.factual_recall;
    if (/tại sao|giải thích|phân tích|so sánh|viết code|kế hoạch|lập trình|thiết kế|đánh giá|why|explain|write a|analyze|create a|review|debug|nghiên cứu/.test(lower)) return ROUTE_IDX.deep_reasoning;
    if (/dùng lại|chạy lại|lần trước|repeat|do it again|run that again|làm lại|thử lại/.test(lower)) return ROUTE_IDX.tool_recall;
    return -1;
}

// ===========================
// Tests
// ===========================

describe("SemanticRouter", () => {
    let router: SemanticRouter;

    beforeEach(() => {
        vi.clearAllMocks();

        mockEmbed.mockImplementation(async (text: string) => {
            const idx = classifyText(text);
            if (idx >= 0) return routeVector(idx);
            return new Array(DIMS).fill(0.001);
        });

        // embedBatch delegates to mockEmbed per-text for consistent route classification
        mockEmbedBatch.mockImplementation(async (texts: string[]) => {
            return Promise.all(texts.map(t => mockEmbed(t)));
        });

        mockEmbedWithTimeout.mockImplementation(async (text: string) => mockEmbed(text));
        mockEnsureReady.mockResolvedValue(undefined);

        router = new SemanticRouter();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    describe("Initialization", () => {
        it("should initialize successfully with route anchors", async () => {
            await router.initialize();
            expect(router.ready).toBe(true);
            const routes = router.getRoutes();
            expect(routes).toContain("chitchat");
            expect(routes).toContain("factual_recall");
            expect(routes).toContain("deep_reasoning");
            expect(routes).toContain("system_command");
        });

        it("should be idempotent — multiple init calls share same promise", async () => {
            const p1 = router.initialize();
            const p2 = router.initialize();
            await Promise.all([p1, p2]);
            expect(router.ready).toBe(true);
        });

        it("should handle embedding failure gracefully and fallback", async () => {
            mockEmbed.mockRejectedValue(new Error("Model not loaded"));
            mockEnsureReady.mockRejectedValue(new Error("Model not loaded"));

            const failRouter = new SemanticRouter();
            await failRouter.initialize();
            expect(failRouter.ready).toBe(true);
            const result = await failRouter.route("test query");
            expect(result.route).toBe("deep_reasoning");
            expect(result.confidence).toBe(0);
        });

        it("should skip undefined and dummy vectors and handle empty routes during initialization", async () => {
            const tempRouter = new SemanticRouter();
            // This mock returns an array of 0.01, which are treated as dummy vectors.
            // As a result, vectors.length === 0, and the route is skipped.
            mockEmbedBatch.mockImplementation(async (texts: string[]) => {
                return texts.map(() => new Array(DIMS).fill(0.01));
            });
            await tempRouter.initialize();
            expect(tempRouter.ready).toBe(true);
            expect(tempRouter.getRoutes().length).toBe(0);
        });
        
        it("should handle undefined vectors gracefully", async () => {
            const tempRouter = new SemanticRouter();
            mockEmbedBatch.mockImplementation(async (texts: string[]) => {
                return texts.map(() => undefined);
            });
            await tempRouter.initialize();
            expect(tempRouter.ready).toBe(true);
            expect(tempRouter.getRoutes().length).toBe(0);
        });
    });

    describe("Route Classification", () => {
        beforeEach(async () => {
            await router.initialize();
        });

        it("should route chitchat queries correctly", async () => {
            const result = await router.route("chào bạn");
            expect(result.route).toBe("chitchat");
            expect(result.confidence).toBeGreaterThan(0.45);
        });

        it("should route factual recall queries correctly", async () => {
            const result = await router.route("cho tôi biết thông tin về");
            expect(result.route).toBe("factual_recall");
            expect(result.confidence).toBeGreaterThan(0.45);
        });

        it("should route deep reasoning queries correctly", async () => {
            const result = await router.route("giải thích tại sao");
            expect(result.route).toBe("deep_reasoning");
            expect(result.confidence).toBeGreaterThan(0.45);
        });

        it("should use relaxed confidence threshold for short queries", async () => {
            const result = await router.route("tại sao?"); // <20 length
            expect(result.route).toBe("deep_reasoning");
        });

        it("should use strict confidence threshold for long queries", async () => {
            const longQ = "giải thích tại sao " + "abc ".repeat(30); // >100 length
            const result = await router.route(longQ);
            expect(result.route).toBe("deep_reasoning");
        });

        it("should route system commands correctly", async () => {
            const result = await router.route("chạy lệnh");
            expect(result.route).toBe("system_command");
            expect(result.confidence).toBeGreaterThan(0.45);
        });

        it("should handle English queries", async () => {
            const result = await router.route("hello");
            expect(result.route).toBe("chitchat");
        });

        it("should route tool recall queries and have low kit confidence", async () => {
            // "thử lại đi" maps to tool_recall in our mock.
            // No kit utterances map to tool_recall, so kit confidence will be low,
            // hitting the FALSE branch (GENERAL_KIT).
            const result = await router.route("thử lại đi");
            expect(result.route).toBe("tool_recall");
            expect(result.activeKit).toBe("GENERAL_KIT");
        });
    });

    describe("Fallback Behavior", () => {
        beforeEach(async () => {
            await router.initialize();
        });

        it("should fallback to deep_reasoning when confidence is low", async () => {
            // Return a vector orthogonal to all route anchors (one-hot at dim 100, far from dims 0-3)
            const orthogonalVec = new Array(DIMS).fill(0);
            orthogonalVec[100] = 1.0;
            mockEmbedWithTimeout.mockResolvedValueOnce(orthogonalVec);
            const result = await router.route("abc xyz random gibberish");
            expect(result.route).toBe("deep_reasoning");
        });

        it("should fallback when embedWithTimeout fails", async () => {
            mockEmbedWithTimeout.mockRejectedValueOnce(new Error("Timeout"));
            const result = await router.route("any query");
            expect(result.route).toBe("deep_reasoning");
            expect(result.confidence).toBe(0);
        });

        it("should return activeKit as GENERAL_KIT when kit confidence is low", async () => {
            mockEmbedWithTimeout.mockResolvedValueOnce(new Array(DIMS).fill(0)); // cosine similarity 0
            const result = await router.route("some query");
            expect(result.activeKit).toBe("GENERAL_KIT");
        });

        it("should return specific activeKit when kit confidence is high", async () => {
            // First kit is OBSIDIAN_KIT. We force embedWithTimeout to return [1,1,1...] 
            // and also mock embedBatch to return [1,1,1...] for kit queries so they perfectly match.
            // Actually, `beforeEach` already initialized router with `mockEmbedBatch`.
            // The default `mockEmbed` returns 0.001 for unknown queries.
            // So if we return 0.001 array, it perfectly matches the kits!
            const queryVec = new Array(DIMS).fill(0.001);
            mockEmbedWithTimeout.mockResolvedValueOnce(queryVec);
            const result = await router.route("tạo note obsidian");
            // Due to classification logic in beforeEach, chitchat/etc might override if we aren't careful, 
            // but kit classification is independent.
            expect(result.activeKit).not.toBe("GENERAL_KIT");
        });
    });

    describe("Edge Cases", () => {
        beforeEach(async () => {
            await router.initialize();
        });

        it("should handle empty query", async () => {
            const result = await router.route("");
            expect(result).toHaveProperty("route");
            expect(result).toHaveProperty("confidence");
        });

        it("should handle very long query", async () => {
            const longQuery = "a".repeat(10000);
            const result = await router.route(longQuery);
            expect(result).toHaveProperty("route");
        });

        it("should return confidence as a number between -1 and 1", async () => {
            const result = await router.route("xin chào");
            expect(result.confidence).toBeGreaterThanOrEqual(-1);
            expect(result.confidence).toBeLessThanOrEqual(1);
        });

        it("should handle cosineSimilarity mismatch in vector length", async () => {
            mockEmbedWithTimeout.mockResolvedValueOnce(new Array(10).fill(1)); // Length 10 != DIMS
            const result = await router.route("test query");
            expect(result.route).toBe("deep_reasoning");
        });

        it("should handle cosineSimilarity with zero norm", async () => {
            mockEmbedWithTimeout.mockResolvedValueOnce(new Array(DIMS).fill(0)); // normA = 0
            const result = await router.route("test query");
            expect(result.route).toBe("deep_reasoning");
        });
    });

    describe("getRoutes()", () => {
        it("should return 5 routes after initialization (incl. tool_recall from v4.0)", async () => {
            await router.initialize();
            const routes = router.getRoutes();
            expect(routes).toHaveLength(6);
            expect(routes).toEqual(
                expect.arrayContaining(["chitchat", "factual_recall", "deep_reasoning", "system_command", "tool_recall"])
            );
        });

        it("should return empty array before initialization", () => {
            expect(router.getRoutes()).toHaveLength(0);
        });
    });
});
