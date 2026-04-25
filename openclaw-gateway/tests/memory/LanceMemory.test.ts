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
    };

    return {
        connect: vi.fn().mockResolvedValue({
            openTable: vi.fn().mockResolvedValue(mockTable),
            createTable: vi.fn().mockResolvedValue(mockTable),
        }),
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
    });

    describe("getAllEpisodicMemories", () => {
        it("should return all non-AXIOM memories", async () => {
            await lance.connect();
            const results = await lance.getAllEpisodicMemories();
            expect(results.length).toBeGreaterThan(0);
        });
    });

    describe("clearEpisodicMemories", () => {
        it("should delete non-AXIOM memories", async () => {
            await lance.connect();
            await lance.clearEpisodicMemories();
            const mockTable = (lancedb as any).__mockTable;
            expect(mockTable.delete).toHaveBeenCalledWith("type != 'AXIOM'");
        });
    });
});
