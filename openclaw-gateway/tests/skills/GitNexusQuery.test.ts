import { describe, it, expect, vi, afterEach } from "vitest";
import * as GitNexusQuery from "../../src/skills/devops/GitNexusQuery";
import { EmbeddingService } from "../../src/services/EmbeddingService";

// Mock EmbeddingService with 384D vectors (matching default vec dimension)
vi.mock("../../src/services/EmbeddingService", () => {
    const dummyVec = new Array(384).fill(0.1);
    const embedMock = vi.fn().mockResolvedValue(dummyVec);
    const getInstanceMock = vi.fn().mockReturnValue({ embed: embedMock, dimension: 384 });
    return {
        EmbeddingService: {
            getInstance: getInstanceMock
        }
    };
});

// Mock StructuredMemory to avoid real sqlite-vec initialization
vi.mock("../../src/memory/StructuredMemory", () => {
    const mockInstance = {
        vecReady: true,
        searchSimilarVectors: vi.fn().mockReturnValue([
            { type: "ANCHOR", domain: "src", category: "Code", content: "const a = 1;", distance: 0.5, traceKeywords: [] }
        ]),
        initVecDimension: vi.fn(),
    };
    return {
        StructuredMemory: {
            create: vi.fn().mockResolvedValue(mockInstance),
        }
    };
});

describe("GitNexusQuery Skill", () => {
    afterEach(() => {
        vi.clearAllMocks();
    });

    it("should query semantic codebase without leaking VRAM", async () => {
        const result = await GitNexusQuery.execute({ query: "test function" });
        
        expect(EmbeddingService.getInstance).toHaveBeenCalled();
        expect(EmbeddingService.getInstance().embed).toHaveBeenCalledWith("test function");
        expect(result).toContain("const a = 1;");
    });
});
