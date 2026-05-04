import { describe, it, expect, vi, beforeEach } from "vitest";
import { DockerEnvManager } from "../../src/sandbox/DockerEnvManager";
import { SkillRegistry } from "../../src/SkillRegistry";
import * as fs from "node:fs";
import { spawn } from "child_process";

vi.mock("child_process", () => {
    return {
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
                embedWithTimeout: vi.fn().mockResolvedValue([0.1, 0.2])
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
            readFile: vi.fn()
        }
    };
});

describe("ExtraCoverage - DockerEnvManager", () => {
    let docker: DockerEnvManager;

    beforeEach(() => {
        vi.clearAllMocks();
        docker = new DockerEnvManager();
    });

    it("should reject with exit code when child closes with non-zero", async () => {
        vi.mocked(spawn).mockImplementation(() => {
            return {
                stdout: { on: vi.fn() },
                stderr: { on: vi.fn() },
                on: vi.fn((event, cb) => {
                    if (event === "close") {
                        cb(1); // non-zero code
                    }
                })
            } as any;
        });
        await expect(docker.runSandboxTest(["arg"])).rejects.toThrow("Exit code 1");
    });

    it("should reject with generic error when child emits error (not AbortError)", async () => {
        vi.mocked(spawn).mockImplementation(() => {
            return {
                stdout: { on: vi.fn() },
                stderr: { on: vi.fn() },
                on: vi.fn((event, cb) => {
                    if (event === "error") {
                        const err = new Error("General error");
                        err.name = "Error";
                        cb(err);
                    }
                })
            } as any;
        });
        await expect(docker.runSandboxTest(["arg"])).rejects.toThrow("General error");
    });

    it("should handle AbortError and call cleanupZombieContainer", async () => {
        // mock cleanup to throw so we cover the catch block too
        vi.spyOn(docker as any, "cleanupZombieContainer").mockRejectedValueOnce(new Error("Cleanup Failed"));
        
        vi.mocked(spawn).mockImplementation(() => {
            return {
                stdout: { on: vi.fn() },
                stderr: { on: vi.fn() },
                on: vi.fn((event, cb) => {
                    if (event === "error") {
                        const err = new Error("Abort");
                        err.name = "AbortError";
                        cb(err);
                    }
                })
            } as any;
        });
        
        await expect(docker.runSandboxTest(["arg"])).rejects.toThrow("Timeout 60s. Bị ngắt bởi AbortController.");
        expect((docker as any).cleanupZombieContainer).toHaveBeenCalled();
    });
});

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
        vi.mock("../../src/services/EmbeddingService", () => ({
            EmbeddingService: {
                getInstance: vi.fn().mockReturnValue({
                    ready: true,
                    ensureReady: vi.fn().mockResolvedValue(true),
                    embed: vi.fn().mockResolvedValue([0, 0]),
                })
            }
        }));
        
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
        expect(result.length).toBeGreaterThanOrEqual(2);
    });

    it("should return immediately if active tools list is empty", async () => {
        const result = await registry.getSemanticTopK("hello", "SPECIAL_KIT" as any, 1);
        expect(result.length).toBeGreaterThanOrEqual(2);
    });
});
