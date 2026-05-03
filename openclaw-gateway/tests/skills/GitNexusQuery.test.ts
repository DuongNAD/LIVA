import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as GitNexusQuery from "../../src/skills/devops/GitNexusQuery";
import { EmbeddingService } from "../../src/services/EmbeddingService";
import { LanceMemoryManager } from "../../src/memory/LanceMemory";

vi.mock("../../src/services/EmbeddingService", () => {
    const embedMock = vi.fn().mockResolvedValue([0.1, 0.2, 0.3]);
    const getInstanceMock = vi.fn().mockReturnValue({ embed: embedMock });
    return {
        EmbeddingService: {
            getInstance: getInstanceMock
        }
    };
});

vi.mock("../../src/memory/LanceMemory", () => {
    const executeMock = vi.fn().mockResolvedValue([
        { filepath: "src/test.ts", content: "const a = 1;" }
    ]);
    const limitMock = vi.fn().mockReturnValue({ execute: executeMock });
    const searchMock = vi.fn().mockReturnValue({ limit: limitMock });
    
    return {
        LanceMemoryManager: {
            getInstance: vi.fn().mockReturnValue({
                getDB: vi.fn().mockResolvedValue({
                    search: searchMock
                })
            })
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
        expect(result).toContain("src/test.ts");
        expect(result).toContain("const a = 1;");
    });
});
