import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("sqlite-vec", () => ({
    load: vi.fn(),
}));

const { mockExec, mockPrepare, mockStmtRun, mockStmtGet, mockStmtAll } = vi.hoisted(() => {
    const mockStmtRun = vi.fn(() => ({ changes: 1 }));
    const mockStmtGet = vi.fn();
    const mockStmtAll = vi.fn(() => [] as any[]);
    const mockPrepare = vi.fn(() => ({
        get: mockStmtGet,
        all: mockStmtAll,
        run: mockStmtRun,
    }));
    const mockExec = vi.fn();
    return { mockExec, mockPrepare, mockStmtRun, mockStmtGet, mockStmtAll };
});

vi.mock("node:sqlite", () => {
    class MockDatabaseSync {
        exec = mockExec;
        prepare = mockPrepare;
        constructor() {}
    }
    return { DatabaseSync: MockDatabaseSync };
});

import { VectorRepository } from "../../src/memory/VectorRepository";
import { DatabaseSync } from "node:sqlite";
import { DatabaseWorkerBridge } from "../../src/memory/DatabaseWorkerBridge";

describe("VectorRepository — Weighted RRF and createdAt", () => {
    let repo: VectorRepository;

    beforeEach(async () => {
        vi.resetAllMocks();
        mockStmtRun.mockImplementation(() => ({ changes: 1 }));
        mockStmtAll.mockImplementation(() => [] as any[]);
        mockPrepare.mockImplementation((sql: string) => {
            if (sql.includes("vectors_fts") && sql.includes("sqlite_master")) {
                return {
                    get: vi.fn().mockReturnValue({ sql: "unicode61" }),
                    all: mockStmtAll,
                    run: mockStmtRun,
                };
            }
            return {
                get: mockStmtGet,
                all: mockStmtAll,
                run: mockStmtRun,
            };
        });
        const db = new DatabaseSync(":memory:" as any);
        repo = new VectorRepository(db as unknown as DatabaseWorkerBridge);
        
        mockStmtGet
            .mockReturnValueOnce(undefined)  // no existing vec_idx
            .mockReturnValueOnce({ c: 0 })   // vectors_meta count
            .mockReturnValueOnce({ c: 0 });   // vectors_fts count
        await repo.init();
        vi.clearAllMocks();
    });

    it("should retrieve FTS results with m.created_at and map them correctly", async () => {
        // KNN results
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.2, vec_id: "v1", content: "hello world",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: 1000000000000
            }
        ]);
        // FTS results
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 2, vec_id: "v2", content: "hello friend",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: 1000000000000
            }
        ]);

        const results = await repo.searchHybridVectors("hello", [0.1], 5);
        expect(results.length).toBe(2);
        expect(results[0].createdAt).toBe(1000000000000);
        expect(results[1].createdAt).toBe(1000000000000);
    });

    it("should apply custom dense and sparse weights to RRF score calculation", async () => {
        // KNN results
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.1, vec_id: "v1", content: "v1 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: 1000000000000
            }
        ]);
        // FTS results
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 2, vec_id: "v2", content: "v2 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: 1000000000000
            }
        ]);

        // Scenario 1: Equal weights (0.5 / 0.5)
        const resultsEqual = await repo.searchHybridVectors("hello", [0.1], 5, undefined, { dense: 0.5, sparse: 0.5 });
        const scoreV1Equal = resultsEqual.find(r => r.vecId === "v1")?.score ?? 0;
        const scoreV2Equal = resultsEqual.find(r => r.vecId === "v2")?.score ?? 0;

        // Reset mocks for Scenario 2
        vi.clearAllMocks();
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.1, vec_id: "v1", content: "v1 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: 1000000000000
            }
        ]);
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 2, vec_id: "v2", content: "v2 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: 1000000000000
            }
        ]);

        // Scenario 2: High Dense weight (0.8 / 0.2)
        const resultsDenseHeavy = await repo.searchHybridVectors("hello", [0.1], 5, undefined, { dense: 0.8, sparse: 0.2 });
        const scoreV1DenseHeavy = resultsDenseHeavy.find(r => r.vecId === "v1")?.score ?? 0;
        const scoreV2DenseHeavy = resultsDenseHeavy.find(r => r.vecId === "v2")?.score ?? 0;

        // V1 (KNN) score should be higher with dense weight 0.8 than with dense weight 0.5
        expect(scoreV1DenseHeavy).toBeGreaterThan(scoreV1Equal);
        // V2 (FTS) score should be lower with sparse weight 0.2 than with sparse weight 0.5
        expect(scoreV2DenseHeavy).toBeLessThan(scoreV2Equal);
    });

    it("should return createdAt: 0 fallback when created_at is missing/null/undefined in raw SQLite rows", async () => {
        // KNN results with null/undefined created_at
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.2, vec_id: "v1", content: "hello world",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: null
            }
        ]);
        // FTS results with undefined/null created_at
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 2, vec_id: "v2", content: "hello friend",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: undefined
            }
        ]);

        const results = await repo.searchHybridVectors("hello", [0.1], 5);
        expect(results.length).toBe(2);
        const v1 = results.find(r => r.vecId === "v1");
        const v2 = results.find(r => r.vecId === "v2");
        expect(v1?.createdAt).toBe(0);
        expect(v2?.createdAt).toBe(0);
    });

    it("should return createdAt fallback of 0 in searchSimilarVectors when created_at is null/undefined", async () => {
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.2, vec_id: "v1", content: "hello world",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: null
            }
        ]);
        const results = await repo.searchSimilarVectors([0.1], 5);
        expect(results.length).toBe(1);
        expect(results[0].createdAt).toBe(0);
    });

    it("should use default weight 1.0 when weights are not specified or partially specified", async () => {
        // KNN results
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.1, vec_id: "v1", content: "v1 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: 1000
            }
        ]);
        // FTS results
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 2, vec_id: "v2", content: "v2 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: 1000
            }
        ]);

        // Scenario 1: weights is undefined -> should default to dense = 1.0, sparse = 1.0
        const resultsDefault = await repo.searchHybridVectors("hello", [0.1], 5);
        const scoreV1Default = resultsDefault.find(r => r.vecId === "v1")?.score ?? 0;
        const scoreV2Default = resultsDefault.find(r => r.vecId === "v2")?.score ?? 0;

        // RRF scores with weights=undefined:
        // Since both v1 and v2 are at rank 1 in their respective lists, their scores should be:
        // dense score = 1.0 * (1 / (60 + 1)) = 1/61
        // sparse score = 1.0 * (1 / (60 + 1)) = 1/61
        expect(scoreV1Default).toBeCloseTo(1 / 61, 8);
        expect(scoreV2Default).toBeCloseTo(1 / 61, 8);

        // Reset mocks for partially specified weights
        vi.clearAllMocks();
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 1, distance: 0.1, vec_id: "v1", content: "v1 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                decay_weight: 1.0, access_count: 0, created_at: 1000
            }
        ]);
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 2, vec_id: "v2", content: "v2 content",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: 1000
            }
        ]);

        // Scenario 2: partially specified weights: dense=0.5, sparse is undefined -> sparse defaults to 1.0
        const resultsPartial = await repo.searchHybridVectors("hello", [0.1], 5, undefined, { dense: 0.5 });
        const scoreV1Partial = resultsPartial.find(r => r.vecId === "v1")?.score ?? 0;
        const scoreV2Partial = resultsPartial.find(r => r.vecId === "v2")?.score ?? 0;

        expect(scoreV1Partial).toBeCloseTo(0.5 * (1 / 61), 8);
        expect(scoreV2Partial).toBeCloseTo(1.0 * (1 / 61), 8);
    });

    it("should return createdAt fallback of 0 in searchHybridVectors fallback FTS query path", async () => {
        // Let KNN return empty list
        mockStmtAll.mockReturnValueOnce([]);
        // Let first FTS fail to trigger fallback FTS query path
        mockStmtAll.mockImplementationOnce(() => { throw new Error("FTS error"); });
        // Let fallback FTS return result with null created_at
        mockStmtAll.mockReturnValueOnce([
            {
                rowid: 3, vec_id: "v3", content: "hello world",
                type: "ANCHOR", domain: "G", category: "C",
                trace_keywords: "[]", source_event_ids: "[]",
                created_at: null
            }
        ]);

        const results = await repo.searchHybridVectors("hello", [0.1], 5);
        expect(results.length).toBe(1);
        expect(results[0].createdAt).toBe(0);
    });
});
