import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

// Mock fs/promises stat
const mockStat = vi.fn();
vi.mock("node:fs/promises", () => ({
    stat: (...args: any[]) => mockStat(...args)
}));

// Mock fs.createReadStream - we will control events manually
const mockCreateReadStream = vi.fn();
vi.mock("node:fs", () => ({
    createReadStream: (...args: any[]) => mockCreateReadStream(...args)
}));

// Mock readline.createInterface
const mockCreateInterface = vi.fn();
vi.mock("node:readline", () => ({
    createInterface: (...args: any[]) => mockCreateInterface(...args)
}));

import { execute, metadata } from "../../../src/skills/devops/LogAnalyzer";
import { EventEmitter } from "node:events";

describe("Skill - LogAnalyzer", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata", () => {
        expect(metadata.name).toBe("log_analyzer");
    });

    it("should read last lines from log file", async () => {
        mockStat.mockResolvedValueOnce({ isFile: () => true, size: 5000 });

        const mockStream = new EventEmitter();
        const mockRl = new EventEmitter();
        mockCreateReadStream.mockReturnValue(mockStream);
        mockCreateInterface.mockReturnValue(mockRl);

        const promise = execute({ filePath: "app.log", lines: 3 });

        // Wait a tick for the promise to set up the stream
        await new Promise(r => setTimeout(r, 10));

        for (let i = 0; i < 5; i++) mockRl.emit("line", `Line ${i}`);
        mockRl.emit("close");

        const result = await promise;
        expect(result).toContain("LOG ANALYZER SUCCESS");
        expect(result).toContain("app.log");
    });

    it("should filter by keyword", async () => {
        mockStat.mockResolvedValueOnce({ isFile: () => true, size: 5000 });

        const mockStream = new EventEmitter();
        const mockRl = new EventEmitter();
        mockCreateReadStream.mockReturnValue(mockStream);
        mockCreateInterface.mockReturnValue(mockRl);

        const promise = execute({ filePath: "app.log", keyword: "ERROR" });
        await new Promise(r => setTimeout(r, 10));

        mockRl.emit("line", "[INFO] ok");
        mockRl.emit("line", "[ERROR] crash");
        mockRl.emit("close");

        const result = await promise;
        expect(result).toContain("ERROR");
        expect(result).toContain("crash");
    });

    it("should handle readline error", async () => {
        mockStat.mockResolvedValueOnce({ isFile: () => true, size: 1000 });

        const mockStream = new EventEmitter();
        const mockRl = new EventEmitter();
        mockCreateReadStream.mockReturnValue(mockStream);
        mockCreateInterface.mockReturnValue(mockRl);

        const promise = execute({ filePath: "bad.log" });
        await new Promise(r => setTimeout(r, 10));

        mockRl.emit("error", new Error("Read fail"));

        await expect(promise).rejects.toThrow("Lỗi đọc stream");
    });

    it("should handle ZodError", async () => {
        const result = await execute({});
        expect(result).toContain("LOG ERROR");
        expect(result).toContain("Sai định dạng");
    });

    it("should handle stat error", async () => {
        mockStat.mockRejectedValueOnce(new Error("ENOENT"));
        const result = await execute({ filePath: "missing.log" });
        expect(result).toContain("LOG ERROR");
    });

    it("should reject when path is not a file", async () => {
        mockStat.mockResolvedValueOnce({ isFile: () => false, size: 0 });
        const result = await execute({ filePath: "/some/dir" });
        expect(result).toContain("LOG ERROR");
    });
});
