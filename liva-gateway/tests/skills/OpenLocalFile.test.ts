/**
 * OpenLocalFile.test.ts — File opening skill unit tests
 * Tests metadata, Zod validation, and exec error handling
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock child_process.exec — promisify will wrap this into Promise<{stdout, stderr}>
const { execMockFn } = vi.hoisted(() => {
    // Node's promisify for exec uses a custom symbol to return {stdout, stderr}
    // We emulate this by attaching [Symbol.for("nodejs.util.promisify.custom")]
    const execMockFn: any = vi.fn();
    // Custom promisify: returns a function that yields Promise<{stdout, stderr}>
    execMockFn[Symbol.for("nodejs.util.promisify.custom")] = vi.fn().mockResolvedValue({ stdout: "", stderr: "" });
    return { execMockFn };
});

vi.mock("node:child_process", () => ({
    exec: execMockFn,
}));

import * as OpenLocalFile from "../../src/skills/core/OpenLocalFile";

describe("OpenLocalFile Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Reset promisified exec to return clean result
        execMockFn[Symbol.for("nodejs.util.promisify.custom")].mockResolvedValue({ stdout: "", stderr: "" });
    });

    // ─── Metadata ───
    it("should have correct metadata", () => {
        expect(OpenLocalFile.metadata.name).toBe("open_local_file");
        expect(OpenLocalFile.metadata.parameters.required).toContain("targetPath");
    });

    // ─── Happy Path ───
    it("should return success message for valid file path", async () => {
        const result = await OpenLocalFile.execute({ targetPath: "D:/test/file.txt" });
        expect(result).toContain("File opened successfully");
    });

    // ─── Validation: empty targetPath ───
    it("should return ValidationError for empty targetPath", async () => {
        const result = await OpenLocalFile.execute({ targetPath: "" });
        expect(result).toContain("[ValidationError]");
        expect(result).toContain("targetPath is required");
    });

    // ─── Validation: missing targetPath ───
    it("should return ValidationError when targetPath is undefined", async () => {
        const result = await OpenLocalFile.execute({});
        expect(result).toContain("[ValidationError]");
    });

    // ─── Exec Failure ───
    it("should return 'Failed to open file' when exec throws", async () => {
        execMockFn[Symbol.for("nodejs.util.promisify.custom")].mockRejectedValueOnce(
            new Error("Command not found")
        );

        const result = await OpenLocalFile.execute({ targetPath: "D:/nonexistent/file.exe" });
        expect(result).toContain("Failed to open file");
        expect(result).toContain("Command not found");
    });

    // ─── OS Warning (stderr) ───
    it("should return OS warning when stderr is non-empty", async () => {
        execMockFn[Symbol.for("nodejs.util.promisify.custom")].mockResolvedValueOnce({
            stdout: "",
            stderr: "Access is denied.",
        });

        const result = await OpenLocalFile.execute({ targetPath: "D:/test/file.txt" });
        expect(result).toContain("Command executed with OS warning");
        expect(result).toContain("Access is denied.");
    });
});
