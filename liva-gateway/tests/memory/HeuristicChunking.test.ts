import { describe, it, expect, vi, beforeEach, type Mocked } from "vitest";
import { HeuristicSemanticChunker } from "../../src/utils/HeuristicSemanticChunker";
import { EmbeddingService } from "../../src/services/EmbeddingService";

vi.mock("../../src/services/EmbeddingService");

describe("HeuristicSemanticChunker", () => {
    let mockEmbeddingService: Mocked<EmbeddingService>;

    beforeEach(() => {
        mockEmbeddingService = Object.create(EmbeddingService.prototype) as any;
        mockEmbeddingService.embed = vi.fn().mockResolvedValue(new Array(384).fill(0.1));
    });

    it("should return empty array for empty or whitespace-only text", async () => {
        expect(await HeuristicSemanticChunker.chunk("", mockEmbeddingService)).toEqual([]);
        expect(await HeuristicSemanticChunker.chunk("   ", mockEmbeddingService)).toEqual([]);
    });

    it("should group sentences below soft limit without calling embed", async () => {
        const text = "First sentence. Second sentence. Third sentence.";
        const result = await HeuristicSemanticChunker.chunk(text, mockEmbeddingService, 150, 256);
        
        expect(result).toEqual([text]);
        expect(mockEmbeddingService.embed).not.toHaveBeenCalled();
    });

    it("should trigger semantic check and split when topic shift is detected (> soft limit)", async () => {
        // Create sentences such that cumulative token count exceeds soft limit (10 tokens)
        // Sentence 1: "Hello world this is a test sentence for soft limit." (11 words * 1.3 = 15 tokens)
        // Sentence 2: "Unrelated topic banana fruit apple orange juice." (8 words * 1.3 = 11 tokens)
        const s1 = "Hello world this is a test sentence for soft limit.";
        const s2 = "Unrelated topic banana fruit apple orange juice.";
        const text = `${s1} ${s2}`;

        // Return orthogonal vectors to force similarity to be low (topic shift)
        mockEmbeddingService.embed
            .mockResolvedValueOnce([1, 0, 0])  // vec for current chunk (s1)
            .mockResolvedValueOnce([0, 1, 0]); // vec for s2

        const result = await HeuristicSemanticChunker.chunk(text, mockEmbeddingService, 10, 50);
        
        expect(result).toEqual([s1, s2]);
        expect(mockEmbeddingService.embed).toHaveBeenCalledTimes(2);
    });

    it("should trigger semantic check and append when no topic shift is detected (> soft limit)", async () => {
        const s1 = "Hello world this is a test sentence for soft limit.";
        const s2 = "Hello world this is another test sentence for soft limit.";
        const text = `${s1} ${s2}`;

        // Return identical vectors to force similarity to be high (no topic shift)
        mockEmbeddingService.embed
            .mockResolvedValueOnce([1, 1, 0])  // vec for current chunk
            .mockResolvedValueOnce([1, 1, 0]); // vec for s2

        const result = await HeuristicSemanticChunker.chunk(text, mockEmbeddingService, 10, 50);
        
        expect(result).toEqual([`${s1} ${s2}`]);
        expect(mockEmbeddingService.embed).toHaveBeenCalledTimes(2);
    });

    it("should force split when hard limit is exceeded", async () => {
        // Sentence 1: ~15 tokens
        // Sentence 2: ~15 tokens
        // total: 30 tokens, exceeds hard limit of 20
        const s1 = "Hello world this is a test sentence for soft limit.";
        const s2 = "Hello world this is another test sentence for soft limit.";
        const text = `${s1} ${s2}`;

        const result = await HeuristicSemanticChunker.chunk(text, mockEmbeddingService, 10, 20);
        
        expect(result).toEqual([s1, s2]);
        // Should split strictly due to hard limit, without calling embed
        expect(mockEmbeddingService.embed).not.toHaveBeenCalled();
    });

    it("should fall back to greedy append when embed throws an error", async () => {
        const s1 = "Hello world this is a test sentence for soft limit.";
        const s2 = "Hello world this is another test sentence for soft limit.";
        const text = `${s1} ${s2}`;

        mockEmbeddingService.embed.mockRejectedValue(new Error("Embedding API failure"));

        const result = await HeuristicSemanticChunker.chunk(text, mockEmbeddingService, 10, 50);
        
        expect(result).toEqual([`${s1} ${s2}`]);
        expect(mockEmbeddingService.embed).toHaveBeenCalledTimes(2); // Attempted to embed both but failed
    });
});
