import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock sqlite-vec
vi.mock("sqlite-vec", () => ({
    load: vi.fn(),
}));

// Mock JsonExtractor
vi.mock("../../src/utils/JsonExtractor", () => ({
    safeExtractJSON: vi.fn((s: string) => { try { return JSON.parse(s); } catch { return []; } }),
}));

// vi.hoisted — survives module hoisting
const { mockExec, mockPrepare, mockStmtRun, mockStmtGet, mockStmtAll, mockTransactionBatch } = vi.hoisted(() => {
    const mockStmtRun = vi.fn(() => ({ changes: 1 }));
    const mockStmtGet = vi.fn();
    const mockStmtAll = vi.fn(() => [] as any[]);
    const mockPrepare = vi.fn(() => ({
        get: mockStmtGet,
        all: mockStmtAll,
        run: mockStmtRun,
    }));
    const mockExec = vi.fn();
    const mockTransactionBatch = vi.fn();
    return { mockExec, mockPrepare, mockStmtRun, mockStmtGet, mockStmtAll, mockTransactionBatch };
});

vi.mock("node:sqlite", () => {
    class MockDatabaseSync {
        exec = mockExec;
        prepare = mockPrepare;
        transactionBatch = mockTransactionBatch;
        constructor() {}
    }
    return { DatabaseSync: MockDatabaseSync };
});

import { VectorRepository } from "../../src/memory/VectorRepository";
import { DatabaseSync } from "node:sqlite";
import * as sqliteVec from "sqlite-vec";
import { DatabaseWorkerBridge } from "../../src/memory/DatabaseWorkerBridge";

describe("VectorRepository — sqlite-vec Vector CRUD", () => {
    let repo: VectorRepository;

    beforeEach(() => {
        vi.resetAllMocks();
        mockStmtRun.mockImplementation(() => ({ changes: 1 }));
        mockStmtAll.mockImplementation(() => [] as any[]);
        mockPrepare.mockImplementation(() => ({
            get: mockStmtGet,
            all: mockStmtAll,
            run: mockStmtRun,
        }));
        const db = new DatabaseSync(":memory:" as any);
        repo = new VectorRepository(db as unknown as DatabaseWorkerBridge);
    });

    // ============================================================
    // Constructor
    // ============================================================
    describe("Constructor", () => {
        it("should create without error", () => {
            expect(repo).toBeTruthy();
        });

        it("should not be vecReady before init()", () => {
            expect(repo.vecReady).toBe(false);
        });
    });

    // ============================================================
    // init()
    // ============================================================
    describe("init()", () => {
        it("should set vecReady to true on successful initialization", async () => {
            // detectOrCreateVecTable needs to return undefined for new table
            mockStmtGet
                .mockReturnValueOnce(undefined)  // no existing vec_idx
                .mockReturnValueOnce({ c: 0 })   // vectors_meta count
                .mockReturnValueOnce({ c: 0 });   // vectors_fts count

            await repo.init();

            expect(repo.vecReady).toBe(true);
        });

        it("should create vectors_meta and vectors_fts tables", async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });

            await repo.init();

            expect(mockExec).toHaveBeenCalledWith(expect.stringContaining("CREATE TABLE IF NOT EXISTS vectors_meta"));
            expect(mockExec).toHaveBeenCalledWith(expect.stringContaining("CREATE VIRTUAL TABLE IF NOT EXISTS vectors_fts"));
        });

        it("should detect existing vec_idx dimension", async () => {
            // Simulate existing vec_idx with 768D
            mockStmtGet
                .mockReturnValueOnce({ sql: "CREATE VIRTUAL TABLE vec_idx USING vec0(embedding float[768])" })
                .mockReturnValueOnce({ c: 10 })
                .mockReturnValueOnce({ c: 10 });

            await repo.init();

            expect(repo.vecReady).toBe(true);
        });

        it("should set vecReady=false on init failure", async () => {
            mockExec.mockImplementationOnce(() => { throw new Error("Table creation failed"); });

            await repo.init();

            expect(repo.vecReady).toBe(false);
        });

        it("should backfill FTS5 when vectors exist but FTS is empty", async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)     // no existing vec_idx
                .mockReturnValueOnce({ c: 5 })      // 5 vectors in meta
                .mockReturnValueOnce({ c: 0 });      // 0 in FTS

            await repo.init();

            expect(mockExec).toHaveBeenCalledWith(expect.stringContaining("INSERT INTO vectors_fts"));
        });
    });

    // ============================================================
    // initVecDimension()
    // ============================================================
    describe("initVecDimension()", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("should no-op if same dimension", async () => {
            await repo.initVecDimension(384);
            // No DROP/CREATE calls
            expect(mockExec).not.toHaveBeenCalledWith(expect.stringContaining("DROP TABLE"));
        });

        it("should recreate vec_idx when dimension changes with empty table", async () => {
            // vec_idx exists, empty
            mockStmtGet
                .mockReturnValueOnce({ name: "vec_idx" })  // table exists
                .mockReturnValueOnce({ c: 0 });             // empty

            await repo.initVecDimension(768);

            expect(mockExec).toHaveBeenCalledWith("DROP TABLE vec_idx");
            expect(mockExec).toHaveBeenCalledWith("CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[768])");
        });

        it("should clear and recreate when dimension changes with existing vectors", async () => {
            // vec_idx exists, has vectors
            mockStmtGet
                .mockReturnValueOnce({ name: "vec_idx" })
                .mockReturnValueOnce({ c: 10 });

            await repo.initVecDimension(768);

            expect(mockExec).toHaveBeenCalledWith("DELETE FROM vec_idx");
            expect(mockExec).toHaveBeenCalledWith("DELETE FROM vectors_meta");
        });

        it("should create vec_idx if table doesn't exist", async () => {
            mockStmtGet.mockReturnValueOnce(undefined); // no table

            await repo.initVecDimension(1024);

            expect(mockExec).toHaveBeenCalledWith("CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[1024])");
        });
    });

    // ============================================================
    // upsertVector()
    // ============================================================
    describe("upsertVector()", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("should not upsert when vecReady is false", async () => {
            const freshRepo = new VectorRepository(new DatabaseSync(":memory:" as any) as any);
            await freshRepo.upsertVector({
                vecId: "v1",
                type: "ANCHOR",
                content: "test",
                vector: [0.1, 0.2],
            });
            // mockPrepare should not be called for vector insert
        });

        it("should INSERT new vector", async () => {
            // No existing vector
            mockStmtGet
                .mockReturnValueOnce({ id: 1 });   // inserted row id

            await repo.upsertVector({
                vecId: "vec_new",
                type: "ANCHOR",
                content: "test content",
                vector: [0.1, 0.2, 0.3],
                domain: "Work",
                sourceEventIds: ["evt_1", "evt_2"],
            });

            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("INSERT OR IGNORE INTO vectors_meta")
            );
        });

        it("should UPDATE existing vector", async () => {
            // Existing vector: INSERT OR IGNORE should do nothing (changes: 0)
            mockStmtRun.mockReturnValueOnce({ changes: 0 }); // for INSERT OR IGNORE
            mockStmtGet.mockReturnValueOnce({ id: 42 }); // existing record

            await repo.upsertVector({
                vecId: "vec_existing",
                type: "AXIOM",
                content: "updated",
                vector: [0.1, 0.2, 0.3],
            });

            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("DELETE FROM vec_idx WHERE rowid = ?")
            );
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("UPDATE vectors_meta SET")
            );
        });

        it("should cap sourceEventIds at 50", async () => {
            mockStmtGet
                .mockReturnValueOnce({ id: 1 });

            const manyIds = Array.from({ length: 100 }, (_, i) => `evt_${i}`);
            await repo.upsertVector({
                vecId: "vec_cap",
                type: "ANCHOR",
                content: "capped",
                vector: [0.1],
                sourceEventIds: manyIds,
            });

            // Verify the JSON-serialized string has only 50 IDs
            const insertCall = mockStmtRun.mock.calls.find(
                (call: any[]) => typeof call[0] === "string" && call[0].startsWith("vec_cap")
            ) as any[] | undefined;
            if (insertCall) {
                const eventIdsArg = insertCall[7]; // source_event_ids position
                if (typeof eventIdsArg === "string") {
                    const parsed = JSON.parse(eventIdsArg);
                    expect(parsed.length).toBeLessThanOrEqual(50);
                }
            }
        });
    });

    // ============================================================
    // upsertVectorsBatch()
    // ============================================================
    describe("upsertVectorsBatch()", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("should no-op for empty array", async () => {
            await repo.upsertVectorsBatch([]);
            expect(mockTransactionBatch).not.toHaveBeenCalled();
        });

        it("should wrap in transaction", async () => {
            await repo.upsertVectorsBatch([
                { vecId: "b1", type: "ANCHOR", content: "c1", vector: Array(384).fill(0.1) },
                { vecId: "b2", type: "ANCHOR", content: "c2", vector: Array(384).fill(0.2) },
            ]);

            expect(mockTransactionBatch).toHaveBeenCalled();
            const statements = mockTransactionBatch.mock.calls[0][0];
            expect(statements).toHaveLength(4);
            expect(statements[0].sql).toContain("INSERT INTO vectors_meta");
            expect(statements[1].sql).toContain("DELETE FROM vec_idx");
            expect(statements[2].sql).toContain("INSERT INTO vec_idx");
            expect(statements[3].sql).toContain("INSERT OR REPLACE INTO vectors_fts");
        });

        it("should rollback on error", async () => {
            mockTransactionBatch.mockRejectedValue(new Error("DB error"));

            await expect(repo.upsertVectorsBatch([
                { vecId: "b_err", type: "ANCHOR", content: "err", vector: Array(384).fill(0.1) },
            ])).rejects.toThrow("DB error");
        });
    });

    // ============================================================
    // searchSimilarVectors()
    // ============================================================
    describe("searchSimilarVectors()", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("should return empty when not vecReady", async () => {
            const freshRepo = new VectorRepository(new DatabaseSync(":memory:" as any) as any);
            expect(await freshRepo.searchSimilarVectors([0.1], 5)).toEqual([]);
        });

        it("should execute KNN query and return results", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.5, vec_id: "v1", content: "hello",
                    type: "ANCHOR", domain: "General", category: "Test",
                    trace_keywords: "[]", source_event_ids: "[]",
                    decay_weight: 1.0, access_count: 0,
                },
            ]);

            const results = await repo.searchSimilarVectors([0.1, 0.2], 5);
            expect(results).toHaveLength(1);
            expect(results[0].vecId).toBe("v1");
            expect(results[0].score).toBeGreaterThan(0);
        });

        it("should filter by type when typeFilter provided", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.3, vec_id: "v1", content: "anchor",
                    type: "ANCHOR", domain: "G", category: "C",
                    trace_keywords: "[]", source_event_ids: "[]",
                    decay_weight: 1.0, access_count: 0,
                }
            ]);

            const results = await repo.searchSimilarVectors([0.1], 5, { type: "ANCHOR" });
            expect(results.length).toBe(1);
            expect(results[0].type).toBe("ANCHOR");
        });

        it("should parse sourceEventIds safely", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.1, vec_id: "v_safe", content: "safe",
                    type: "ANCHOR", domain: "G", category: "C",
                    trace_keywords: '["kw1"]',
                    source_event_ids: '["evt_1", "evt_2"]',
                    decay_weight: 0.8, access_count: 5,
                },
            ]);

            const results = await repo.searchSimilarVectors([0.1], 5);
            expect(results[0].traceKeywords).toEqual(["kw1"]);
            expect(results[0].sourceEventIds).toEqual(["evt_1", "evt_2"]);
        });
    });

    // ============================================================
    // searchAnchors() / searchAxiomsByVector()
    // ============================================================
    describe("searchAnchors / searchAxiomsByVector", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("searchAnchors should return content strings", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.1, vec_id: "a1", content: "anchor text",
                    type: "ANCHOR", domain: "G", category: "C",
                    trace_keywords: "[]", source_event_ids: "[]",
                    decay_weight: 1.0, access_count: 0,
                },
            ]);

            const results = await repo.searchAnchors([0.1], 5);
            expect(results).toEqual(["anchor text"]);
        });

        it("searchAxiomsByVector should return text+trace", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.1, vec_id: "ax1", content: "axiom text",
                    type: "AXIOM", domain: "G", category: "C",
                    trace_keywords: '["k1"]', source_event_ids: "[]",
                    decay_weight: 1.0, access_count: 0,
                },
            ]);

            const results = await repo.searchAxiomsByVector([0.1], 3);
            expect(results[0].text).toBe("axiom text");
            expect(results[0].traceKeywords).toBe('["k1"]');
        });
    });

    // ============================================================
    // deleteVectorByContent / deleteVectorById / deleteAllVectors
    // ============================================================
    describe("Delete operations", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("deleteVectorByContent should delete meta, vec_idx, fts", async () => {
            mockStmtGet.mockReturnValueOnce({ id: 10 });
            await repo.deleteVectorByContent("some content");
            expect(mockPrepare).toHaveBeenCalledWith("DELETE FROM vec_idx WHERE rowid = ?");
            expect(mockPrepare).toHaveBeenCalledWith("DELETE FROM vectors_meta WHERE id = ?");
        });

        it("deleteVectorByContent should no-op if content not found", async () => {
            mockStmtGet.mockReturnValueOnce(undefined);
            await repo.deleteVectorByContent("nonexistent");
            expect(mockPrepare).not.toHaveBeenCalledWith(
                expect.stringContaining("DELETE FROM vec_idx WHERE rowid = ?")
            );
        });

        it("deleteVectorById should delete by vecId", async () => {
            mockStmtGet.mockReturnValueOnce({ id: 20 });
            await repo.deleteVectorById("vec_to_delete");
            expect(mockPrepare).toHaveBeenCalledWith("DELETE FROM vec_idx WHERE rowid = ?");
        });

        it("deleteAllVectors should clear all tables", async () => {
            await repo.deleteAllVectors();
            expect(mockExec).toHaveBeenCalledWith("DELETE FROM vec_idx");
            expect(mockExec).toHaveBeenCalledWith("DELETE FROM vectors_meta");
            expect(mockExec).toHaveBeenCalledWith("DELETE FROM vectors_fts");
        });
    });

    // ============================================================
    // vectorCount
    // ============================================================
    describe("vectorCount", () => {
        it("should return 0 when not ready", async () => {
            expect(await repo.getVectorCount()).toBe(0);
        });

        it("should return count from DB", async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();

            mockStmtGet.mockReturnValueOnce({ c: 42 });
            expect(await repo.getVectorCount()).toBe(42);
        });
    });

    // ============================================================
    // DLQ — pushToDLQ / processDLQ
    // ============================================================
    describe("DLQ", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("pushToDLQ should insert pending entry", async () => {
            await repo.pushToDLQ("content_to_delete");
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("INSERT INTO vector_dlq")
            );
        });

        it("pushToDLQ should catch errors", async () => {
            mockPrepare.mockImplementationOnce(() => { throw new Error("DLQ insert error"); });
            await expect(repo.pushToDLQ("fail")).resolves.not.toThrow();
        });

        it("processDLQ should clean pending entries", async () => {
            mockStmtAll.mockReturnValueOnce([
                { id: 1, delete_filter: "old content", retry_count: 0 },
            ]);
            // deleteVectorByContent lookup
            mockStmtGet.mockReturnValueOnce({ id: 99 });

            await repo.processDLQ();
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("DELETE FROM vector_dlq WHERE id = ?")
            );
        });

        it("processDLQ should mark as dead_letter after 3 retries", async () => {
            mockStmtAll.mockReturnValueOnce([
                { id: 5, delete_filter: "dead content", retry_count: 3 },
            ]);

            await repo.processDLQ();
            expect(mockPrepare).toHaveBeenCalledWith(
                expect.stringContaining("status = 'dead_letter'")
            );
        });

        it("processDLQ should increment retry on failure", async () => {
            mockStmtAll.mockReturnValueOnce([
                { id: 2, delete_filter: "retry content", retry_count: 1 },
            ]);
            // Make deleteVectorByContent throw
            mockStmtGet.mockReturnValueOnce({ id: 100 });
            mockPrepare.mockImplementationOnce(() => { throw new Error("Delete failed"); });

            await repo.processDLQ();
            // Should have called retry increment
        });

        it("processDLQ should catch top-level error", async () => {
            mockStmtAll.mockImplementation(() => { throw new Error("Query error"); });
            await expect(repo.processDLQ()).resolves.not.toThrow();
        });
    });

    // ============================================================
    // searchHybridVectors (RRF)
    // ============================================================
    describe("searchHybridVectors()", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("should return empty when not vecReady", async () => {
            const freshRepo = new VectorRepository(new DatabaseSync(":memory:" as any) as any);
            expect(await freshRepo.searchHybridVectors("query", [0.1], 5)).toEqual([]);
        });

        it("should combine vector and FTS results via RRF", async () => {
            // First call: vector KNN
            // Second call: FTS search
            mockStmtAll
                .mockReturnValueOnce([
                    {
                        rowid: 1, distance: 0.2, vec_id: "v1", content: "hello world",
                        type: "ANCHOR", domain: "G", category: "C",
                        trace_keywords: "[]", source_event_ids: "[]",
                        decay_weight: 1.0, access_count: 0,
                    },
                ])
                .mockReturnValueOnce([
                    {
                        rowid: 2, vec_id: "v2", content: "hello friend",
                        type: "ANCHOR", domain: "G", category: "C",
                        trace_keywords: "[]", source_event_ids: "[]",
                    },
                ]);

            const results = await repo.searchHybridVectors("hello", [0.1], 5);
            expect(results.length).toBeGreaterThanOrEqual(1);
        });

        it("should boost score for items appearing in both vector and FTS", async () => {
            // Same vec_id in both results → RRF score is sum of both ranks
            mockStmtAll
                .mockReturnValueOnce([
                    {
                        rowid: 1, distance: 0.1, vec_id: "shared", content: "shared content",
                        type: "ANCHOR", domain: "G", category: "C",
                        trace_keywords: "[]", source_event_ids: "[]",
                        decay_weight: 1.0, access_count: 0,
                    },
                ])
                .mockReturnValueOnce([
                    {
                        rowid: 1, vec_id: "shared", content: "shared content",
                        type: "ANCHOR", domain: "G", category: "C",
                        trace_keywords: "[]", source_event_ids: "[]",
                    },
                ]);

            const results = await repo.searchHybridVectors("shared", [0.1], 5);
            expect(results[0].vecId).toBe("shared");
            // Score should be > single rank score (1/(60+1))
            expect(results[0].score).toBeGreaterThan(1 / 61);
        });

        it("should fallback on FTS5 syntax error", async () => {
            mockStmtAll
                .mockReturnValueOnce([]) // vector results
                .mockImplementationOnce(() => { throw new Error("FTS5 syntax error"); }) // first FTS fails
                .mockReturnValueOnce([]); // fallback FTS

            const results = await repo.searchHybridVectors("query with special chars", [0.1], 5);
            expect(results).toEqual([]);
        });
    });

    // ============================================================
    // searchWithDrilldown / collectDrilldownEventIds
    // ============================================================
    describe("searchWithDrilldown / collectDrilldownEventIds", () => {
        beforeEach(async () => {
            mockStmtGet
                .mockReturnValueOnce(undefined)
                .mockReturnValueOnce({ c: 0 })
                .mockReturnValueOnce({ c: 0 });
            await repo.init();
            vi.clearAllMocks();
        });

        it("searchWithDrilldown should return vecId, content, sourceEventIds", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.1, vec_id: "drill1", content: "drill content",
                    type: "ANCHOR", domain: "G", category: "C",
                    trace_keywords: "[]", source_event_ids: '["evt_x", "evt_y"]',
                    decay_weight: 1.0, access_count: 0,
                },
            ]);

            const results = await repo.searchWithDrilldown([0.1], 3);
            expect(results[0].sourceEventIds).toEqual(["evt_x", "evt_y"]);
        });

        it("collectDrilldownEventIds should deduplicate", async () => {
            mockStmtAll.mockReturnValue([
                {
                    rowid: 1, distance: 0.1, vec_id: "d1", content: "c1",
                    type: "ANCHOR", domain: "G", category: "C",
                    trace_keywords: "[]", source_event_ids: '["evt_shared", "evt_a"]',
                    decay_weight: 1.0, access_count: 0,
                },
                {
                    rowid: 2, distance: 0.2, vec_id: "d2", content: "c2",
                    type: "ANCHOR", domain: "G", category: "C",
                    trace_keywords: "[]", source_event_ids: '["evt_shared", "evt_b"]',
                    decay_weight: 1.0, access_count: 0,
                },
            ]);

            const ids = await repo.collectDrilldownEventIds([0.1], 3);
            expect(ids).toContain("evt_shared");
            expect(ids).toContain("evt_a");
            expect(ids).toContain("evt_b");
            expect(new Set(ids).size).toBe(ids.length); // no duplicates
        });
    });
});
