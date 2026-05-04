import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("@memory/LanceMemory", () => ({
    LanceMemoryManager: class {
        addMemory = vi.fn().mockResolvedValue(undefined);
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
        const longText = "A".repeat(60); // > 50 chars to trigger lance memory
        const mockPage = {
            getTextContent: vi.fn().mockResolvedValue({
                items: [{ str: longText }]
            })
        };
        mockGetDocument.mockReturnValue({
            promise: Promise.resolve({
                numPages: 2,
                getPage: vi.fn().mockResolvedValue(mockPage)
            })
        });

        const result = await execute({ filePath: "report.pdf" });
        expect(result).toContain("PDF PARSE");
        expect(result).toContain("report.pdf");
        expect(result).toContain("2");
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
        await expect(execute({ filePath: "corrupt.pdf" })).rejects.toThrow("Corrupted PDF");
    });
});
