import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================================
// Mock fs — NEVER touch real filesystem
// ============================================================
vi.mock("fs/promises", () => ({
    readFile: vi.fn(),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import * as fsp from "fs/promises";
import { execute, metadata } from "../../src/skills/core/ReadLocalFile";

const mockReadFile = vi.mocked(fsp.readFile);

// ============================================================
// Tests
// ============================================================
describe("ReadLocalFile Skill", () => {
    beforeEach(() => {
        vi.resetAllMocks();
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("read_local_file");
        });

        it("should require filePath parameter", () => {
            expect(metadata.parameters.required).toContain("filePath");
        });
    });

    describe("Successful Read", () => {
        it("should return file content wrapped in descriptive text", async () => {
            mockReadFile.mockResolvedValueOnce("Hello World\nLine 2" as any);
            const result = await execute({ filePath: "test.txt" });
            expect(result).toContain("File content");
            expect(result).toContain("Hello World");
            expect(result).toContain("Line 2");
        });

        it("should handle empty files", async () => {
            mockReadFile.mockResolvedValueOnce("" as any);
            const result = await execute({ filePath: "empty.txt" });
            expect(result).toContain("File content");
        });

        it("should handle files with Unicode content", async () => {
            mockReadFile.mockResolvedValueOnce("Xin chào thế giới 🌍" as any);
            const result = await execute({ filePath: "unicode.txt" });
            expect(result).toContain("Xin chào thế giới");
        });
    });

    describe("Error Handling", () => {
        it("should return error message for non-existent file", async () => {
            mockReadFile.mockRejectedValueOnce(new Error("ENOENT: no such file or directory"));
            const result = await execute({ filePath: "nonexistent.txt" });
            expect(result).toContain("error");
            expect(result).toContain("ENOENT");
        });

        it("should return error message for permission denied", async () => {
            mockReadFile.mockRejectedValueOnce(new Error("EACCES: permission denied"));
            const result = await execute({ filePath: "/etc/shadow" });
            expect(result).toContain("error");
            expect(result).toContain("EACCES");
        });
    });
});
