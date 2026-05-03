/**
 * SkillRegistry.test.ts — MCP Tool Routing & Fallback Tests
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../src/mcp/MCPClientManager", () => ({
    MCPClientManager: {
        getInstance: vi.fn().mockReturnValue({
            connectServer: vi.fn().mockResolvedValue(undefined),
            getAllConnectedTools: vi.fn().mockResolvedValue([]),
            executeTool: vi.fn(),
        }),
    },
}));

vi.mock("../src/skills/web/GeminiSurfer.js", () => ({
    metadata: { name: "gemini_surfer", description: "Mock", parameters: {} },
    execute: vi.fn().mockResolvedValue("ok"),
}));

const { mockEmbedWithTimeout, mockEmbed } = vi.hoisted(() => ({
    mockEmbedWithTimeout: vi.fn(),
    mockEmbed: vi.fn(),
}));

vi.mock("../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: vi.fn().mockReturnValue({
            embedWithTimeout: mockEmbedWithTimeout,
            embed: mockEmbed,
        }),
    },
}));

import { SkillRegistry, type AgentSkill } from "../src/SkillRegistry";

describe("SkillRegistry", () => {
    let registry: SkillRegistry;
    beforeEach(() => { registry = new SkillRegistry(); });

    it("should register get_current_time on construction", () => {
        const skills = registry.getAllSkills();
        expect(skills.find(s => s.name === "get_current_time")).toBeDefined();
    });

    it("should register read_file on construction", () => {
        expect(registry.getAllSkills().find(s => s.name === "read_file")).toBeDefined();
    });

    it("get_current_time should return a string", async () => {
        const skill = registry.getAllSkills().find(s => s.name === "get_current_time");
        const result = await skill!.execute!({});
        expect(typeof result).toBe("string");
    });

    it("get_current_time should return time in specified timezone", async () => {
        const skill = registry.getAllSkills().find(s => s.name === "get_current_time");
        const result = await skill!.execute!({ timezone: "Asia/Tokyo" });
        expect(typeof result).toBe("string");
        // We can't strictly assert the exact string because time changes, but it shouldn't throw and should be a string.
    });

    it("should register and execute custom fallback skill", async () => {
        const exec = vi.fn().mockResolvedValue("done");
        registry.registerSkill({ name: "custom", description: "x", parameters: {}, execute: exec });
        const result = await registry.executeSkill("custom", { a: 1 });
        expect(result).toBe("done");
        expect(exec).toHaveBeenCalledWith({ a: 1 });
    });

    it("should execute MCP tool successfully and return text", async () => {
        const mcpManager = (await import("../src/mcp/MCPClientManager")).MCPClientManager.getInstance();
        (registry as any).mcpToolsList = [{ name: "mcp_tool", _serverId: "test_srv" }];
        vi.mocked(mcpManager.executeTool).mockResolvedValueOnce({
            content: [{ type: "text", text: "mcp_result" }]
        });
        const result = await registry.executeSkill("mcp_tool", {});
        expect(result).toBe("mcp_result");
    });

    it("should execute MCP tool and return default success when content array is empty", async () => {
        const mcpManager = (await import("../src/mcp/MCPClientManager")).MCPClientManager.getInstance();
        (registry as any).mcpToolsList = [{ name: "mcp_tool", _serverId: "test_srv" }];
        vi.mocked(mcpManager.executeTool).mockResolvedValueOnce({
            content: []
        });
        const result = await registry.executeSkill("mcp_tool", {});
        expect(result).toBe("Success (No content)");
    });

    it("should handle MCP tool isError true with text", async () => {
        const mcpManager = (await import("../src/mcp/MCPClientManager")).MCPClientManager.getInstance();
        (registry as any).mcpToolsList = [{ name: "mcp_tool", _serverId: "test_srv" }];
        vi.mocked(mcpManager.executeTool).mockResolvedValueOnce({
            isError: true,
            content: [{ type: "text", text: "mcp_error_msg" }]
        });
        await expect(registry.executeSkill("mcp_tool", {})).rejects.toThrow("mcp_error_msg");
    });

    it("should handle MCP tool isError true without text", async () => {
        const mcpManager = (await import("../src/mcp/MCPClientManager")).MCPClientManager.getInstance();
        (registry as any).mcpToolsList = [{ name: "mcp_tool", _serverId: "test_srv" }];
        vi.mocked(mcpManager.executeTool).mockResolvedValueOnce({
            isError: true,
            content: []
        });
        await expect(registry.executeSkill("mcp_tool", {})).rejects.toThrow("Unknown MCP Error");
    });

    it("should throw for non-existent skill", async () => {
        await expect(registry.executeSkill("nope", {})).rejects.toThrow();
    });

    it("should return combined MCP + fallback skills", () => {
        registry.registerSkill({ name: "extra", description: "x", parameters: {} });
        const skills = registry.getAllSkills();
        expect(skills.length).toBeGreaterThanOrEqual(3);
    });

    describe("Tool Attention - getSemanticTopK", () => {
        beforeEach(() => {
            mockEmbedWithTimeout.mockClear();
            mockEmbed.mockClear();
        });

        it("should return only core skills if query is empty", async () => {
            const result = await registry.getSemanticTopK("");
            expect(result.every(s => s.isCoreSkill)).toBe(true);
            expect(mockEmbedWithTimeout).not.toHaveBeenCalled();
        });

        it("should fallback to all skills if embedding fails", async () => {
            mockEmbedWithTimeout.mockRejectedValueOnce(new Error("Timeout"));
            const result = await registry.getSemanticTopK("test");
            expect(result.length).toBe(registry.getAllSkills().length);
        });

        it("should return core skills + qualified tools (score >= 0.65)", async () => {
            registry.registerSkill({ name: "core_skill", description: "core", parameters: {}, isCoreSkill: true });
            registry.registerSkill({ name: "low_score_skill", description: "low", parameters: {} });
            registry.registerSkill({ name: "high_score_skill", description: "high", parameters: {} });

            // Mock query vector
            mockEmbedWithTimeout.mockResolvedValue([1, 0]);
            
            // Mock desc vectors. Cosine similarity with [1, 0]:
            // [1, 0] -> 1.0 (>= 0.65)
            // [0, 1] -> 0.0 (< 0.65)
            mockEmbed.mockImplementation((text: string) => {
                if (text.includes("high")) return Promise.resolve([1, 0]);
                return Promise.resolve([0, 1]);
            });

            const result = await registry.getSemanticTopK("find high score");

            const names = result.map(s => s.name);
            expect(names).toContain("core_skill"); // Core skill always included
            expect(names).toContain("high_score_skill"); // Score 1.0 >= 0.65
            expect(names).not.toContain("low_score_skill"); // Score 0.0 < 0.65
    });

        it("should fallback to description.substring when short_desc is missing", async () => {
            registry.registerSkill({ name: "no_short", description: "long description text", parameters: {} });
            
            mockEmbedWithTimeout.mockResolvedValue([1, 0]);
            mockEmbed.mockImplementation((text: string) => {
                return Promise.resolve([1, 0]);
            });

            await registry.getSemanticTopK("query");
            expect(mockEmbed).toHaveBeenCalledWith("long description text");
        });

        it("should cache embeddings and not re-embed the same skill (LRU Cache)", async () => {
            registry.registerSkill({ name: "cache_test", description: "test cache", parameters: {} });
            mockEmbedWithTimeout.mockResolvedValue([1, 0]);
            mockEmbed.mockResolvedValue([1, 0]);

            // First call - should embed all non-core skills (e.g. read_file + cache_test)
            await registry.getSemanticTopK("query 1");
            const firstCallCount = mockEmbed.mock.calls.length;
            expect(firstCallCount).toBeGreaterThan(0);

            // Second call - should use cache, not embed again
            await registry.getSemanticTopK("query 2");
            expect(mockEmbed).toHaveBeenCalledTimes(firstCallCount);
        });
    });
});