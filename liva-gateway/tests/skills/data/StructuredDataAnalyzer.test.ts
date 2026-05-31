import { describe, it, expect, vi, beforeEach } from "vitest";
import * as fs from "node:fs";
import * as readline from "node:readline";
import { EventEmitter } from "node:events";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

// Mock fs.createReadStream
vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    return { ...actual, createReadStream: vi.fn() };
});

// Mock readline
vi.mock("node:readline", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:readline")>();
    return { ...actual, createInterface: vi.fn() };
});

import { execute, metadata } from "../../../src/skills/data/StructuredDataAnalyzer";

describe("Skill - StructuredDataAnalyzer", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("analyze_structured_data");
        expect(metadata.parameters.required).toContain("filePath");
    });

    it("should stream CSV and return summary with numeric profiling", async () => {
        const csvLines = [
            "Name,Age,Score",
            "Alice,30,95.5",
            "Bob,25,88.0",
            "Charlie,,72.3",
            "Dave,40,",
            "Eve,35,91.0"
        ];

        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "test.csv" });

        // Emit lines
        for (const line of csvLines) { mockRl.emit("line", line); }
        mockRl.emit("close");

        const result = await promise;
        expect(result).toContain("DATA ANALYSIS SUMMARY");
        expect(result).toContain("test.csv");
        expect(result).toContain("Name");
        expect(result).toContain("Age");
        // Should count nulls
        const parsed = JSON.parse(result.match(/```json\n([\s\S]*?)\n```/)?.[1] || "{}");
        expect(parsed.total_data_rows).toBe(5);
        expect(parsed.null_counts.Score).toBe(1); // Dave has empty Score
    });

    it("should handle custom delimiter (tab)", async () => {
        const tsvLines = [
            "Name\tAge",
            "Alice\t30",
            "Bob\t25",
        ];
        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "test.tsv", delimiter: "\t" });
        for (const line of tsvLines) { mockRl.emit("line", line); }
        mockRl.emit("close");

        const result = await promise;
        expect(result).toContain("test.tsv");
    });

    it("should handle readline error", async () => {
        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "bad.csv" });
        mockRl.emit("error", new Error("Read error"));

        await expect(promise).rejects.toThrow("Read error");
    });

    it("should handle fileStream ENOENT error", async () => {
        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "missing.csv" });
        mockStream.emit("error", Object.assign(new Error("ENOENT"), { code: "ENOENT" }));

        await expect(promise).rejects.toThrow("Không tìm thấy file");
    });

    it("should handle fileStream generic error", async () => {
        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "corrupt.csv" });
        mockStream.emit("error", Object.assign(new Error("Disk failure"), { code: "EIO" }));

        await expect(promise).rejects.toThrow("Lỗi mở file");
    });

    it("should handle empty CSV (header only)", async () => {
        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "empty.csv" });
        mockRl.emit("line", "Col1,Col2");
        mockRl.emit("close");

        const result = await promise;
        const parsed = JSON.parse(result.match(/```json\n([\s\S]*?)\n```/)?.[1] || "{}");
        expect(parsed.total_data_rows).toBe(0);
    });

    it("should count NaN and null variants", async () => {
        const mockRl = new EventEmitter();
        const mockStream = new EventEmitter();
        vi.mocked(fs.createReadStream).mockReturnValue(mockStream as any);
        vi.mocked(readline.createInterface).mockReturnValue(mockRl as any);

        const promise = execute({ filePath: "nulls.csv" });
        mockRl.emit("line", "A,B,C");
        mockRl.emit("line", "null,NaN,-");
        mockRl.emit("line", '"","",');
        mockRl.emit("close");

        const result = await promise;
        const parsed = JSON.parse(result.match(/```json\n([\s\S]*?)\n```/)?.[1] || "{}");
        expect(parsed.null_counts.A).toBeGreaterThanOrEqual(1);
    });
});
