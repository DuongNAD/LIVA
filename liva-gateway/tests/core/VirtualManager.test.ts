import { describe, it, expect, vi, beforeEach } from "vitest";
import { VirtualManager } from "../../src/core/VirtualManager";
import { SemanticRouter } from "../../src/memory/SemanticRouter";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import { EmbeddingService } from "../../src/services/EmbeddingService";

// Mock dependencies
vi.mock("../../src/memory/SemanticRouter", () => ({
    SemanticRouter: function() {
        return {
            route: vi.fn()
        };
    }
}));
vi.mock("../../src/memory/StructuredMemory", () => ({
    StructuredMemory: function() {
        return {
            formatForSystemPrompt: vi.fn().mockReturnValue("mock-structured-facts")
        };
    }
}));
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: function() {
        return {
            embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1))
        };
    }
}));

describe("VirtualManager", () => {
    let router: SemanticRouter;
    let structMem: StructuredMemory;
    let embeddingService: EmbeddingService;
    let manager: VirtualManager;

    beforeEach(() => {
        vi.clearAllMocks();
        router = new SemanticRouter();
        structMem = new StructuredMemory("agent.sqlite");
        structMem.vecReady = true;
        structMem.searchAnchors = vi.fn().mockReturnValue(["memory-anchor-1"]);
        
        embeddingService = new EmbeddingService();
        manager = new VirtualManager(router, structMem, embeddingService);
    });

    it("should return chitchat bypass if routed as chitchat", async () => {
        vi.mocked(router.route).mockResolvedValue({ route: "chitchat", confidence: 1 });
        const result = await manager.buildContextWorkflow("hello");
        expect(result.route).toBe("chitchat");
        expect(result.anchors).toEqual([]);
        expect(result.facts).toBe("");
    });

    it("should return system_command bypass if routed as system_command", async () => {
        vi.mocked(router.route).mockResolvedValue({ route: "system_command", confidence: 1 });
        const result = await manager.buildContextWorkflow("system status");
        expect(result.route).toBe("system_command");
        expect(result.anchors).toEqual([]);
        expect(result.facts).toBe("mock-structured-facts");
    });

    it("should build full context if factual_recall", async () => {
        vi.mocked(router.route).mockResolvedValue({ route: "factual_recall", confidence: 0.9 });
        const result = await manager.buildContextWorkflow("query");
        expect(result.route).toBe("factual_recall");
        expect(result.anchors).toEqual(["memory-anchor-1"]);
        expect(result.facts).toBe("mock-structured-facts");
    });

    it("should fallback to deep_reasoning if SemanticRouter fails", async () => {
        vi.mocked(router.route).mockRejectedValue(new Error("Router error"));
        const result = await manager.buildContextWorkflow("complex query");
        expect(result.route).toBe("deep_reasoning");
        expect(result.anchors).toEqual(["memory-anchor-1"]);
    });

    it("should return empty anchors if embeddingService throws error", async () => {
        vi.mocked(router.route).mockResolvedValue({ route: "factual_recall", confidence: 0.9 });
        vi.mocked(embeddingService.embed).mockRejectedValue(new Error("Embedding error"));
        const result = await manager.buildContextWorkflow("query");
        expect(result.anchors).toEqual([]);
    });

    it("should return empty anchors if vecReady is false", async () => {
        structMem.vecReady = false;
        vi.mocked(router.route).mockResolvedValue({ route: "factual_recall", confidence: 0.9 });
        const result = await manager.buildContextWorkflow("query");
        expect(result.anchors).toEqual([]);
    });

    it("should log 'none' when structured facts are empty", async () => {
        vi.mocked(router.route).mockResolvedValue({ route: "factual_recall", confidence: 0.9 });
        vi.mocked(structMem.formatForSystemPrompt).mockReturnValue("");
        const result = await manager.buildContextWorkflow("query");
        expect(result.facts).toBe("");
    });
});
