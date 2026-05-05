import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { GitNexusIndexer } from "../../src/evolution/GitNexusIndexer";
import * as child_process from "child_process";
import { promises as fsp } from "node:fs";
import { EventEmitter } from "events";

// Mock fs.existsSync for resolveGitNexusBin (preserves promises export)
vi.mock("fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("fs")>();
    return {
        ...actual,
        existsSync: vi.fn().mockReturnValue(true),
    };
});

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

function createMockProc(exitCode: number = 0) {
    const proc = new EventEmitter() as any;
    proc.stdout = new EventEmitter();
    proc.stderr = new EventEmitter();
    // Auto-emit close after a tick
    setTimeout(() => {
        proc.emit("close", exitCode);
    }, 5);
    return proc;
}

vi.mock("child_process", () => ({
    spawn: vi.fn(() => createMockProc(0)),
}));

describe("GitNexusIndexer", () => {
    let indexer: GitNexusIndexer;
    const spawnMock = vi.mocked(child_process.spawn);

    beforeEach(() => {
        indexer = new GitNexusIndexer();
        vi.useFakeTimers();
        vi.clearAllMocks();
        spawnMock.mockImplementation(() => createMockProc(0) as any);
    });

    afterEach(() => {
        indexer.dispose();
        vi.useRealTimers();
    });

    it("should trigger index after delay (no embeddings by default)", async () => {
        indexer.triggerIndex(100);
        
        expect(spawnMock).not.toHaveBeenCalled();
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(spawnMock).toHaveBeenCalled();
        // Should NOT include --embeddings by default
        const args = spawnMock.mock.calls[0][1] as string[];
        expect(args).toContain("analyze");
        expect(args).not.toContain("--embeddings");
    });

    it("should include --embeddings when opted-in", async () => {
        indexer.triggerIndex(100, { embeddings: true });
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(spawnMock).toHaveBeenCalled();
        const args = spawnMock.mock.calls[0][1] as string[];
        expect(args).toContain("analyze");
        expect(args).toContain("--embeddings");
    });

    it("should debounce rapid calls", async () => {
        indexer.triggerIndex(100);
        indexer.triggerIndex(100);
        indexer.triggerIndex(100);
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(spawnMock).toHaveBeenCalledTimes(1);
    });

    it("should handle spawn errors", async () => {
        spawnMock.mockImplementation(() => {
            const proc = new EventEmitter() as any;
            proc.stdout = new EventEmitter();
            proc.stderr = new EventEmitter();
            setTimeout(() => proc.emit("close", 1), 5);
            return proc as any;
        });

        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50);
        
        expect(spawnMock).toHaveBeenCalled();
        expect(fsp.mkdir).not.toHaveBeenCalled();
    });

    it("should handle spawn 'error' event", async () => {
        spawnMock.mockImplementation(() => {
            const proc = new EventEmitter() as any;
            proc.stdout = new EventEmitter();
            proc.stderr = new EventEmitter();
            setTimeout(() => proc.emit("error", new Error("ENOENT")), 5);
            return proc as any;
        });

        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50);
        
        expect(spawnMock).toHaveBeenCalled();
        expect(fsp.mkdir).not.toHaveBeenCalled();
    });

    it("should prevent concurrent indexing", async () => {
        // Use a never-resolving proc for first call
        spawnMock.mockImplementation(() => {
            const proc = new EventEmitter() as any;
            proc.stdout = new EventEmitter();
            proc.stderr = new EventEmitter();
            // Never emits close — simulates long running process
            return proc as any;
        });

        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50); // first starts

        indexer.triggerIndex(10);
        await vi.advanceTimersByTimeAsync(50); // second tries to start

        expect(spawnMock).toHaveBeenCalledTimes(1);
    });

    it("should dispose timer", async () => {
        indexer.triggerIndex(100);
        indexer.dispose();
        
        await vi.advanceTimersByTimeAsync(150);
        
        expect(spawnMock).not.toHaveBeenCalled();
    });
});
