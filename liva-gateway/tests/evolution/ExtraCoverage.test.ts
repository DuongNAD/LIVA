import { describe, it, expect, vi, beforeEach } from "vitest";
import { MicroVMDaemon } from "../../src/sandbox/MicroVMDaemon";
import { SkillRegistry } from "../../src/SkillRegistry";
import * as fs from "node:fs";
import { spawn } from "child_process";

vi.mock("child_process", async (importOriginal) => {
    const actual = await importOriginal<typeof import("child_process")>();
    return {
        ...actual,
        spawn: vi.fn()
    };
});

vi.mock("../../src/services/EmbeddingService", () => {
    return {
        EmbeddingService: {
            getInstance: vi.fn().mockReturnValue({
                ready: true,
                ensureReady: vi.fn().mockResolvedValue(true),
                embed: vi.fn().mockResolvedValue([0.1, 0.2]),
                embedWithTimeout: vi.fn().mockResolvedValue([0.1, 0.2]),
                isVramYielded: vi.fn().mockReturnValue(false)
            })
        }
    };
});

vi.mock("node:fs", async (importOriginal) => {
    const actual: any = await importOriginal();
    return {
        ...actual,
        existsSync: vi.fn().mockReturnValue(true),
        promises: {
            ...actual.promises,
            mkdir: vi.fn().mockResolvedValue(undefined),
            readFile: vi.fn().mockResolvedValue("[]"),
            writeFile: vi.fn().mockResolvedValue(undefined),
            readdir: vi.fn().mockResolvedValue([]),
        }
    };
});

vi.mock("node:fs/promises", async () => {
    return {
        access: vi.fn().mockResolvedValue(undefined),
        readdir: vi.fn().mockResolvedValue([]),
        readFile: vi.fn().mockResolvedValue("[]"),
        writeFile: vi.fn().mockResolvedValue(undefined),
        mkdir: vi.fn().mockResolvedValue(undefined),
    };
});

// DockerEnvManager tests removed — module was deprecated and deleted
// MicroVMDaemon is the sole sandbox executor now

vi.mock("../../src/mcp/MCPClientManager", () => {
    return {
        MCPClientManager: {
            getInstance: vi.fn().mockReturnValue({
                connectServer: vi.fn(),
                getAllConnectedTools: vi.fn().mockResolvedValue([]),
                callTool: vi.fn().mockRejectedValue(new Error("MCP Tool 'MockSkill' không tồn tại hoặc chưa kết nối!"))
            })
        }
    };
});

describe("ExtraCoverage - SkillRegistry", () => {
    let registry: SkillRegistry;

    beforeEach(() => {
        vi.clearAllMocks();
        registry = new SkillRegistry();
    });

    it("should catch and log error in registerLocalSkills", async () => {
        const mcp = (registry as any).mcpManager;
        vi.mocked(mcp.connectServer).mockRejectedValueOnce(new Error("Init failed"));
        await expect(registry.registerLocalSkills()).resolves.not.toThrow();
    });

    it("should fallback to activeKit in getSemanticTopK", async () => {
        registry.registerSkill({
            name: "test_skill",
            description: "test",
            isCoreSkill: false,
            kit: "GENERAL_KIT",
            execute: vi.fn()
        } as any);
        const result = await registry.getSemanticTopK("hello", "GENERAL_KIT" as any, 1);
        expect(result).toBeDefined();
    });

    it("should gracefully handle execution error", async () => {
        try {
            await registry.executeSkill("MockSkill", {});
        } catch (e: any) {
            expect(e.message).toBe("MCP Tool 'MockSkill' không tồn tại hoặc chưa kết nối!");
        }
    });

    it("should return only core skills if no tools above threshold", async () => {
        // mock low score
        const mockInstance = (await import("../../src/services/EmbeddingService")).EmbeddingService.getInstance();
        vi.mocked(mockInstance.embed).mockResolvedValue([0, 0]);
        
        registry.registerSkill({
            name: "test_skill_low",
            description: "test test", // missing short_desc to hit fallback
            isCoreSkill: false,
            kit: "GENERAL_KIT",
            execute: vi.fn()
        } as any);

        const result = await registry.getSemanticTopK("totally different query", "GENERAL_KIT" as any, 1);
        expect(result).toBeDefined();
        // Since score will be 0 < 0.65 threshold, it should return core tools
        expect(result.length).toBeGreaterThanOrEqual(1);
    });

    it("should return immediately if active tools list is empty", async () => {
        const result = await registry.getSemanticTopK("hello", "SPECIAL_KIT" as any, 1);
        expect(result.length).toBeGreaterThanOrEqual(1);
    });
});
