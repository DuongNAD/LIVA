import { describe, it, expect, vi, beforeEach, type Mocked } from "vitest";
import { GraphRepository } from "../../src/memory/GraphRepository";
import { DatabaseWorkerBridge } from "../../src/memory/DatabaseWorkerBridge";
import { EmbeddingService } from "../../src/services/EmbeddingService";
import OpenAI from "openai";

vi.mock("../../src/memory/DatabaseWorkerBridge");
vi.mock("../../src/services/EmbeddingService");
vi.mock("openai");

describe("GraphRepository - GraphRAG Community Summaries", () => {
    let repository: GraphRepository;
    let mockDb: Mocked<DatabaseWorkerBridge>;
    let mockEmbeddingService: Mocked<EmbeddingService>;
    let mockOpenAI: Mocked<OpenAI>;

    beforeEach(() => {
        mockDb = new DatabaseWorkerBridge("test.sqlite") as any;
        mockEmbeddingService = Object.create(EmbeddingService.prototype) as any;
        mockOpenAI = new OpenAI({ apiKey: "test" }) as any;

        mockOpenAI.chat = {
            completions: {
                create: vi.fn()
            }
        } as any;

        repository = new GraphRepository(mockDb);
    });

    it("should handle empty graph nodes list gracefully", async () => {
        const prepareMock = vi.fn().mockReturnValue({
            all: vi.fn().mockResolvedValue([])
        });
        mockDb.prepare = prepareMock;

        const upsertVector = vi.fn();

        await repository.buildCommunitySummaries(mockOpenAI, mockEmbeddingService, upsertVector);

        expect(upsertVector).not.toHaveBeenCalled();
        expect(mockOpenAI.chat.completions.create).not.toHaveBeenCalled();
    });

    it("should group nodes via Label Propagation and summarize communities size >= 2", async () => {
        // Nodes list mock
        const nodes = [
            { id: "A", label: "PERSON", properties: "{}" },
            { id: "B", label: "PERSON", properties: "{}" },
            { id: "C", label: "PROJECT", properties: "{}" },
            { id: "X", label: "COMPANY", properties: "{}" },
            { id: "Y", label: "TECH", properties: "{}" },
            { id: "Z", label: "TOPIC", properties: "{}" } // Isolated node
        ];

        // Edges list mock
        const edges = [
            { source: "A", target: "B", relation: "KNOWS", weight: 1.0, obsolete: 0 },
            { source: "B", target: "C", relation: "WORKS_ON", weight: 1.0, obsolete: 0 },
            { source: "C", target: "A", relation: "INCLUDES", weight: 1.0, obsolete: 0 },
            { source: "X", target: "Y", relation: "USES", weight: 1.0, obsolete: 0 }
            // Z has no edge
        ];

        const mockAllNodes = vi.fn().mockResolvedValue(nodes);
        const mockAllEdges = vi.fn().mockResolvedValue(edges);

        mockDb.prepare = vi.fn().mockImplementation((query: string) => {
            if (query.includes("l3_nodes")) {
                return { all: mockAllNodes };
            }
            if (query.includes("l3_edges")) {
                return { all: mockAllEdges };
            }
            return { all: vi.fn().mockResolvedValue([]) };
        });

        // Mock OpenAI completion response
        (mockOpenAI.chat.completions.create as any).mockResolvedValue({
            choices: [{
                message: { content: "Mocked community summary content." }
            }]
        });

        // Mock Embedding Service
        mockEmbeddingService.embed = vi.fn().mockResolvedValue(new Array(384).fill(0.2));

        const upsertVector = vi.fn();

        await repository.buildCommunitySummaries(mockOpenAI, mockEmbeddingService, upsertVector);

        // Community 1: {A, B, C} -> size 3 >= 2
        // Community 2: {X, Y} -> size 2 >= 2
        // Isolated node: {Z} -> size 1 < 2, filtered out
        // Therefore, it should call OpenAI chat completions twice
        expect(mockOpenAI.chat.completions.create).toHaveBeenCalledTimes(2);

        // Should embed and upsert vector twice
        expect(mockEmbeddingService.embed).toHaveBeenCalledTimes(2);
        expect(upsertVector).toHaveBeenCalledTimes(2);

        // Check format of records passed to upsertVector
        const upsertCalls = upsertVector.mock.calls;
        expect(upsertCalls[0][0]).toMatchObject({
            type: "ANCHOR",
            domain: "Community",
            category: "CommunitySummary"
        });
        expect(upsertCalls[0][0].content).toContain("Mocked community summary content.");
    });
});
