import { describe, it, expect, vi, beforeEach } from "vitest";
import { GraphWeaverDaemon } from "../../src/memory/GraphWeaverDaemon";
import { EmbeddingService } from "../../src/services/EmbeddingService";
import { ObsidianVaultManager } from "../../src/memory/ObsidianVaultManager";

vi.mock("../../src/services/EmbeddingService", () => {
    const embedMock = vi.fn();
    return {
        EmbeddingService: {
            getInstance: vi.fn().mockReturnValue({ embed: embedMock })
        }
    };
});

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn()
    }
}));

describe("GraphWeaverDaemon", () => {
    let weaver: GraphWeaverDaemon;
    let vaultManager: any;

    beforeEach(() => {
        vaultManager = {} as any;
        weaver = new GraphWeaverDaemon(vaultManager);
        vi.clearAllMocks();
    });

    it("should parse defective JSON securely", () => {
        const rawLLMOutput = `
        Đây là kết quả trích xuất:
        \`\`\`json
        {
            "entities": [
                { "name": "Vi khuẩn DPAOs", "type": "Microorganism", "description": "Vi khuẩn tích lũy polyphosphate" }
            ],
            "relationships": []
        // Quên đóng ngoặc
        `;

        const parsed = weaver.parseLLMOutput(rawLLMOutput);
        expect(parsed).not.toBeNull();
        expect(parsed?.entities[0].name).toBe("Vi khuẩn DPAOs");
    });

    it("should merge entities with high cosine similarity (Semantic Disambiguation)", async () => {
        // Mock vectors
        // A và B giống hệt nhau
        const vecA = [1, 0, 0];
        const vecB = [0.95, 0.3, 0]; // Giống A ở khoảng 0.95 (nằm giữa 0.92 và 0.99)

        weaver.seedExistingEntity("Tetrasphaera", vecA);

        const embedMock = vi.mocked(EmbeddingService.getInstance().embed);
        embedMock.mockResolvedValue(vecB);

        const result = await weaver.disambiguateEntity("Tetra-sphaera bacteria");
        
        // Vì vecA và vecB có similarity = 0.995 > 0.92, nó phải merge
        expect(result).toBe("Tetrasphaera|Tetra-sphaera bacteria");
    });

    it("should create new entity if similarity is low", async () => {
        const vecA = [1, 0, 0];
        const vecC = [0, 1, 0]; // Trực giao, similarity = 0

        weaver.seedExistingEntity("Tetrasphaera", vecA);

        const embedMock = vi.mocked(EmbeddingService.getInstance().embed);
        embedMock.mockResolvedValue(vecC);

        const result = await weaver.disambiguateEntity("E. coli");
        
        expect(result).toBe("E. coli");
    });
});
