import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

const mockStat = vi.fn();
const mockReaddir = vi.fn();
const mockMkdir = vi.fn().mockResolvedValue(undefined);
const mockRename = vi.fn().mockResolvedValue(undefined);

vi.mock("node:fs/promises", () => ({
    stat: (...args: any[]) => mockStat(...args),
    readdir: (...args: any[]) => mockReaddir(...args),
    mkdir: (...args: any[]) => mockMkdir(...args),
    rename: (...args: any[]) => mockRename(...args)
}));

import { execute, metadata } from "../../../src/skills/data/FileOrganizer";

describe("Skill - FileOrganizer", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("file_organizer");
    });

    it("should organize files by category", async () => {
        // First stat call is for directory validation
        mockStat.mockResolvedValueOnce({ isDirectory: () => true, isFile: () => false });
        // Then stat for each file
        mockStat.mockResolvedValueOnce({ isFile: () => true, isDirectory: () => false });
        mockStat.mockResolvedValueOnce({ isFile: () => true, isDirectory: () => false });
        mockStat.mockResolvedValueOnce({ isFile: () => true, isDirectory: () => false });
        mockReaddir.mockResolvedValue(["photo.jpg", "report.pdf", "song.mp3"]);

        const result = await execute({ targetDirectory: "C:\\Users\\test\\Downloads" });
        expect(result).toContain("ORGANIZER SUCCESS");
        expect(result).toContain("3");
    });

    it("should return 'already clean' when no movable files", async () => {
        mockStat.mockResolvedValueOnce({ isDirectory: () => true, isFile: () => false }); // dir check
        mockStat.mockResolvedValueOnce({ isFile: () => false, isDirectory: () => true }); // subfolder
        mockReaddir.mockResolvedValue(["subfolder"]);

        const result = await execute({ targetDirectory: "C:\\Users\\test" });
        expect(result).toContain("đã gọn gàng");
    });

    it("should skip hidden files", async () => {
        mockStat.mockResolvedValueOnce({ isDirectory: () => true, isFile: () => false });
        mockStat.mockResolvedValueOnce({ isFile: () => true, isDirectory: () => false });
        mockReaddir.mockResolvedValue([".hidden"]);

        const result = await execute({ targetDirectory: "C:\\Users\\test" });
        expect(result).toContain("đã gọn gàng");
    });

    it("should put unknown extensions in Others", async () => {
        mockStat.mockResolvedValueOnce({ isDirectory: () => true, isFile: () => false });
        mockStat.mockResolvedValueOnce({ isFile: () => true, isDirectory: () => false });
        mockReaddir.mockResolvedValue(["data.xyz"]);

        const result = await execute({ targetDirectory: "C:\\Users\\test" });
        expect(result).toContain("Others: 1");
    });

    it("should handle directory not found", async () => {
        mockStat.mockRejectedValueOnce(new Error("ENOENT"));
        const result = await execute({ targetDirectory: "C:\\nonexistent" });
        expect(result).toContain("ORGANIZER ERROR");
    });

    it("should handle stat failure for individual file", async () => {
        mockStat.mockResolvedValueOnce({ isDirectory: () => true, isFile: () => false });
        mockStat.mockRejectedValueOnce(new Error("stat fail")); // file stat fail
        mockReaddir.mockResolvedValue(["broken.txt"]);

        const result = await execute({ targetDirectory: "C:\\Users\\test" });
        expect(result).toContain("đã gọn gàng");
    });

    it("should handle ZodError", async () => {
        const result = await execute({});
        expect(result).toContain("ORGANIZER ERROR");
        expect(result).toContain("Sai định dạng");
    });

    it("should skip files without extension", async () => {
        mockStat.mockResolvedValueOnce({ isDirectory: () => true, isFile: () => false });
        mockStat.mockResolvedValueOnce({ isFile: () => true, isDirectory: () => false });
        mockReaddir.mockResolvedValue(["noext"]);

        const result = await execute({ targetDirectory: "C:\\Users\\test" });
        expect(result).toContain("đã gọn gàng");
    });
});
