import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================================
// Mock fs — NEVER touch real filesystem (user's critical advice)
// ============================================================
vi.mock("fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeFile: vi.fn().mockResolvedValue(undefined),
    rename: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

vi.mock("../../src/utils/FileUtils", () => ({
    safeRename: vi.fn()
}));
import { safeRename } from "../../src/utils/FileUtils";

import * as fsp from "fs/promises";
import { execute, metadata } from "../../src/skills/core/WriteLocalFile";

const mockWriteFile = vi.mocked(fsp.writeFile);
const mockRename = vi.mocked(fsp.rename);
const mockMkdir = vi.mocked(fsp.mkdir);

// ============================================================
// Tests
// ============================================================
describe("WriteLocalFile Skill", () => {
    beforeEach(() => {
        vi.resetAllMocks();
        mockMkdir.mockResolvedValue(undefined);
        mockWriteFile.mockResolvedValue(undefined);
        mockRename.mockResolvedValue(undefined);
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("write_local_file");
        });

        it("should require filePath and content parameters", () => {
            expect(metadata.parameters.required).toContain("filePath");
            expect(metadata.parameters.required).toContain("content");
        });
    });

    describe("Successful Write", () => {
        it("should write file using atomic pattern (.tmp + rename)", async () => {
            const result = await execute({ filePath: "test/output.txt", content: "Hello World" });

            // Verify atomic write: writeFile to .tmp, then rename
            expect(mockWriteFile).toHaveBeenCalledOnce();
            const writtenPath = mockWriteFile.mock.calls[0][0] as string;
            expect(writtenPath).toContain(".tmp");

            expect(safeRename).toHaveBeenCalledOnce();
            expect(result).toContain("successfully");
        });

        it("should create parent directories before writing", async () => {
            await execute({ filePath: "deep/nested/dir/file.txt", content: "content" });
            expect(mockMkdir).toHaveBeenCalledWith(expect.any(String), { recursive: true });
        });
    });

    describe("Path Security Guardrails", () => {
        it("should BLOCK writing to C:\\Windows", async () => {
            const result = await execute({ filePath: "C:\\Windows\\malicious.bat", content: "bad" });
            expect(result).toContain("SECURITY_ERROR");
            expect(mockWriteFile).not.toHaveBeenCalled();
        });

        it("should BLOCK writing to C:\\Program Files", async () => {
            const result = await execute({ filePath: "C:\\Program Files\\app\\hack.dll", content: "bad" });
            expect(result).toContain("SECURITY_ERROR");
            expect(mockWriteFile).not.toHaveBeenCalled();
        });

        it("should BLOCK writing to C:\\Program Files (x86)", async () => {
            const result = await execute({ filePath: "C:\\Program Files (x86)\\app\\test.exe", content: "bad" });
            expect(result).toContain("SECURITY_ERROR");
            expect(mockWriteFile).not.toHaveBeenCalled();
        });

        it("should BLOCK writing to C:\\ProgramData", async () => {
            const result = await execute({ filePath: "C:\\ProgramData\\evil.bat", content: "bad" });
            expect(result).toContain("SECURITY_ERROR");
            expect(mockWriteFile).not.toHaveBeenCalled();
        });

        it("should BLOCK writing directly to C:\\ root", async () => {
            const result = await execute({ filePath: "C:\\", content: "bad" });
            expect(result).toContain("SECURITY_ERROR");
            expect(mockWriteFile).not.toHaveBeenCalled();
        });

        it("should ALLOW writing to user project directories", async () => {
            const result = await execute({ filePath: "output/report.md", content: "Safe content" });
            expect(result).toContain("successfully");
            expect(mockWriteFile).toHaveBeenCalled();
        });
    });

    describe("Error Handling", () => {
        it("should return error message when write fails", async () => {
            mockWriteFile.mockRejectedValueOnce(new Error("EACCES: permission denied"));
            const result = await execute({ filePath: "test.txt", content: "test" });
            expect(result).toContain("error");
            expect(result).toContain("EACCES");
        });

        it("should return error message when rename fails", async () => {
            vi.mocked(safeRename).mockRejectedValueOnce(new Error("EXDEV: cross-device link"));
            const result = await execute({ filePath: "test.txt", content: "test" });
            expect(result).toContain("error");
        });
    });
});
