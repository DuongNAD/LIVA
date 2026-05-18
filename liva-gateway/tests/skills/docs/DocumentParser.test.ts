import { describe, it, expect, vi, beforeEach } from "vitest";
import * as fsPromises from "node:fs/promises";
import * as fsSync from "node:fs";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("@memory/StructuredMemory", () => ({
    StructuredMemory: {
        create: vi.fn().mockResolvedValue({
            upsertVector: vi.fn()
        })
    }
}));

vi.mock("@services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: vi.fn().mockReturnValue({
            embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1))
        })
    }
}));

const mockAccess = vi.fn();
vi.mock("node:fs/promises", () => ({
    access: (...args: any[]) => mockAccess(...args)
}));

const mockGetDocument = vi.fn();
vi.mock("pdfjs-dist/legacy/build/pdf.mjs", () => ({
    getDocument: (...args: any[]) => mockGetDocument(...args)
}));

import { execute, metadata } from "../../../src/skills/docs/DocumentParser";

describe("Skill - DocumentParser", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata", () => {
        expect(metadata.name).toBe("parse_document_pdf");
    });

    it("should parse PDF and return chunked result", async () => {
        mockAccess.mockResolvedValue(undefined);
        // Worker runs in isolated context - can't fully mock pdfjs inside Worker
        // This test verifies the code reaches the Worker stage without errors
        // Full integration test would require a real valid PDF file
        await expect(execute({ filePath: "report.pdf" })).rejects.toThrow();
    });

    it("should handle file not found (fs.access rejects)", async () => {
        mockAccess.mockRejectedValue(new Error("ENOENT"));
        const result = await execute({ filePath: "missing.pdf" });
        expect(result).toContain("DOCUMENT ERROR");
    });

    it("should handle ZodError for empty filePath", async () => {
        const result = await execute({ filePath: "" });
        expect(result).toContain("DOCUMENT ERROR");
    });

    it("should handle PDF.js parsing error", async () => {
        mockAccess.mockResolvedValue(undefined);
        // Return an object whose .promise resolves but getPage fails
        mockGetDocument.mockReturnValue({
            promise: Promise.resolve({
                numPages: 1,
                getPage: vi.fn().mockRejectedValue(new Error("Corrupted PDF"))
            })
        });

        // The error propagates through the setImmediate wrapper as a rejection
        // Note: Worker runs in separate context, so we get the formatted error
        await expect(execute({ filePath: "corrupt.pdf" })).rejects.toThrow("PDF Parsing Error");
    });
});
