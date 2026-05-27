import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

const { MockWorker } = vi.hoisted(() => {
    const { EventEmitter } = require("node:events");
    class MockWorker extends EventEmitter {
        postMessage = vi.fn();
        terminate = vi.fn().mockResolvedValue(0);
    }
    return { MockWorker };
});

vi.mock("node:worker_threads", () => ({
    Worker: MockWorker
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { EmbeddingService, EmbeddingNotReadyError } from "../../src/services/EmbeddingService";

describe("EmbeddingService — Crash Recovery & Resilience", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    afterEach(() => {
        const instance = EmbeddingService.getInstance();
        instance.dispose();
        // @ts-ignore
        EmbeddingService.instance = undefined;
        vi.useRealTimers();
    });

    it("should handle worker exit mid-execution and recover on subsequent requests", async () => {
        const service = EmbeddingService.getInstance();
        
        // 1. Initial successful startup
        const initPromise1 = service.ensureReady();
        const mockWorker1 = (service as any).worker as MockWorker;
        mockWorker1.emit("message", { type: "ready" });
        await initPromise1;
        expect(service.ready).toBe(true);

        // 2. Simulate worker crash/exit
        mockWorker1.emit("exit", 1);
        expect(service.ready).toBe(false);

        // 3. Next request should auto-reinitialize a new worker
        const embedPromise = service.embed("hello");
        
        // Flush microtasks to allow ensureReady() inside embed() to run and instantiate the new worker
        await Promise.resolve();
        await new Promise(r => setTimeout(r, 10));

        const mockWorker2 = (service as any).worker as MockWorker;
        
        // Ensure it's a newly spawned worker instance
        expect(mockWorker2).not.toBe(mockWorker1);
        
        // Simulate the new worker getting ready
        mockWorker2.emit("message", { type: "ready" });
        
        // Flush microtasks so the embed request gets posted to mockWorker2
        await Promise.resolve();
        await new Promise(r => setTimeout(r, 10));

        const call = mockWorker2.postMessage.mock.calls.find(c => c[0].type === "embed");
        expect(call).toBeDefined();
        const reqId = call![0].id;
        mockWorker2.emit("message", { type: "embed_result", id: reqId, vector: [0.9, 0.9, 0.9] });

        const result = await embedPromise;
        expect(result).toEqual([0.9, 0.9, 0.9]);
        expect(service.ready).toBe(true);
    });

    it("should reject pending requests when service is disposed", async () => {
        const service = EmbeddingService.getInstance();
        const initPromise = service.ensureReady();
        const mockWorker = (service as any).worker as MockWorker;
        mockWorker.emit("message", { type: "ready" });
        await initPromise;

        const embedPromise = service.embed("test");
        
        // Wait for microtask tick to let isReady check pass and request get registered
        await Promise.resolve();
        
        // Dispose service while request is pending
        service.dispose();

        await expect(embedPromise).rejects.toThrow("EmbeddingService disposed");
    });

    it("should trigger default 30s timeout if worker hangs on a single embed request", async () => {
        const service = EmbeddingService.getInstance();
        const initPromise = service.ensureReady();
        const mockWorker = (service as any).worker as MockWorker;
        mockWorker.emit("message", { type: "ready" });
        await initPromise;

        vi.useFakeTimers();
        const embedPromise = service.embed("hung text");
        // Attach catch handler before advancing time to prevent unhandled rejection errors
        embedPromise.catch(() => {});
        
        // Let the microtask execute to register the setTimeout inside embed()
        await vi.advanceTimersByTimeAsync(0);

        // Advance timers past 30 seconds
        await vi.advanceTimersByTimeAsync(31_000);

        await expect(embedPromise).rejects.toThrow("Embedding timeout (default 30s)");
    });

    it("should trigger default 60s timeout if worker hangs on a batch embed request", async () => {
        const service = EmbeddingService.getInstance();
        const initPromise = service.ensureReady();
        const mockWorker = (service as any).worker as MockWorker;
        mockWorker.emit("message", { type: "ready" });
        await initPromise;

        vi.useFakeTimers();
        const batchPromise = service.embedBatch(["hang1", "hang2"]);
        // Attach catch handler before advancing time to prevent unhandled rejection errors
        batchPromise.catch(() => {});
        
        // Let the microtask execute to register the setTimeout inside embedBatch()
        await vi.advanceTimersByTimeAsync(0);

        // Advance timers past 60 seconds
        await vi.advanceTimersByTimeAsync(61_000);

        await expect(batchPromise).rejects.toThrow("Embedding batch timeout (default 60s)");
    });

    it("should reject request immediately if worker returns error message", async () => {
        const service = EmbeddingService.getInstance();
        const initPromise = service.ensureReady();
        const mockWorker = (service as any).worker as MockWorker;
        mockWorker.emit("message", { type: "ready" });
        await initPromise;

        const embedPromise = service.embed("error text");
        
        await Promise.resolve();
        await new Promise(r => setTimeout(r, 10));

        const call = mockWorker.postMessage.mock.calls.find(c => c[0].type === "embed");
        expect(call).toBeDefined();
        const reqId = call![0].id;

        // Simulate worker error result
        mockWorker.emit("message", { type: "error", id: reqId, message: "ONNX Runtime inference failed" });

        await expect(embedPromise).rejects.toThrow("ONNX Runtime inference failed");
    });
});
