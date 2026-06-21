import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { FlashRankService } from "../../src/services/FlashRankService";

describe("FlashRankReranker", () => {
    let service: FlashRankService;

    beforeEach(() => {
        service = FlashRankService.getInstance();
    });

    afterEach(async () => {
        await service.dispose();
    });

    it("should successfully initialize FlashRankService and load the worker", async () => {
        await expect(service.initialize()).resolves.not.toThrow();
    });

    it("should rerank 10 mock text chunks and return top 3 sorted by relevance score", async () => {
        const query = "apple banana fruit";
        
        // 10 mock documents with varying degrees of overlap
        const docs = [
            { id: 1, content: "Completely irrelevant content about dogs and cats." },
            { id: 2, content: "This is a document about an apple and a banana, which are both delicious fruits." }, // Max overlap (3 query terms)
            { id: 3, content: "Only mentions banana here." }, // Partial overlap (1 query term)
            { id: 4, content: "Another text talking about spaceships and stars." },
            { id: 5, content: "We love eating apple pies in the summer." }, // Partial overlap (1 query term)
            { id: 6, content: "A fruit salad recipe that includes apple, banana, and orange." }, // Max overlap (3 query terms)
            { id: 7, content: "Under the weather today." },
            { id: 8, content: "Banana bread is very easy to make." }, // Partial overlap (1 query term)
            { id: 9, content: "Random string of text with no real meaning." },
            { id: 10, content: "General overview of fresh agricultural fruits like apple." } // Medium-high overlap (2 query terms)
        ];

        // Call the rerank method
        const reranked = await service.rerank(query, docs);

        // Verify it returned all 10 documents with scores attached
        expect(reranked.length).toBe(10);
        
        // Check scores structure and boundaries
        for (const doc of reranked) {
            expect(doc).toHaveProperty("score");
            expect(typeof doc.score).toBe("number");
            expect(doc.score).toBeGreaterThanOrEqual(0);
            expect(doc.score).toBeLessThanOrEqual(1.0);
        }

        // Verify sorted order (score descending)
        for (let i = 0; i < reranked.length - 1; i++) {
            expect(reranked[i].score).toBeGreaterThanOrEqual(reranked[i + 1].score);
        }

        // Retrieve top 3
        const top3 = reranked.slice(0, 3);
        expect(top3.length).toBe(3);

        const top3Ids = top3.map(d => d.id);
        
        // Doc 2 and Doc 6 have the highest overlap and should definitely be in the top 3
        expect(top3Ids).toContain(2);
        expect(top3Ids).toContain(6);
    });

    it("should return empty array when reranking empty documents list", async () => {
        const reranked = await service.rerank("query", []);
        expect(reranked).toEqual([]);
    });

    it("should handle string-only documents list and sort them", async () => {
        const query = "apple";
        const docs = [
            "completely unrelated doc",
            "this doc mentions apple",
            "just another text"
        ];
        const reranked = await service.rerank(query, docs);
        expect(reranked.length).toBe(3);
        expect(reranked[0].content).toBe("this doc mentions apple");
        expect(reranked[0].score).toBeGreaterThan(reranked[1].score);
    });
});
