import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================================
// Mock fs — NEVER touch real filesystem
// ============================================================
vi.mock("fs/promises", () => ({
    readdir: vi.fn(),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import * as fsp from "fs/promises";
import { execute, metadata } from "../../src/skills/core/ListDirectory";

const mockReaddir = vi.mocked(fsp.readdir);

// ============================================================
// Tests
// ============================================================
describe("ListDirectory Skill", () => {
    beforeEach(() => {
        vi.resetAllMocks();
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("list_directory");
        });

        it("should require targetPath parameter", () => {
            expect(metadata.parameters.required).toContain("targetPath");
        });
    });

    describe("Successful Listing", () => {
        it("should list files and folders with correct labels", async () => {
            mockReaddir.mockResolvedValueOnce([
                { name: "src", isDirectory: () => true, isFile: () => false } as any,
                { name: "package.json", isDirectory: () => false, isFile: () => true } as any,
                { name: "README.md", isDirectory: () => false, isFile: () => true } as any,
            ]);

            const result = await execute({ targetPath: "." });
            expect(result).toContain("src");
            expect(result).toContain("Thư mục");
            expect(result).toContain("package.json");
            expect(result).toContain("Tệp");
        });

        it("should handle empty directories", async () => {
            mockReaddir.mockResolvedValueOnce([]);
            const result = await execute({ targetPath: "." });
            expect(result).toContain("Cấu trúc thư mục");
        });
    });

    describe("Error Handling", () => {
        it("should return error message for non-existent directory", async () => {
            mockReaddir.mockRejectedValueOnce(new Error("ENOENT: no such file or directory"));
            const result = await execute({ targetPath: "/nonexistent" });
            expect(result).toContain("Lỗi");
            expect(result).toContain("ENOENT");
        });

        it("should return error message for permission denied", async () => {
            mockReaddir.mockRejectedValueOnce(new Error("EACCES: permission denied"));
            const result = await execute({ targetPath: "/root" });
            expect(result).toContain("Lỗi");
        });
    });
});
