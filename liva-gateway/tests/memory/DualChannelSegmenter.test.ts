import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock VectorMath
vi.mock("../../src/utils/VectorMath", () => ({
    cosineSimilarity: vi.fn(),
}));

// Mock EmbeddingService
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: vi.fn(),
}));

// Mock openai
vi.mock("openai", () => ({
    default: vi.fn(),
}));

import { DualChannelSegmenter, smartTruncate } from "@memory/DualChannelSegmenter";
import { cosineSimilarity } from "../../src/utils/VectorMath";

describe("DualChannelSegmenter — Topic-Aware Episode Boundary Detection", () => {
    let segmenter: DualChannelSegmenter;
    let mockAiClient: any;
    let mockEmbeddingService: any;

    beforeEach(() => {
        vi.clearAllMocks();
        mockEmbeddingService = {};
        mockAiClient = {
            chat: {
                completions: {
                    create: vi.fn(),
                },
            },
        };
        segmenter = new DualChannelSegmenter(mockEmbeddingService, mockAiClient);
    });

    // ============================================================
    // smartTruncate() — exported utility
    // ============================================================
    describe("smartTruncate()", () => {
        it("should return text unchanged if shorter than maxLen", () => {
            expect(smartTruncate("short", 100)).toBe("short");
        });

        it("should truncate long text keeping head and tail", () => {
            const text = "A".repeat(100);
            const result = smartTruncate(text, 20);
            expect(result.length).toBeLessThan(100);
            expect(result).toContain("[...]");
        });

        it("should handle exact boundary length", () => {
            const text = "exact";
            expect(smartTruncate(text, 5)).toBe("exact");
        });

        it("should handle empty string", () => {
            expect(smartTruncate("", 10)).toBe("");
        });
    });

    // ============================================================
    // Channel 1: Topic Shift Detection
    // ============================================================
    describe("Channel 1: detectTopicShift()", () => {
        it("should return false for first message (no cluster)", async () => {
            const embedding = [0.1, 0.2, 0.3];
            const result = await segmenter.detectTopicShift(embedding);
            expect(result).toBe(false);
        });

        it("should return false when similarity is above threshold", async () => {
            vi.mocked(cosineSimilarity).mockReturnValue(0.85); // > 0.65
            await segmenter.detectTopicShift([0.1, 0.2, 0.3]); // seed
            const result = await segmenter.detectTopicShift([0.1, 0.2, 0.4]);
            expect(result).toBe(false);
        });

        it("should return true when similarity is below threshold (topic shift)", async () => {
            vi.mocked(cosineSimilarity).mockReturnValue(0.4); // < 0.65
            await segmenter.detectTopicShift([0.1, 0.2, 0.3]); // seed
            const result = await segmenter.detectTopicShift([0.9, 0.8, 0.7]);
            expect(result).toBe(true);
        });
    });

    // ============================================================
    // Channel 2: Surprise Detection (LLM Judge)
    // ============================================================
    describe("Channel 2: detectSurprise()", () => {
        it("should return 0 when no novel entities detected", async () => {
            // Message with no capitalized words, no tech terms, no URLs
            const score = await segmenter.detectSurprise("hello world", "recent context");
            expect(score).toBe(0);
            expect(mockAiClient.chat.completions.create).not.toHaveBeenCalled();
        });

        it("should call LLM when novel entities detected and return score", async () => {
            mockAiClient.chat.completions.create.mockResolvedValue({
                choices: [{ message: { content: "8" } }],
            });
            // Tech term is novel entity
            const score = await segmenter.detectSurprise("Using Docker for deployment", "recent context");
            expect(score).toBe(8);
        });

        it("should return 0 on LLM failure (fail-safe)", async () => {
            mockAiClient.chat.completions.create.mockRejectedValue(new Error("API down"));
            const score = await segmenter.detectSurprise("Docker is great", "context");
            expect(score).toBe(0);
        });

        it("should return 0 when LLM returns non-numeric response", async () => {
            mockAiClient.chat.completions.create.mockResolvedValue({
                choices: [{ message: { content: "not a number" } }],
            });
            const score = await segmenter.detectSurprise("New API endpoint", "context");
            expect(score).toBe(0);
        });
    });

    // ============================================================
    // shouldCreateNewEpisode()
    // ============================================================
    describe("shouldCreateNewEpisode()", () => {
        it("should create new episode when topic shifts (Channel 1)", async () => {
            vi.mocked(cosineSimilarity).mockReturnValue(0.3); // Topic shift
            // Seed the cluster
            await segmenter.detectTopicShift([0.1, 0.2, 0.3]);

            const result = await segmenter.shouldCreateNewEpisode(
                "Something new", [0.9, 0.8, 0.7], "old context", "user"
            );
            expect(result).toBe(true);
        });

        it("should NOT create new episode when both channels are low", async () => {
            vi.mocked(cosineSimilarity).mockReturnValue(0.9); // No topic shift
            // Seed the cluster
            await segmenter.detectTopicShift([0.1, 0.2, 0.3]);

            // No novel entities = surprise returns 0
            const result = await segmenter.shouldCreateNewEpisode(
                "hello again", [0.1, 0.2, 0.3], "old context", "user"
            );
            expect(result).toBe(false);
        });
    });

    // ============================================================
    // resetCluster()
    // ============================================================
    describe("resetCluster()", () => {
        it("should reset cluster embeddings", async () => {
            // Add to cluster
            await segmenter.detectTopicShift([0.1, 0.2, 0.3]);
            segmenter.resetCluster([0.9, 0.9, 0.9]);

            // After reset, next call should be like first message
            vi.mocked(cosineSimilarity).mockReturnValue(0.9);
            const result = await segmenter.detectTopicShift([0.9, 0.9, 0.9]);
            expect(result).toBe(false);
        });
    });

    // ============================================================
    // Circuit Breaker
    // ============================================================
    describe("Circuit Breaker (MAX_TURNS_PER_EPISODE)", () => {
        it("should force new episode after 10 turns when role is 'ai'", async () => {
            vi.mocked(cosineSimilarity).mockReturnValue(0.95); // No topic shift
            // Fill up 10 cluster embeddings
            for (let i = 0; i < 10; i++) {
                await segmenter.detectTopicShift([0.1 * i, 0.2, 0.3]);
            }

            // 11th turn as 'ai' should trigger circuit breaker
            const result = await segmenter.shouldCreateNewEpisode(
                "turn 11", [0.1, 0.2, 0.3], "context", "ai"
            );
            expect(result).toBe(true);
        });

        it("should NOT force new episode on 11th turn as 'user'", async () => {
            vi.mocked(cosineSimilarity).mockReturnValue(0.95);
            for (let i = 0; i < 10; i++) {
                await segmenter.detectTopicShift([0.1 * i, 0.2, 0.3]);
            }

            const result = await segmenter.shouldCreateNewEpisode(
                "turn 11 user", [0.1, 0.2, 0.3], "context", "user"
            );
            // Won't force-break mid Q&A when role is 'user'
            expect(result).toBe(false);
        });
    });
});
