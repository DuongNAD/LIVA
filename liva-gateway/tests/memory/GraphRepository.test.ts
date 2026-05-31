import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/utils/ULID", () => ({
    generateULID: vi.fn().mockReturnValue("01H_MOCK_ULID"),
}));

import { GraphRepository, type L3Node, type L3Edge } from "../../src/memory/GraphRepository";
import { logger } from "../../src/utils/logger";

// ─── Mock DatabaseWorkerBridge ───
function createMockDb() {
    const rows: any[] = [];
    return {
        exec: vi.fn().mockResolvedValue(undefined),
        prepare: vi.fn().mockReturnValue({
            run: vi.fn().mockResolvedValue({ changes: 1, lastInsertRowid: null }),
            all: vi.fn().mockResolvedValue(rows),
            get: vi.fn().mockResolvedValue(null),
        }),
        _setRows(data: any[]) {
            rows.length = 0;
            rows.push(...data);
        },
    };
}

describe("GraphRepository", () => {
    let db: ReturnType<typeof createMockDb>;
    let graph: GraphRepository;

    beforeEach(() => {
        vi.clearAllMocks();
        db = createMockDb();
        graph = new GraphRepository(db as any);
    });

    // ─── Initialization ───
    describe("init", () => {
        it("should create l3_nodes and l3_edges tables", async () => {
            await graph.init();

            expect(db.exec).toHaveBeenCalledTimes(3); // 2 CREATE TABLE + 1 ALTER TABLE
            const calls = db.exec.mock.calls.map((c: any[]) => c[0]);
            expect(calls[0]).toContain("CREATE TABLE IF NOT EXISTS l3_nodes");
            expect(calls[1]).toContain("CREATE TABLE IF NOT EXISTS l3_edges");
        });

        it("should log success on init", async () => {
            await graph.init();
            expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Graph tables initialized"));
        });

        it("should handle init failure gracefully", async () => {
            db.exec.mockRejectedValueOnce(new Error("disk full"));
            await graph.init();
            expect(logger.error).toHaveBeenCalledWith(expect.stringContaining("Failed to initialize Graph tables"));
        });
    });

    // ─── Node CRUD ───
    describe("upsertNode", () => {
        it("should upsert a node with INSERT ON CONFLICT", async () => {
            const node: L3Node = { id: "user_duong", label: "PERSON", properties: '{"name":"Duong"}' };
            await graph.upsertNode(node);

            expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("INSERT INTO l3_nodes"));
            const stmt = db.prepare.mock.results[0].value;
            expect(stmt.run).toHaveBeenCalledWith("user_duong", "PERSON", '{"name":"Duong"}');
        });

        it("should log error on upsertNode failure without throwing", async () => {
            db.prepare.mockReturnValueOnce({
                run: vi.fn().mockRejectedValue(new Error("constraint")),
            });

            await graph.upsertNode({ id: "x", label: "Y", properties: "{}" });
            expect(logger.error).toHaveBeenCalledWith(expect.stringContaining("Error upserting node x"));
        });
    });

    // ─── Edge CRUD ───
    describe("upsertEdge", () => {
        it("should upsert an edge with INSERT ON CONFLICT", async () => {
            const edge: L3Edge = { source: "A", target: "B", relation: "KNOWS", weight: 0.8, obsolete: 0 };
            await graph.upsertEdge(edge);

            expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("INSERT INTO l3_edges"));
            const stmt = db.prepare.mock.results[0].value;
            expect(stmt.run).toHaveBeenCalledWith("A", "B", "KNOWS", 0.8, 0);
        });
    });

    describe("markEdgeObsolete", () => {
        it("should set obsolete=1 for matching edge", async () => {
            await graph.markEdgeObsolete("A", "B", "KNOWS");

            expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("UPDATE l3_edges"));
            expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("obsolete = 1"));
        });
    });

    // ─── Query Methods ───
    describe("getActiveEdgesBySource", () => {
        it("should query edges filtered by source and obsolete=0", async () => {
            const mockEdges: L3Edge[] = [
                { source: "A", target: "B", relation: "KNOWS", weight: 1.0, obsolete: 0 },
            ];
            db.prepare.mockReturnValueOnce({
                all: vi.fn().mockResolvedValue(mockEdges),
            });

            const edges = await graph.getActiveEdgesBySource("A");
            expect(edges).toHaveLength(1);
            expect(edges[0].target).toBe("B");
        });

        it("should return empty array on error", async () => {
            db.prepare.mockReturnValueOnce({
                all: vi.fn().mockRejectedValue(new Error("db locked")),
            });

            const edges = await graph.getActiveEdgesBySource("X");
            expect(edges).toEqual([]);
        });
    });

    describe("getAllActiveNodes", () => {
        it("should return all nodes from l3_nodes", async () => {
            const mockNodes: L3Node[] = [
                { id: "A", label: "PERSON", properties: "{}" },
                { id: "B", label: "PROJECT", properties: "{}" },
            ];
            db.prepare.mockReturnValueOnce({
                all: vi.fn().mockResolvedValue(mockNodes),
            });

            const nodes = await graph.getAllActiveNodes();
            expect(nodes).toHaveLength(2);
        });
    });

    // ─── Multi-hop Search ───
    describe("multiHopSearch", () => {
        it("should use recursive CTE with correct parameters", async () => {
            const mockResults = [
                { source: "A", target: "B", relation: "KNOWS", depth: 1 },
                { source: "B", target: "C", relation: "WORKS_WITH", depth: 2 },
            ];
            db.prepare.mockReturnValueOnce({
                all: vi.fn().mockResolvedValue(mockResults),
            });

            const results = await graph.multiHopSearch("A", 3);

            expect(results).toHaveLength(2);
            expect(db.prepare).toHaveBeenCalledWith(expect.stringContaining("WITH RECURSIVE traverse"));
        });

        it("should default maxDepth to 3", async () => {
            db.prepare.mockReturnValueOnce({
                all: vi.fn().mockResolvedValue([]),
            });

            await graph.multiHopSearch("A");

            const stmt = db.prepare.mock.results[0].value;
            expect(stmt.all).toHaveBeenCalledWith("A", 3);
        });

        it("should return empty array on error", async () => {
            db.prepare.mockReturnValueOnce({
                all: vi.fn().mockRejectedValue(new Error("timeout")),
            });

            const results = await graph.multiHopSearch("X");
            expect(results).toEqual([]);
        });
    });

    // ─── Community Summaries ───
    describe("buildCommunitySummaries", () => {
        it("should skip if no nodes exist", async () => {
            // Mock getAllActiveNodes → empty
            db.prepare.mockReturnValueOnce({ all: vi.fn().mockResolvedValue([]) }); // nodes
            db.prepare.mockReturnValueOnce({ all: vi.fn().mockResolvedValue([]) }); // edges

            const mockAiClient = { chat: { completions: { create: vi.fn() } } };
            const mockEmbedding = { embed: vi.fn() };

            await graph.buildCommunitySummaries(mockAiClient as any, mockEmbedding as any, vi.fn());

            expect(mockAiClient.chat.completions.create).not.toHaveBeenCalled();
            expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("No nodes found"));
        });

        it("should detect communities and call LLM for summaries", async () => {
            const nodes: L3Node[] = [
                { id: "A", label: "PERSON", properties: "{}" },
                { id: "B", label: "PROJECT", properties: "{}" },
            ];
            const edges: L3Edge[] = [
                { source: "A", target: "B", relation: "WORKS_ON", weight: 1.0, obsolete: 0 },
            ];

            // Mock prepare calls for getAllActiveNodes and getAllActiveEdges
            db.prepare
                .mockReturnValueOnce({ all: vi.fn().mockResolvedValue(nodes) })   // getAllActiveNodes
                .mockReturnValueOnce({ all: vi.fn().mockResolvedValue(edges) });  // getAllActiveEdges

            const mockAiClient = {
                chat: { completions: { create: vi.fn().mockResolvedValue({
                    choices: [{ message: { content: "Community summary about A and B" } }],
                }) } },
            };
            const mockEmbedding = { embed: vi.fn().mockResolvedValue(new Float32Array(128)) };
            const upsertVector = vi.fn();

            await graph.buildCommunitySummaries(mockAiClient as any, mockEmbedding as any, upsertVector);

            // Should have called LLM at least once for the valid community
            expect(mockAiClient.chat.completions.create).toHaveBeenCalled();
            expect(upsertVector).toHaveBeenCalledWith(expect.objectContaining({
                type: "ANCHOR",
                domain: "Community",
                category: "CommunitySummary",
            }));
        });
    });
});
