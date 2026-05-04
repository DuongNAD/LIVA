import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { GitNexusIndexer } from "../../src/evolution/GitNexusIndexer";
import * as child_process from "child_process";
import { promises as fsp } from "node:fs";

const { mockExecAsync } = vi.hoisted(() => ({
    mockExecAsync: vi.fn().mockResolvedValue({ stdout: "mock stdout", stderr: "mock stderr" })
}));

vi.mock("child_process", () => ({
    exec: vi.fn()
}));

vi.mock("util", () => ({
    promisify: () => mockExecAsync
}));

vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    return {
        ...actual,
        promises: {
            ...actual.promises,
            mkdir: vi.fn(),
            writeFile: vi.fn()
        }
    };
});

vi.mock("../../src/evolution/ASTGraphBuilder", () => {
    return {
        ASTGraphBuilder: class {
            buildGraph = vi.fn().mockResolvedValue({ type: "repository", children: [] });
        }
    };
});

describe("GitNexusIndexer", () => {
    let indexer: GitNexusIndexer;

    beforeEach(() => {
        indexer = new GitNexusIndexer();
        vi.useFakeTimers();
        vi.clearAllMocks();
    });

    afterEach(() => {
        indexer.dispose();
        vi.useRealTimers();
    });

    it("should trigger index after delay", async () => {
        indexer.triggerIndex(100);
        
        expect(mockExecAsync).not.toHaveBeenCalled();
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(mockExecAsync).toHaveBeenCalled();
        expect(fsp.mkdir).toHaveBeenCalled();
        expect(fsp.writeFile).toHaveBeenCalled();
    });

    it("should debounce rapid calls", async () => {
        indexer.triggerIndex(100);
        indexer.triggerIndex(100);
        indexer.triggerIndex(100);
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(mockExecAsync).toHaveBeenCalledTimes(1);
    });

    it("should handle exec errors", async () => {
        mockExecAsync.mockRejectedValueOnce(new Error("Command failed"));
        
        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50);
        
        // It should log error and not crash
        expect(mockExecAsync).toHaveBeenCalled();
        expect(fsp.mkdir).not.toHaveBeenCalled();
    });

    it("should prevent concurrent indexing", async () => {
        let resolveExec: any;
        mockExecAsync.mockImplementation(() => new Promise(resolve => resolveExec = resolve));

        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50); // first starts

        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50); // second tries to start

        // Complete first
        resolveExec({ stdout: "", stderr: "" });
        await vi.advanceTimersByTimeAsync(10);

        expect(mockExecAsync).toHaveBeenCalledTimes(1);
    });

    it("should dispose timer", async () => {
        indexer.triggerIndex(100);
        indexer.dispose();
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(mockExecAsync).not.toHaveBeenCalled();
    });
});
