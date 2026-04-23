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

vi.mock("../src/skills/GeminiSurfer.js", () => ({
    metadata: { name: "gemini_surfer", description: "Mock", parameters: {} },
    execute: vi.fn().mockResolvedValue("ok"),
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

    it("should register and execute custom fallback skill", async () => {
        const exec = vi.fn().mockResolvedValue("done");
        registry.registerSkill({ name: "custom", description: "x", parameters: {}, execute: exec });
        const result = await registry.executeSkill("custom", { a: 1 });
        expect(result).toBe("done");
        expect(exec).toHaveBeenCalledWith({ a: 1 });
    });

    it("should throw for non-existent skill", async () => {
        await expect(registry.executeSkill("nope", {})).rejects.toThrow();
    });

    it("should return combined MCP + fallback skills", () => {
        registry.registerSkill({ name: "extra", description: "x", parameters: {} });
        const skills = registry.getAllSkills();
        expect(skills.length).toBeGreaterThanOrEqual(3);
    });
});
