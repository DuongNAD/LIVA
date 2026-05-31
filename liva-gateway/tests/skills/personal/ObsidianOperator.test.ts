import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("@memory/ObsidianVaultManager", () => ({
    ObsidianVaultManager: class {
        readNote = vi.fn().mockResolvedValue({ content: "# Test Note", mtimeMs: Date.now() });
        createOrOverwriteNote = vi.fn().mockResolvedValue(undefined);
        safeAppendInsights = vi.fn().mockResolvedValue(undefined);
    }
}));

import { execute, metadata } from "../../../src/skills/personal/ObsidianOperator";

describe("Skill - ObsidianOperator", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata", () => { expect(metadata.name).toBe("obsidian_operator"); });

    it("should read a note", async () => {
        const result = await execute({ action: "read", relativePath: "test.md" });
        expect(result).toContain("OBSIDIAN READ SUCCESS");
    });

    it("should create a note", async () => {
        const result = await execute({ action: "create", relativePath: "new", content: "# Hello" });
        expect(result).toContain("OBSIDIAN CREATE SUCCESS");
    });

    it("should auto-append .md extension", async () => {
        const result = await execute({ action: "read", relativePath: "test" });
        expect(result).toContain("test.md");
    });

    it("should error when create is missing content", async () => {
        const result = await execute({ action: "create", relativePath: "x.md" });
        expect(result).toContain("OBSIDIAN ERROR");
    });

    it("should append to a note", async () => {
        const result = await execute({ action: "append", relativePath: "log.md", content: "new entry" });
        expect(result).toContain("OBSIDIAN APPEND SUCCESS");
    });

    it("should error when append is missing content", async () => {
        const result = await execute({ action: "append", relativePath: "x.md" });
        expect(result).toContain("OBSIDIAN ERROR");
    });
});
