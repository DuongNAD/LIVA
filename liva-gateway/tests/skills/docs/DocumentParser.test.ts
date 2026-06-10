import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

const mockIngestFile = vi.fn();
vi.mock("@services/RAGIngestionPipeline", () => ({
    RAGIngestionPipeline: {
        getInstance: vi.fn().mockReturnValue({
            ingestFile: (...args: any[]) => mockIngestFile(...args)
        })
    }
}));

const mockAccess = vi.fn();
vi.mock("node:fs/promises", () => ({
    access: (...args: any[]) => mockAccess(...args)
}));

import { execute, metadata } from "../../../src/skills/docs/DocumentParser";

describe("Skill - DocumentParser (ingest_document)", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata with renamed name", () => {
        expect(metadata.name).toBe("ingest_document");
        expect(metadata.search_keywords).toContain("Markdown");
    });

    it("should successfully ingest PDF document", async () => {
        mockAccess.mockResolvedValue(undefined);
        mockIngestFile.mockResolvedValue({
            success: true,
            filePath: "/dummy/report.pdf",
            numPages: 5,
            numChunks: 12,
            previewText: "[Trang 1] Content",
            processingTimeMs: 150
        });

        const result = await execute({ filePath: "report.pdf" });
        expect(result).toContain("[DOCUMENT INGESTION SUCCESS]");
        expect(result).toContain("Tổng số trang: 5");
        expect(result).toContain("Đã nhúng 12 chunks");
        expect(result).toContain("[Trang 1] Content");
    });

    it("should successfully ingest Markdown document without page numbers", async () => {
        mockAccess.mockResolvedValue(undefined);
        mockIngestFile.mockResolvedValue({
            success: true,
            filePath: "/dummy/readme.md",
            numChunks: 4,
            previewText: "# Heading",
            processingTimeMs: 40
        });

        const result = await execute({ filePath: "readme.md" });
        expect(result).toContain("[DOCUMENT INGESTION SUCCESS]");
        expect(result).not.toContain("Tổng số trang:");
        expect(result).toContain("Đã nhúng 4 chunks");
        expect(result).toContain("# Heading");
    });

    it("should handle file not found (fs.access rejects)", async () => {
        mockAccess.mockRejectedValue(new Error("ENOENT"));
        const result = await execute({ filePath: "missing.pdf" });
        expect(result).toContain("[DOCUMENT ERROR]");
    });

    it("should handle ZodError for empty filePath", async () => {
        const result = await execute({ filePath: "" });
        expect(result).toContain("[DOCUMENT ERROR]");
    });

    it("should handle ingestion pipeline failure", async () => {
        mockAccess.mockResolvedValue(undefined);
        mockIngestFile.mockResolvedValue({
            success: false,
            error: "Mock ingestion pipeline error"
        });

        const result = await execute({ filePath: "fail.pdf" });
        expect(result).toContain("[DOCUMENT ERROR] Lỗi hệ thống: Mock ingestion pipeline error");
    });
});
