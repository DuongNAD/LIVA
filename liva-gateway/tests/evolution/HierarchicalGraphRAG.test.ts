import { describe, it, expect, vi, beforeEach } from "vitest";
import { HierarchicalGraphRAG } from "../../src/evolution/HierarchicalGraphRAG";
import { promises as fsp } from "node:fs";
import * as path from "path";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn()
    }
}));

vi.mock("node:fs", () => ({
    promises: {
        readFile: vi.fn()
    }
}));

const mockGraphData = {
    type: "repository",
    name: "root",
    children: [
        {
            type: "file",
            name: "Main.ts",
            filePath: "/src/Main.ts",
            children: [
                {
                    type: "class",
                    name: "MainClass",
                    children: [
                        {
                            type: "function",
                            name: "initSystem",
                            codeSnippet: "function initSystem() { start(); }",
                            calls: ["start", "ab"] // 'ab' is length < 3
                        },
                        {
                            type: "function",
                            name: "start",
                            codeSnippet: "function start() { run(); }",
                            calls: ["run"]
                        }
                    ]
                }
            ]
        },
        {
            type: "file",
            name: "Helper.ts",
            filePath: "/src/Helper.ts",
            children: [
                {
                    type: "function",
                    name: "run",
                    codeSnippet: "function run() { console.log('running'); }",
                    calls: []
                }
            ]
        }
    ]
};

describe("HierarchicalGraphRAG", () => {
    let rag: HierarchicalGraphRAG;

    beforeEach(() => {
        vi.clearAllMocks();
        rag = new HierarchicalGraphRAG();
        (fsp.readFile as any).mockResolvedValue(JSON.stringify(mockGraphData));
    });

    it("should throw error if graph file is missing", async () => {
        (fsp.readFile as any).mockRejectedValueOnce(new Error("ENOENT"));
        await expect(rag.system1Search("keyword")).rejects.toThrow("AST Graph chưa được xây dựng");
    });

    it("should return cache if already loaded", async () => {
        await rag.system1Search("Main");
        expect(fsp.readFile).toHaveBeenCalledTimes(1);
        
        // Second call should hit cache
        await rag.system1Search("Helper");
        expect(fsp.readFile).toHaveBeenCalledTimes(1); // Still 1
    });

    describe("System 1", () => {
        it("should find nodes by partial name", async () => {
            const results = await rag.system1Search("main");
            expect(results.length).toBe(2); // Main.ts and MainClass
            expect(results).toContainEqual(expect.objectContaining({ name: "Main.ts" }));
            expect(results).toContainEqual(expect.objectContaining({ name: "MainClass" }));
        });

        it("should return empty if no matches", async () => {
            const results = await rag.system1Search("missing_keyword");
            expect(results.length).toBe(0);
        });
    });

    describe("System 2", () => {
        it("should return empty if root function not found", async () => {
            const results = await rag.system2DeepDive("missingFunc");
            expect(results.length).toBe(0);
        });

        it("should return details for root function without calls if depth=0", async () => {
            const results = await rag.system2DeepDive("initSystem", 0);
            expect(results.length).toBe(1);
            expect(results[0].name).toBe("initSystem");
            expect(results[0].calls).toHaveLength(0);
        });

        it("should traverse calls recursively up to depth", async () => {
            const results = await rag.system2DeepDive("initSystem", 2);
            expect(results.length).toBe(1);
            
            const root = results[0];
            expect(root.name).toBe("initSystem");
            // "ab" should be ignored because length < 3
            expect(root.calls).toHaveLength(1);
            
            const startCall = root.calls[0];
            expect(startCall.name).toBe("start");
            expect(startCall.calls).toHaveLength(1);

            const runCall = startCall.calls[0];
            expect(runCall.name).toBe("run");
            expect(runCall.calls).toHaveLength(0); // depth ends or no calls
        });

        it("should prevent infinite recursion by not calling itself", async () => {
            const cyclicGraphData = {
                type: "repository",
                name: "root",
                children: [
                    {
                        type: "function",
                        name: "recursiveFunc",
                        codeSnippet: "function recursiveFunc() { recursiveFunc(); }",
                        calls: ["recursiveFunc"]
                    }
                ]
            };
            (fsp.readFile as any).mockResolvedValue(JSON.stringify(cyclicGraphData));
            // Invalidate cache by recreating RAG
            rag = new HierarchicalGraphRAG();
            
            const results = await rag.system2DeepDive("recursiveFunc", 2);
            expect(results.length).toBe(1);
            expect(results[0].calls.length).toBe(0); // Should be empty due to recursion prevention
        });
    });
});
