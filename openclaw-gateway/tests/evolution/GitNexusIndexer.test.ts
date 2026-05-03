import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { GitNexusIndexer } from "../../src/evolution/GitNexusIndexer";

const execMock = vi.hoisted(() => vi.fn());
vi.mock("child_process", () => ({
    exec: (cmd: string, callback: any) => {
        execMock(cmd).then(res => callback(null, res)).catch(err => callback(err));
    }
}));

describe("GitNexusIndexer", () => {
    let indexer: GitNexusIndexer;

    beforeEach(() => {
        vi.useFakeTimers();
        indexer = new GitNexusIndexer();
        execMock.mockReset();
    });

    afterEach(() => {
        indexer.dispose();
        vi.useRealTimers();
    });

    it("should debounce and trigger index", async () => {
        execMock.mockResolvedValue({ stdout: "Done", stderr: "" });
        
        indexer.triggerIndex(1000);
        indexer.triggerIndex(1000); // Should reset timer
        
        await vi.advanceTimersByTimeAsync(500);
        expect(execMock).not.toHaveBeenCalled();

        await vi.advanceTimersByTimeAsync(500); // Now it reaches 1000ms from the second trigger
        expect(execMock).toHaveBeenCalledTimes(1);
    });
});
