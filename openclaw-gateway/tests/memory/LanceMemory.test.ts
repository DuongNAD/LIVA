/**
 * LanceMemory.test.ts — Vector memory with LanceDB tests
 * Mocks lancedb to avoid real database operations
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock EmbeddingService
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: () => ({
            ensureReady: vi.fn().mockResolvedValue(undefined),
            embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1)),
            ready: true,
        }),
    },
}));

// Mock lancedb — must define mocks inline to avoid hoisting issues
vi.mock("@lancedb/lancedb", () => {
    const mockTable = {
        add: vi.fn().mockResolvedValue(undefined),
        vectorSearch: vi.fn().mockReturnValue({
            limit: vi.fn().mockReturnValue({
                toArray: vi.fn().mockResolvedValue([
                    { type: "SUCCESS", fileTarget: "src/test.ts", text: "Fixed bug in parser" },
                ]),
            }),
        }),
        query: vi.fn().mockReturnValue({
            where: vi.fn().mockReturnValue({
                toArray: vi.fn().mockResolvedValue([
                    { type: "DEAD-END", fileTarget: "src/old.ts", text: "Failed approach" },
                ]),
            }),
        }),
        delete: vi.fn().mockResolvedValue(undefined),
        createIndex: vi.fn().mockResolvedValue(undefined),
    };

    return {
        connect: vi.fn().mockResolvedValue({
            openTable: vi.fn().mockResolvedValue(mockTable),
            createTable: vi.fn().mockResolvedValue(mockTable),
        }),
        Index: {
            fts: vi.fn().mockReturnValue("fts_config")
        },
        rerankers: {
            RRFReranker: {
                create: vi.fn().mockResolvedValue({})
            }
        },
        __mockTable: mockTable,
    };
});

import { LanceMemoryManager } from "../../src/memory/LanceMemory";
import * as lancedb from "@lancedb/lancedb";

describe("LanceMemoryManager", () => {
    let lance: LanceMemoryManager;

    beforeEach(() => {
        vi.clearAllMocks();
        lance = new LanceMemoryManager();
    });

    describe("connect", () => {
        it("should connect to LanceDB", async () => {
            await expect(lance.connect()).resolves.not.toThrow();
        });

        it("should catch and log error if embeddingService.ensureReady throws", async () => {
            const fresh = new LanceMemoryManager();
            (fresh as any).embeddingService.ensureReady = vi.fn().mockRejectedValue(new Error("Network error"));
            await expect(fresh.connect()).resolves.not.toThrow();
        });
    });

    describe("addMemory", () => {
        it("should add a SUCCESS memory", async () => {
            await lance.connect();
            await lance.addMemory("SUCCESS", "Fixed the parser bug", "src/parser.ts");
            const mockTable = (lancedb as any).__mockTable;
            expect(mockTable.add).toHaveBeenCalled();
        });

        it("should add a DEAD-END memory", async () => {
            await lance.connect();
            await lance.addMemory("DEAD-END", "This approach failed", "src/old.ts");
        });

        it("should add an AXIOM memory", async () => {
            await lance.connect();
            await lance.addMemory("AXIOM", "Always use safeFetch", "src/utils/HttpClient.ts");
        });

        it("should auto-connect if not connected", async () => {
            // First addMemory should trigger connect
            await lance.addMemory("SUCCESS", "test", "test.ts");
            expect(lancedb.connect).toHaveBeenCalled();
        });

        it("should create table if table is null during addMemory", async () => {
            // New instance
            const fresh = new LanceMemoryManager();
            const mockConn = await lancedb.connect("test");
            // openTable fails so table remains null
            (mockConn.openTable as any).mockRejectedValueOnce(new Error("no table"));
            
            await fresh.addMemory("SUCCESS", "test fallback", "test.ts");
            expect(mockConn.createTable).toHaveBeenCalled();
        });

        it("should handle rapid concurrent calls to addMemory safely", async () => {
            const fresh = new LanceMemoryManager();
            const mockConn = await lancedb.connect("test");
            
            // Re-mock openTable to fail 3 times exactly for this test
            (mockConn.openTable as any).mockImplementationOnce(() => Promise.reject(new Error("no table")));
            (mockConn.openTable as any).mockImplementationOnce(() => Promise.reject(new Error("no table")));
            (mockConn.openTable as any).mockImplementationOnce(() => Promise.reject(new Error("no table")));
            
            // Fire multiple adds concurrently
            await Promise.all([
                fresh.addMemory("SUCCESS", "test1", "t1.ts"),
                fresh.addMemory("SUCCESS", "test2", "t2.ts"),
                fresh.addMemory("SUCCESS", "test3", "t3.ts")
            ]);
            
            // Should call createTable initially
            expect(mockConn.createTable).toHaveBeenCalled();
        });
    });

    describe("addSemanticAnchor", () => {
        it("should add an ANCHOR memory properly formatted", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            mockTable.add.mockClear();

            await lance.addSemanticAnchor("summary info", ["turn1", "turn2"], 123456789);
            
            expect(mockTable.add).toHaveBeenCalled();
            const args = mockTable.add.mock.calls[0][0];
            expect(args[0].type).toBe("ANCHOR");
            expect(args[0].text).toBe("summary info");
            expect(args[0].fileTarget).toBe(JSON.stringify(["turn1", "turn2"]));
            expect(args[0].timestamp).toBe(123456789);
        });

        it("should create table if table is null during addSemanticAnchor", async () => {
            const fresh = new LanceMemoryManager();
            const mockConn = await lancedb.connect("test");
            (mockConn.openTable as any).mockRejectedValueOnce(new Error("no table"));
            
            await fresh.addSemanticAnchor("summary info", ["turn1", "turn2"], 123456789);
            expect(mockConn.createTable).toHaveBeenCalled();
        });
    });

    describe("searchMemory", () => {
        it("should return formatted search results", async () => {
            await lance.connect();
            const results = await lance.searchMemory("parser bug", 3);
            expect(results.length).toBeGreaterThan(0);
            expect(results[0]).toContain("[SUCCESS]");
        });

        it("should return empty array when table is null", async () => {
            // New instance without connecting - table is null
            const fresh = new LanceMemoryManager();
            // Override connect to NOT set the table
            const mockConn = await lancedb.connect("test");
            (mockConn.openTable as any).mockRejectedValueOnce(new Error("no table"));
            await fresh.connect();
            const results = await fresh.searchMemory("test");
            expect(results).toEqual([]);
        });

        it("should fallback to empty array when vectorSearch throws exception", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            mockTable.vectorSearch.mockImplementationOnce(() => {
                throw new Error("Vector search error");
            });
            const results = await lance.searchMemory("error test", 3);
            expect(results).toEqual([]);
        });
    });

    describe("searchAnchors", () => {
        it("should return anchor texts correctly", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            
            // Setup mock chain for searchAnchors
            mockTable.vectorSearch.mockReturnValueOnce({
                where: vi.fn().mockReturnValue({
                    limit: vi.fn().mockReturnValue({
                        toArray: vi.fn().mockResolvedValue([
                            { type: "ANCHOR", text: "anchor text 1" }
                        ])
                    })
                })
            });

            const results = await lance.searchAnchors("query", 5);
            expect(results).toEqual(["anchor text 1"]);
        });

        it("should return empty array when table is null", async () => {
            const fresh = new LanceMemoryManager();
            const mockConn = await lancedb.connect("test");
            (mockConn.openTable as any).mockRejectedValueOnce(new Error("no table"));
            await fresh.connect();
            const results = await fresh.searchAnchors("test");
            expect(results).toEqual([]);
        });

        it("should fallback to empty array when vectorSearch throws exception", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            mockTable.vectorSearch.mockImplementationOnce(() => {
                throw new Error("Vector search error");
            });
            const results = await lance.searchAnchors("error test", 3);
            expect(results).toEqual([]);
        });
    });

    describe("getAllEpisodicMemories", () => {
        it("should return all non-AXIOM memories", async () => {
            await lance.connect();
            const results = await lance.getAllEpisodicMemories();
            expect(results.length).toBeGreaterThan(0);
        });

        it("should return empty array and log warning when query throws exception", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            
            // Mock where() to throw
            mockTable.query = vi.fn().mockReturnValue({
                where: vi.fn().mockImplementation(() => {
                    throw new Error("Query failed");
                }),
            });
            
            const results = await lance.getAllEpisodicMemories();
            expect(results).toEqual([]);
        });

        it("should return empty array when table is null", async () => {
            const fresh = new LanceMemoryManager();
            const mockConn = await lancedb.connect("test");
            (mockConn.openTable as any).mockRejectedValueOnce(new Error("no table"));
            const results = await fresh.getAllEpisodicMemories();
            expect(results).toEqual([]);
        });
    });

    describe("clearEpisodicMemories", () => {
        it("should delete non-AXIOM memories", async () => {
            await lance.connect();
            await lance.clearEpisodicMemories();
            const mockTable = (lancedb as any).__mockTable;
            expect(mockTable.delete).toHaveBeenCalledWith("type != 'AXIOM'");
        });

        it("should catch and log error silently when delete throws exception", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            mockTable.delete.mockRejectedValueOnce(new Error("Delete failed"));
            
            await expect(lance.clearEpisodicMemories()).resolves.not.toThrow();
        });

        it("should return early if table is null", async () => {
            const fresh = new LanceMemoryManager();
            const mockConn = await lancedb.connect("test");
            (mockConn.openTable as any).mockRejectedValueOnce(new Error("no table"));
            await expect(fresh.clearEpisodicMemories()).resolves.not.toThrow();
        });
    });

    // ===========================
    // [v4.0] Enterprise Tests
    // ===========================

    describe("[v4.0] dispose", () => {
        it("should nullify db and table references", async () => {
            await lance.connect();
            expect(lance["db"]).not.toBeNull();

            await lance.dispose();
            expect(lance["db"]).toBeNull();
            expect(lance["table"]).toBeNull();
        });

        it("should be safe to call multiple times", async () => {
            await lance.connect();
            await lance.dispose();
            await expect(lance.dispose()).resolves.not.toThrow();
        });

        it("should be safe to call without connecting first", async () => {
            const fresh = new LanceMemoryManager();
            await expect(fresh.dispose()).resolves.not.toThrow();
        });
    });

    describe("[v4.0] deleteVectors (GDPR)", () => {
        it("should delete vectors matching filter expression", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;

            await lance.deleteVectors("type != ''");
            expect(mockTable.delete).toHaveBeenCalledWith("type != ''");
        });

        it("should return early if table is null", async () => {
            const fresh = new LanceMemoryManager();
            await expect(fresh.deleteVectors("type != ''")).resolves.not.toThrow();
        });

        it("should catch and log error when delete fails", async () => {
            await lance.connect();
            const mockTable = (lancedb as any).__mockTable;
            mockTable.delete.mockRejectedValueOnce(new Error("GDPR delete failed"));

            await expect(lance.deleteVectors("type != ''")).resolves.not.toThrow();
        });
    });
});
