import { describe, it, expect, vi, beforeEach } from "vitest";
import { execute, metadata } from "../../../src/skills/personal/LocalSemanticSearch";
import { EmbeddingService } from "../../../src/services/EmbeddingService";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("../../../src/services/EmbeddingService", () => {
    const mockEmbedSvc = {
        embedWithTimeout: vi.fn().mockResolvedValue(new Array(384).fill(0.01)),
        ready: true
    };
    return {
        EmbeddingService: {
            getInstance: () => mockEmbedSvc
        }
    };
});

describe("Skill - LocalSemanticSearch", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        delete (globalThis as any).kernelInstance;
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("local_semantic_search");
        expect(metadata.category).toBe("personal");
    });

    it("should fallback to mock mode when database is not set", async () => {
        const result = await execute({ query: "remind call Duong" });
        expect(result).toContain("Mock Mode");
        expect(result).toContain("Local Semantic Search Results");
    });

    it("should perform hybrid search when embedding is ready", async () => {
        const mockStructuredMemory = {
            dbBridge: {},
            searchHybridVectors: vi.fn().mockResolvedValue([
                { content: "Sếp muốn ăn cơm trưa", type: "CONVERSATION", domain: "General", category: "personal", score: 0.85 }
            ])
        };

        (globalThis as any).kernelInstance = {
            memory: {
                getStructuredMemoryInstance: () => mockStructuredMemory
            }
        };

        const result = await execute({ query: "ăn trưa", limit: 3 });
        expect(EmbeddingService.getInstance().embedWithTimeout).toHaveBeenCalled();
        expect(mockStructuredMemory.searchHybridVectors).toHaveBeenCalled();
        expect(result).toContain("Hybrid (Semantic + Keyword)");
        expect(result).toContain("Sếp muốn ăn cơm trưa");
    });

    it("should fallback to FTS5 keyword-only search if embedding fails", async () => {
        const mockDb = {
            all: vi.fn().mockResolvedValue([
                { content: "Gọi điện thoại cho Dương", type: "FACT", domain: "General", category: "work" }
            ])
        };
        const mockStructuredMemory = {
            dbBridge: mockDb,
            searchHybridVectors: vi.fn()
        };

        (globalThis as any).kernelInstance = {
            memory: {
                getStructuredMemoryInstance: () => mockStructuredMemory
            }
        };

        // Make embedding fail
        vi.mocked(EmbeddingService.getInstance().embedWithTimeout).mockRejectedValueOnce(new Error("ONNX error"));

        const result = await execute({ query: "gọi điện thoại", limit: 2 });
        expect(mockStructuredMemory.searchHybridVectors).not.toHaveBeenCalled();
        expect(mockDb.all).toHaveBeenCalled();
        expect(result).toContain("FTS5 Keyword-Only");
        expect(result).toContain("Gọi điện thoại cho Dương");
    });
});
