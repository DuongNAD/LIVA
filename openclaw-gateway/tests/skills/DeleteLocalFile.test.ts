import { describe, it, expect, vi, beforeEach } from "vitest";

// ============================================================
// Mock fs — NEVER touch real filesystem
// ============================================================
vi.mock("fs/promises", () => ({
    unlink: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import * as fsp from "fs/promises";
import { execute, metadata } from "../../src/skills/DeleteLocalFile";

const mockUnlink = vi.mocked(fsp.unlink);

// ============================================================
// Tests
// ============================================================
describe("DeleteLocalFile Skill", () => {
    beforeEach(() => {
        vi.resetAllMocks();
        mockUnlink.mockResolvedValue(undefined);
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("delete_local_file");
        });

        it("should require filePath parameter", () => {
            expect(metadata.parameters.required).toContain("filePath");
        });
    });

    describe("Successful Deletion", () => {
        it("should delete a file in safe location", async () => {
            const result = await execute({ filePath: "temp/test-output.txt" });
            expect(mockUnlink).toHaveBeenCalledOnce();
            expect(result).toContain("thành công");
        });
    });

    describe("Path Security Guardrails — System Directories", () => {
        it("should BLOCK deletion in C:\\Windows", async () => {
            const result = await execute({ filePath: "C:\\Windows\\system32\\config\\sam" });
            expect(result).toContain("BẢO MẬT");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion in C:\\Program Files", async () => {
            const result = await execute({ filePath: "C:\\Program Files\\app\\important.dll" });
            expect(result).toContain("BẢO MẬT");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion in C:\\Program Files (x86)", async () => {
            const result = await execute({ filePath: "C:\\Program Files (x86)\\app\\lib.dll" });
            expect(result).toContain("BẢO MẬT");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion in C:\\ProgramData", async () => {
            const result = await execute({ filePath: "C:\\ProgramData\\config.xml" });
            expect(result).toContain("BẢO MẬT");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion in C:\\Users\\Default", async () => {
            const result = await execute({ filePath: "C:\\Users\\Default\\profile.dat" });
            expect(result).toContain("BẢO MẬT");
            expect(mockUnlink).not.toHaveBeenCalled();
        });
    });

    describe("Path Security Guardrails — Boot Files", () => {
        it("should BLOCK deletion of bootmgr", async () => {
            const result = await execute({ filePath: "C:\\bootmgr" });
            expect(result).toContain("BẢO MẬT");
            expect(result).toContain("Boot");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion of ntldr", async () => {
            const result = await execute({ filePath: "C:\\ntldr" });
            expect(result).toContain("Boot");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion of hiberfil.sys", async () => {
            const result = await execute({ filePath: "C:\\hiberfil.sys" });
            expect(result).toContain("Boot");
            expect(mockUnlink).not.toHaveBeenCalled();
        });

        it("should BLOCK deletion of pagefile.sys", async () => {
            const result = await execute({ filePath: "C:\\pagefile.sys" });
            expect(result).toContain("Boot");
            expect(mockUnlink).not.toHaveBeenCalled();
        });
    });

    describe("Error Handling", () => {
        it("should return error message when file does not exist", async () => {
            mockUnlink.mockRejectedValueOnce(new Error("ENOENT: no such file"));
            const result = await execute({ filePath: "nonexistent.txt" });
            expect(result).toContain("Lỗi");
            expect(result).toContain("ENOENT");
        });

        it("should return error message when permission denied", async () => {
            mockUnlink.mockRejectedValueOnce(new Error("EACCES: permission denied"));
            const result = await execute({ filePath: "locked.txt" });
            expect(result).toContain("Lỗi");
        });
    });
});
