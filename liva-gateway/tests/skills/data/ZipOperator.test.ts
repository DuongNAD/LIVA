import { describe, it, expect, vi, beforeEach } from "vitest";
import { promisify } from "node:util";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

const { mockExecAsync } = vi.hoisted(() => ({
    mockExecAsync: vi.fn().mockResolvedValue({ stdout: "", stderr: "" })
}));

vi.mock("node:child_process", () => {
    const execFn = (...args: any[]) => {};
    (execFn as any)[promisify.custom] = mockExecAsync;
    return { exec: execFn };
});

import { execute, metadata } from "../../../src/skills/data/ZipOperator";

describe("Skill - ZipOperator", () => {
    beforeEach(() => { vi.clearAllMocks(); mockExecAsync.mockResolvedValue({ stdout: "", stderr: "" }); });

    it("should export metadata", () => { expect(metadata.name).toBe("zip_operator"); });

    it("should compress folder", async () => {
        const result = await execute({ action: "compress", sourcePath: "src", destinationPath: "out.zip" });
        expect(result).toContain("ZIP SUCCESS");
        expect(result).toContain("Nén");
    });

    it("should extract archive", async () => {
        const result = await execute({ action: "extract", sourcePath: "archive.zip", destinationPath: "dest" });
        expect(result).toContain("ZIP SUCCESS");
        expect(result).toContain("Giải nén");
    });

    it("should handle exec error", async () => {
        mockExecAsync.mockRejectedValueOnce(new Error("PowerShell error"));
        const result = await execute({ action: "compress", sourcePath: "src", destinationPath: "out.zip" });
        expect(result).toContain("ZIP ERROR");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ action: "invalid" });
        expect(result).toContain("ZIP ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
