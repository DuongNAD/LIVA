import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

const { MockWorker } = vi.hoisted(() => {
    const { EventEmitter } = require("node:events");
    class MockWorker extends EventEmitter {
        postMessage = vi.fn();
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

describe("EmbeddingService", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    afterEach(() => {
        const instance = EmbeddingService.getInstance();
        instance.dispose();
        // @ts-ignore
        EmbeddingService.instance = undefined;
    });

    describe("Singleton Pattern", () => {
        it("should return the same instance on multiple calls", () => {
            const a = EmbeddingService.getInstance();
            const b = EmbeddingService.getInstance();
            expect(a).toBe(b);
        });

        it("should create a new instance after dispose", () => {
            const a = EmbeddingService.getInstance();
            a.dispose();
            // @ts-ignore
            EmbeddingService.instance = undefined;
            const b = EmbeddingService.getInstance();
            expect(a).not.toBe(b);
        });
    });

    describe("ensureReady", () => {
        it("should initialize the worker and resolve when ready message is received", async () => {
            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();

            const mockWorker = (service as any).worker as MockWorker;
            expect(mockWorker.postMessage).toHaveBeenCalledWith({ type: "init", useMultilingual: false });

            // Simulate worker ready
            mockWorker.emit("message", { type: "ready" });

            await initPromise;
            expect(service.ready).toBe(true);
            expect(service.dimension).toBe(384);
            expect(service.supportsMRL).toBe(false);
            expect(service.modelId).toBe("all-MiniLM-L6-v2");
        });

        it("should initialize the worker in multilingual mode when FF is enabled", async () => {
            const { FF } = await import("../../src/utils/FeatureFlags");
            vi.spyOn(FF, "isEnabled").mockImplementation((flag) => flag === "MULTILINGUAL_EMBEDDING");

            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();

            const mockWorker = (service as any).worker as MockWorker;
            expect(mockWorker.postMessage).toHaveBeenCalledWith({ type: "init", useMultilingual: true });

            mockWorker.emit("message", { type: "ready" });
            await initPromise;

            expect(service.modelId).toBe("multilingual-e5-small");
        });

        it("should throw if worker errors during init", async () => {
            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();

            const mockWorker = (service as any).worker as MockWorker;
            mockWorker.emit("error", new Error("Failed to load model"));

            await expect(initPromise).rejects.toThrow("Failed to load model");
            expect(service.ready).toBe(false);
        });
    });

    describe("embed()", () => {
        it("should send embed message and resolve vector", async () => {
            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();
            const mockWorker = (service as any).worker as MockWorker;
            mockWorker.emit("message", { type: "ready" });
            await initPromise;

            const embedPromise = service.embed("test");
            await Promise.resolve(); // Flush microtasks so ensureReady() completes
            expect(mockWorker.postMessage).toHaveBeenCalledWith(
                expect.objectContaining({ type: "embed", text: "test" })
            );

            // Find the req ID
            const call = mockWorker.postMessage.mock.calls.find(c => c[0].type === "embed");
            const reqId = call![0].id;

            mockWorker.emit("message", { type: "embed_result", id: reqId, vector: [0.1, 0.2, 0.3] });

            const vector = await embedPromise;
            expect(vector).toEqual([0.1, 0.2, 0.3]);
        });

        it("should throw if service is not ready", async () => {
            const service = EmbeddingService.getInstance();
            const embedPromise = service.embed("test");
            const mockWorker = (service as any).worker as MockWorker;
            mockWorker.emit("error", new Error("Worker failed"));
            await expect(embedPromise).rejects.toThrow(EmbeddingNotReadyError);
        });
    });

    describe("embedBatch()", () => {
        it("should send embed_batch message and resolve vectors", async () => {
            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();
            const mockWorker = (service as any).worker as MockWorker;
            mockWorker.emit("message", { type: "ready" });
            await initPromise;

            const batchPromise = service.embedBatch(["test1", "test2"]);
            await Promise.resolve(); // Flush microtasks
            const call = mockWorker.postMessage.mock.calls.find(c => c[0].type === "embed_batch");
            const reqId = call![0].id;

            mockWorker.emit("message", {
                type: "embed_batch_result",
                id: reqId,
                vectors: [[0.1], [0.2]]
            });

            const vectors = await batchPromise;
            expect(vectors).toEqual([[0.1], [0.2]]);
        });
    });

    describe("embedWithTimeout()", () => {
        it("should reject on timeout", async () => {
            vi.useFakeTimers();
            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();
            const mockWorker = (service as any).worker as MockWorker;
            mockWorker.emit("message", { type: "ready" });
            await initPromise;

            const embedPromise = service.embedWithTimeout("test", 1000);
            await Promise.resolve(); // Flush microtasks
            
            // Advance timers to trigger timeout
            vi.advanceTimersByTime(1100);

            await expect(embedPromise).rejects.toThrow("Embedding Worker unavailable or timeout.");
            vi.useRealTimers();
        });
    });

    describe("e5 asymmetric prefix", () => {
        async function readyService(multilingual: boolean) {
            if (multilingual) {
                const { FF } = await import("../../src/utils/FeatureFlags");
                vi.spyOn(FF, "isEnabled").mockImplementation((flag) => flag === "MULTILINGUAL_EMBEDDING");
            }
            const service = EmbeddingService.getInstance();
            const initPromise = service.ensureReady();
            const mockWorker = (service as any).worker as MockWorker;
            mockWorker.emit("message", { type: "ready" });
            await initPromise;
            return { service, mockWorker };
        }

        // These cases inspect the postMessage payload without resolving the worker,
        // so the embed promise stays pending until afterEach dispose() rejects it.
        // Swallow that expected rejection to avoid unhandled-rejection noise.
        it("does NOT prefix when the MiniLM (symmetric) model is active", async () => {
            const { service, mockWorker } = await readyService(false);

            void service.embed("xin chào", "passage").catch(() => {});
            await Promise.resolve();
            const call = mockWorker.postMessage.mock.calls.find(c => c[0].type === "embed");
            expect(call![0].text).toBe("xin chào");
        });

        it("prefixes query: and passage: when the multilingual e5 model is active", async () => {
            const { service, mockWorker } = await readyService(true);

            void service.embed("thời tiết hôm nay", "query").catch(() => {});
            void service.embed("Hà Nội là thủ đô", "passage").catch(() => {});
            await Promise.resolve();

            const embedCalls = mockWorker.postMessage.mock.calls.filter(c => c[0].type === "embed");
            expect(embedCalls[0][0].text).toBe("query: thời tiết hôm nay");
            expect(embedCalls[1][0].text).toBe("passage: Hà Nội là thủ đô");
        });

        it("defaults to the query prefix when kind is omitted (e5 active)", async () => {
            const { service, mockWorker } = await readyService(true);

            void service.embed("ai là tổng thống").catch(() => {});
            await Promise.resolve();
            const call = mockWorker.postMessage.mock.calls.find(c => c[0].type === "embed");
            expect(call![0].text).toBe("query: ai là tổng thống");
        });

        it("prefixes every item in a batch (e5 active)", async () => {
            const { service, mockWorker } = await readyService(true);

            void service.embedBatch(["một", "hai"], "passage").catch(() => {});
            await Promise.resolve();
            const call = mockWorker.postMessage.mock.calls.find(c => c[0].type === "embed_batch");
            expect(call![0].texts).toEqual(["passage: một", "passage: hai"]);
        });
    });

    describe("Matryoshka Truncation", () => {
        it("should return unchanged vector", () => {
            const service = EmbeddingService.getInstance();
            const vec = [1, 2, 3];
            expect(service.truncateMatryoshka(vec, 2)).toEqual(vec);
        });
    });
});
