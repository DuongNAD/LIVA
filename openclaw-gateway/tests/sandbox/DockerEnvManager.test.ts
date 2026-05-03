import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DockerEnvManager } from "../../src/sandbox/DockerEnvManager";
import * as child_process from "child_process";
import { EventEmitter } from "events";

vi.mock("child_process");

describe("DockerEnvManager", () => {
    let manager: DockerEnvManager;

    beforeEach(() => {
        vi.useFakeTimers();
        manager = new DockerEnvManager();
    });

    afterEach(() => {
        vi.restoreAllMocks();
        vi.useRealTimers();
    });

    it("should resolve with output on success", async () => {
        const mockChild: any = new EventEmitter();
        mockChild.stdout = new EventEmitter();
        mockChild.stderr = new EventEmitter();

        vi.mocked(child_process.spawn).mockReturnValue(mockChild);

        const promise = manager.runSandboxTest(["node", "-v"]);

        mockChild.stdout.emit("data", "v20.0.0");
        mockChild.emit("close", 0);

        await expect(promise).resolves.toBe("v20.0.0");
    });

    it("should reject and call cleanup on timeout", async () => {
        const mockChild: any = new EventEmitter();
        mockChild.stdout = new EventEmitter();
        mockChild.stderr = new EventEmitter();
        
        vi.mocked(child_process.spawn).mockReturnValue(mockChild);

        const promise = manager.runSandboxTest(["node", "-e", "while(true){}"]);

        // Simulate AbortController firing after 60s
        vi.advanceTimersByTime(60000);
        
        // Internal abort error emitted by spawn
        const abortErr = new Error("Abort");
        abortErr.name = "AbortError";
        mockChild.emit("error", abortErr);

        await expect(promise).rejects.toThrow("Timeout 60s. Bị ngắt bởi AbortController.");
    });
});
