import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock jsonrepair
vi.mock("jsonrepair", () => ({
    jsonrepair: vi.fn((s: string) => s),
}));

// Mock DualChannelSegmenter
vi.mock("../../src/memory/DualChannelSegmenter", () => ({
    smartTruncate: (text: string, maxLen: number) =>
        text.length <= maxLen ? text : text.substring(0, maxLen),
}));

// Mock EmbeddingService
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: vi.fn(),
}));

// Mock HttpClient
vi.mock("../../src/utils/HttpClient", () => ({
    withSafeTimeout: vi.fn((promise: Promise<any>) => promise),
}));

// Mock openai
vi.mock("openai", () => ({
    default: vi.fn(),
}));

// Mock child_process
vi.mock("node:child_process", () => ({
    exec: vi.fn(),
}));

// Mock util
vi.mock("node:util", () => ({
    promisify: vi.fn().mockReturnValue(vi.fn().mockRejectedValue(new Error("nvidia-smi not found"))),
}));

import { ReconsolidationEngine } from "@memory/ReconsolidationEngine";

describe("ReconsolidationEngine — Conflict-Aware Memory Reconsolidation", () => {
    let engine: ReconsolidationEngine;
    let mockStructuredMemory: any;
    let mockEmbeddingService: any;
    let mockAiClient: any;

    beforeEach(() => {
        vi.clearAllMocks();

        mockStructuredMemory = {
            upsertVector: vi.fn(),
            searchAxiomsByVector: vi.fn().mockReturnValue([]),
            deleteVectorByContent: vi.fn(),
        };

        mockEmbeddingService = {
            embed: vi.fn().mockResolvedValue([0.1, 0.2, 0.3]),
        };

        mockAiClient = {
            chat: {
                completions: {
                    create: vi.fn(),
                },
            },
        };

        engine = new ReconsolidationEngine(
            mockStructuredMemory,
            mockEmbeddingService,
            mockAiClient
        );
    });

    // ============================================================
    // sweepAndReconcile() — No existing AXIOMs (independent)
    // ============================================================
    describe("sweepAndReconcile() — new axioms", () => {
        it("should add new axiom when no existing match found", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([]);

            const stats = await engine.sweepAndReconcile([
                { text: "User likes coffee", domain: "Personal", category: "Preference", trace_identifiers: ["ev_1"] },
            ]);

            expect(stats.added).toBe(1);
            expect(stats.updated).toBe(0);
            expect(stats.deleted).toBe(0);
            expect(mockStructuredMemory.upsertVector).toHaveBeenCalledTimes(1);
        });

        it("should handle multiple axioms", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([]);

            const stats = await engine.sweepAndReconcile([
                { text: "Fact A", domain: "D", category: "C", trace_identifiers: [] },
                { text: "Fact B", domain: "D", category: "C", trace_identifiers: [] },
                { text: "Fact C", domain: "D", category: "C", trace_identifiers: [] },
            ]);

            expect(stats.added).toBe(3);
        });

        it("should handle empty axioms array", async () => {
            const stats = await engine.sweepAndReconcile([]);

            expect(stats.added).toBe(0);
            expect(stats.updated).toBe(0);
            expect(stats.deleted).toBe(0);
        });
    });

    // ============================================================
    // sweepAndReconcile() — independent classification
    // ============================================================
    describe("sweepAndReconcile() — independent", () => {
        it("should add when classified as independent", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([
                { text: "Existing unrelated fact", traceKeywords: "[]" },
            ]);
            mockAiClient.chat.completions.create.mockResolvedValue({
                choices: [{ message: { content: "independent" } }],
            });

            const stats = await engine.sweepAndReconcile([
                { text: "New unrelated fact", domain: "D", category: "C", trace_identifiers: [] },
            ]);

            expect(stats.added).toBe(1);
            expect(stats.updated).toBe(0);
        });
    });

    // ============================================================
    // sweepAndReconcile() — contradictory classification
    // ============================================================
    describe("sweepAndReconcile() — contradictory", () => {
        it("should delete old and add new when contradictory", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([
                { text: "User lives in Hanoi", traceKeywords: '["ev_old"]' },
            ]);
            mockAiClient.chat.completions.create.mockResolvedValue({
                choices: [{ message: { content: "contradictory" } }],
            });

            const stats = await engine.sweepAndReconcile([
                { text: "User lives in HCMC", domain: "Personal", category: "Location", trace_identifiers: ["ev_new"] },
            ]);

            expect(stats.deleted).toBe(1);
            expect(stats.added).toBe(1);
            expect(mockStructuredMemory.deleteVectorByContent).toHaveBeenCalledWith("User lives in Hanoi");
        });
    });

    // ============================================================
    // sweepAndReconcile() — extendable classification
    // ============================================================
    describe("sweepAndReconcile() — extendable", () => {
        it("should synthesize and update when extendable", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([
                { text: "User likes coffee", traceKeywords: '["ev_old"]' },
            ]);

            // First call: classify as extendable
            // Second call: synthesize
            mockAiClient.chat.completions.create
                .mockResolvedValueOnce({
                    choices: [{ message: { content: "extendable" } }],
                })
                .mockResolvedValueOnce({
                    choices: [{ message: { content: '{"synthesized_text": "User likes coffee, especially espresso"}' } }],
                });

            const stats = await engine.sweepAndReconcile([
                { text: "User especially likes espresso", domain: "Personal", category: "Preference", trace_identifiers: ["ev_new"] },
            ]);

            expect(stats.updated).toBe(1);
            expect(mockStructuredMemory.deleteVectorByContent).toHaveBeenCalledWith("User likes coffee");
        });

        it("should skip update when synthesized text is identical to existing", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([
                { text: "User likes coffee", traceKeywords: "[]" },
            ]);

            mockAiClient.chat.completions.create
                .mockResolvedValueOnce({
                    choices: [{ message: { content: "extendable" } }],
                })
                .mockResolvedValueOnce({
                    choices: [{ message: { content: '{"synthesized_text": "User likes coffee"}' } }],
                });

            const stats = await engine.sweepAndReconcile([
                { text: "Same info", domain: "D", category: "C", trace_identifiers: [] },
            ]);

            expect(stats.updated).toBe(0);
            expect(mockStructuredMemory.deleteVectorByContent).not.toHaveBeenCalled();
        });
    });

    // ============================================================
    // Error handling
    // ============================================================
    describe("Error handling", () => {
        it("should continue processing on single axiom failure", async () => {
            // First axiom will fail
            mockEmbeddingService.embed
                .mockRejectedValueOnce(new Error("Embedding failed"))
                .mockResolvedValueOnce([0.1, 0.2, 0.3]);

            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([]);

            const stats = await engine.sweepAndReconcile([
                { text: "Will fail", domain: "D", category: "C", trace_identifiers: [] },
                { text: "Will succeed", domain: "D", category: "C", trace_identifiers: [] },
            ]);

            // Second axiom should still be processed
            expect(stats.added).toBe(1);
        });

        it("should default to independent on LLM classification failure", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([
                { text: "Existing", traceKeywords: "[]" },
            ]);
            mockAiClient.chat.completions.create.mockRejectedValue(new Error("API timeout"));

            const stats = await engine.sweepAndReconcile([
                { text: "New fact", domain: "D", category: "C", trace_identifiers: [] },
            ]);

            // Fail-safe: treated as independent → added
            expect(stats.added).toBe(1);
        });
    });

    // ============================================================
    // Batch limiting
    // ============================================================
    describe("Batch limiting", () => {
        it("should process max 50 axioms per sweep", async () => {
            mockStructuredMemory.searchAxiomsByVector.mockReturnValue([]);

            const manyAxioms = Array.from({ length: 100 }, (_, i) => ({
                text: `Fact ${i}`,
                domain: "D",
                category: "C",
                trace_identifiers: [],
            }));

            const stats = await engine.sweepAndReconcile(manyAxioms);

            // Should process at most 50 (hardware check returns false = not throttled)
            expect(stats.added).toBeLessThanOrEqual(50);
        });
    });
});
