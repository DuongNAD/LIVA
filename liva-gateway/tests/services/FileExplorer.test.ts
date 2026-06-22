import { describe, it, expect, vi, beforeEach } from "vitest";
import * as path from "node:path";

// Mock logger using correct relative path to src/utils/logger from tests/services/FileExplorer.test.ts
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock fs/promises
const mockStat = vi.fn();
const mockReaddir = vi.fn();
const mockReadFile = vi.fn();

vi.mock("node:fs/promises", () => ({
    stat: (...args: any[]) => mockStat(...args),
    readdir: (...args: any[]) => mockReaddir(...args),
    readFile: (...args: any[]) => mockReadFile(...args),
}));

import { FileExplorer } from "../../src/services/FileExplorer";

describe("FileExplorer Service", () => {
    const basePath = path.resolve("/workspace");
    let explorer: FileExplorer;

    beforeEach(() => {
        vi.clearAllMocks();
        mockStat.mockReset();
        mockReaddir.mockReset();
        mockReadFile.mockReset();
        explorer = new FileExplorer(basePath);
    });

    describe("chroot jail (resolveAndJail)", () => {
        it("should allow paths inside base path", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => false,
                size: 100,
            });
            mockReadFile.mockResolvedValueOnce("file content");

            const result = await explorer.readFile("sub/file.txt");
            expect(result).toBe("file content");
            expect(mockReadFile).toHaveBeenCalledWith(path.resolve(basePath, "sub/file.txt"), "utf-8");
        });

        it("should deny path traversal escaping base path", async () => {
            await expect(explorer.readFile("../secret.txt")).rejects.toThrow(
                "Access Denied: Path is outside of allowed workspace."
            );
            await expect(explorer.listDirectory("../../etc")).rejects.toThrow(
                "Access Denied: Path is outside of allowed workspace."
            );
        });

        it("should deny absolute paths escaping base path", async () => {
            await expect(explorer.readFile("sub/../../etc/passwd")).rejects.toThrow(
                "Access Denied: Path is outside of allowed workspace."
            );
        });

        it("should deny sibling path traversal escaping base path", async () => {
            await expect(explorer.readFile("../workspace-extra/secret.txt")).rejects.toThrow(
                "Access Denied: Path is outside of allowed workspace."
            );
        });
    });

    describe("listDirectory", () => {
        it("should throw error if target path is not a directory", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => false,
            });

            await expect(explorer.listDirectory("file.txt")).rejects.toThrow("Not a directory");
        });

        it("should return sorted files and directories correctly", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => true,
            });

            const mockFiles = [
                { name: "b_file.txt", isDirectory: () => false },
                { name: "a_dir", isDirectory: () => true },
                { name: "c_dir", isDirectory: () => true },
                { name: "a_file.txt", isDirectory: () => false },
            ];
            mockReaddir.mockResolvedValueOnce(mockFiles);

            mockStat
                .mockResolvedValueOnce({ size: 10 })
                .mockResolvedValueOnce({ size: 0 })
                .mockResolvedValueOnce({ size: 0 })
                .mockResolvedValueOnce({ size: 20 });

            const result = await explorer.listDirectory("my-dir");

            expect(result).toEqual([
                { name: "a_dir", isDirectory: true, size: 0 },
                { name: "c_dir", isDirectory: true, size: 0 },
                { name: "a_file.txt", isDirectory: false, size: 20 },
                { name: "b_file.txt", isDirectory: false, size: 10 },
            ]);
        });

        it("should skip files that fail stat and handle errors", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => true,
            });

            const mockFiles = [
                { name: "accessible.txt", isDirectory: () => false },
                { name: "inaccessible.txt", isDirectory: () => false },
            ];
            mockReaddir.mockResolvedValueOnce(mockFiles);

            mockStat
                .mockResolvedValueOnce({ size: 50 })
                .mockRejectedValueOnce(new Error("Permission denied"));

            const result = await explorer.listDirectory("");
            expect(result).toEqual([
                { name: "accessible.txt", isDirectory: false, size: 50 },
            ]);
        });
    });

    describe("readFile", () => {
        it("should throw error when trying to read a directory", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => true,
            });

            await expect(explorer.readFile("some-dir")).rejects.toThrow(
                "Cannot read a directory as file"
            );
        });

        it("should throw error when file exceeds 5MB limit", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => false,
                size: 6 * 1024 * 1024,
            });

            await expect(explorer.readFile("huge.zip")).rejects.toThrow("File too large");
        });

        it("should return content for valid files", async () => {
            mockStat.mockResolvedValueOnce({
                isDirectory: () => false,
                size: 1024,
            });
            mockReadFile.mockResolvedValueOnce("hello world");

            const content = await explorer.readFile("note.txt");
            expect(content).toBe("hello world");
        });
    });
});
