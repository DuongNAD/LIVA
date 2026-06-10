import { describe, it, expect, vi, beforeEach } from "vitest";
import { RAGIngestionPipeline } from "../../src/services/RAGIngestionPipeline";
import { StructuredMemory } from "../../src/memory/StructuredMemory";
import { EmbeddingService } from "../../src/services/EmbeddingService";
import { promises as fs } from "node:fs";

vi.mock("../../src/memory/StructuredMemory", () => {
    return {
        StructuredMemory: {
            create: vi.fn().mockImplementation(() => {
                return {
                    upsertVectorsBatch: vi.fn().mockResolvedValue(undefined),
                };
            }),
        },
    };
});

vi.mock("../../src/services/EmbeddingService", () => {
    const mockEmbedBatch = vi.fn().mockImplementation((texts: string[]) => {
        return Promise.resolve(texts.map(() => new Array(384).fill(0.1)));
    });
    return {
        EmbeddingService: {
            getInstance: vi.fn().mockReturnValue({
                embedBatch: mockEmbedBatch,
                ready: true,
            }),
        },
    };
});

vi.mock("node:fs", () => {
    return {
        promises: {
            access: vi.fn().mockResolvedValue(undefined),
            readFile: vi.fn().mockResolvedValue("# Test File\n\nHere is content."),
        },
    };
});

describe("RAGIngestionPipeline", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it("should successfully ingest Markdown/text files", async () => {
        const pipeline = RAGIngestionPipeline.getInstance();
        const result = await pipeline.ingestFile("dummy.md");

        expect(result.success).toBe(true);
        expect(result.numChunks).toBe(1);
        expect(result.filePath).toContain("dummy.md");
        expect(result.processingTimeMs).toBeGreaterThanOrEqual(0);
    });

    it("should gracefully handle non-existent files", async () => {
        vi.spyOn(fs, "access").mockRejectedValue(new Error("File not found"));
        const pipeline = RAGIngestionPipeline.getInstance();
        const result = await pipeline.ingestFile("missing.txt");

        expect(result.success).toBe(false);
        expect(result.numChunks).toBe(0);
        expect(result.error).toContain("File not found");
    });
});
