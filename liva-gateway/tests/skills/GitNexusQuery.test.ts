/**
 * GitNexusQuery.test.ts — GitNexus vector-search skill unit tests
 * Tests: metadata, happy path, empty results, vecReady=false, embed failure
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const { embedMock, searchSimilarVectorsMock, mockInstance } = vi.hoisted(() => {
    const dummyVec = new Array(384).fill(0.1);
    const embedMock = vi.fn().mockResolvedValue(dummyVec);
    const searchSimilarVectorsMock = vi.fn().mockResolvedValue([
        { type: "ANCHOR", domain: "src", category: "Code", content: "const a = 1;", distance: 0.5, traceKeywords: [] },
    ]);
    const mockInstance = {
        vecReady: true,
        searchSimilarVectors: searchSimilarVectorsMock,
        initVecDimension: vi.fn(),
    };
    return { embedMock, searchSimilarVectorsMock, mockInstance };
});

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: vi.fn().mockReturnValue({ embed: embedMock, dimension: 384 }),
    },
}));

vi.mock("../../src/memory/StructuredMemory", () => ({
    StructuredMemory: {
        create: vi.fn().mockResolvedValue(mockInstance),
    },
}));

import * as GitNexusQuery from "../../src/skills/devops/GitNexusQuery";
import { EmbeddingService } from "../../src/services/EmbeddingService";

describe("GitNexusQuery Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Reset to happy-path defaults
        mockInstance.vecReady = true;
        embedMock.mockResolvedValue(new Array(384).fill(0.1));
        searchSimilarVectorsMock.mockResolvedValue([
            { type: "ANCHOR", domain: "src", category: "Code", content: "const a = 1;", distance: 0.5, traceKeywords: [] },
        ]);
    });

    it("should have correct metadata", () => {
        expect(GitNexusQuery.metadata.name).toBe("gitnexus_query");
        expect(GitNexusQuery.metadata.parameters.required).toContain("query");
    });

    it("should query semantic codebase without leaking VRAM", async () => {
        const result = await GitNexusQuery.execute({ query: "test function" });

        expect(EmbeddingService.getInstance).toHaveBeenCalled();
        expect(EmbeddingService.getInstance().embed).toHaveBeenCalledWith("test function");
        expect(result).toContain("const a = 1;");
    });

    it("should include 'Không tìm thấy' when results are empty", async () => {
        searchSimilarVectorsMock.mockResolvedValue([]);

        const result = await GitNexusQuery.execute({ query: "nonexistent code" });

        expect(result).toContain("Không tìm thấy kết quả phù hợp.");
    });

    it("should throw when vecReady is false", async () => {
        mockInstance.vecReady = false;

        await expect(GitNexusQuery.execute({ query: "test" }))
            .rejects
            .toThrow("sqlite-vec chưa được khởi tạo");
    });

    it("should throw when embed fails", async () => {
        embedMock.mockRejectedValue(new Error("Embedding model unavailable"));

        await expect(GitNexusQuery.execute({ query: "test" }))
            .rejects
            .toThrow("Embedding model unavailable");
    });
});
