import { describe, it, expect, vi, beforeEach } from "vitest";
import { VirtualManager } from "../../src/core/VirtualManager";
import { SemanticRouter } from "../../src/memory/SemanticRouter";
import { LanceMemoryManager } from "../../src/memory/LanceMemory";
import { StructuredMemory } from "../../src/memory/StructuredMemory";

describe("VirtualManager", () => {
    let virtualManager: VirtualManager;
    let mockSemanticRouter: Partial<SemanticRouter>;
    let mockStructuredMemory: Partial<StructuredMemory>;
    let mockLanceMemory: Partial<LanceMemoryManager>;

    beforeEach(() => {
        mockSemanticRouter = {
            route: vi.fn(),
        };

        mockStructuredMemory = {
            formatForSystemPrompt: vi.fn().mockReturnValue("mocked structured facts"),
        };

        mockLanceMemory = {
            searchMemory: vi.fn(),
        };

        virtualManager = new VirtualManager(
            mockSemanticRouter as SemanticRouter,
            mockStructuredMemory as StructuredMemory,
            mockLanceMemory as LanceMemoryManager
        );
    });

    it("Fast-track bypass: should return chitchat and NOT call DBs when route is chitchat", async () => {
        vi.mocked(mockSemanticRouter.route!).mockResolvedValue({ route: "chitchat", confidence: 0.9 });
        
        const result = await virtualManager.buildContextWorkflow("hello");

        expect(result.route).toBe("chitchat");
        expect(result.anchors).toEqual([]);
        expect(result.facts).toBe("");
        expect(mockStructuredMemory.formatForSystemPrompt).not.toHaveBeenCalled();
        expect(mockLanceMemory.searchMemory).not.toHaveBeenCalled();
    });

    it("Fast-track bypass: should return system_command and NOT call LanceDB but call StructuredMemory", async () => {
        vi.mocked(mockSemanticRouter.route!).mockResolvedValue({ route: "system_command", confidence: 0.95 });
        
        const result = await virtualManager.buildContextWorkflow("sysinfo");

        expect(result.route).toBe("system_command");
        expect(result.anchors).toEqual([]);
        expect(result.facts).toBe("mocked structured facts");
        expect(mockStructuredMemory.formatForSystemPrompt).toHaveBeenCalled();
        expect(mockLanceMemory.searchMemory).not.toHaveBeenCalled();
    });

    it("Parallel I/O: should query both LanceDB and StructuredMemory in parallel for deep_reasoning", async () => {
        vi.mocked(mockSemanticRouter.route!).mockResolvedValue({ route: "deep_reasoning", confidence: 0.8 });
        
        // Mock with delay to test parallel behavior implicitly
        vi.mocked(mockLanceMemory.searchMemory!).mockImplementation(async () => {
            await new Promise((r) => setTimeout(r, 50));
            return ["anchor 1", "anchor 2"];
        });

        const start = performance.now();
        const result = await virtualManager.buildContextWorkflow("complex query");
        const elapsed = performance.now() - start;

        expect(result.route).toBe("deep_reasoning");
        expect(result.anchors).toEqual(["anchor 1", "anchor 2"]);
        expect(result.facts).toBe("mocked structured facts");
        expect(mockStructuredMemory.formatForSystemPrompt).toHaveBeenCalled();
        expect(mockLanceMemory.searchMemory).toHaveBeenCalledWith("complex query", 5);
        expect(elapsed).toBeGreaterThanOrEqual(49);
    });

    it("should fallback to deep_reasoning if SemanticRouter fails", async () => {
        vi.mocked(mockSemanticRouter.route!).mockRejectedValue(new Error("Router crash"));
        vi.mocked(mockLanceMemory.searchMemory!).mockResolvedValue(["fallback anchor"]);

        const result = await virtualManager.buildContextWorkflow("query");

        expect(result.route).toBe("deep_reasoning");
        expect(result.anchors).toEqual(["fallback anchor"]);
        expect(result.facts).toBe("mocked structured facts");
    });

    it("should return empty anchors if LanceDB search fails", async () => {
        vi.mocked(mockSemanticRouter.route!).mockResolvedValue({ route: "deep_reasoning", confidence: 0.8 });
        vi.mocked(mockLanceMemory.searchMemory!).mockRejectedValue(new Error("LanceDB error"));

        const result = await virtualManager.buildContextWorkflow("query");

        expect(result.route).toBe("deep_reasoning");
        expect(result.anchors).toEqual([]);
        expect(result.facts).toBe("mocked structured facts");
    });
});
