import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Hoisted Mock of node:worker_threads
const { MockWorker, instances } = vi.hoisted(() => {
    const { EventEmitter } = require("node:events");
    const instances: any[] = [];
    class MockWorker extends EventEmitter {
        postMessage = vi.fn();
        terminate = vi.fn().mockResolvedValue(0);
        constructor() {
            super();
            instances.push(this);
        }
    }
    return { MockWorker, instances };
});

vi.mock("node:worker_threads", async (importOriginal) => {
    const original = await importOriginal<typeof import("node:worker_threads")>();
    return {
        ...original,
        Worker: MockWorker,
    };
});

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { NemotronSTTService } from "@services/NemotronSTTService";

describe("NemotronSTTService", () => {
    let service: NemotronSTTService;
    let mockWorkerInstance: any;

    beforeEach(async () => {
        vi.clearAllMocks();
        instances.length = 0;
        vi.useFakeTimers();

        service = new NemotronSTTService();

        // Spy on worker instantiation during initialize
        const initPromise = service.initialize();

        // Get the instantiated mock worker
        mockWorkerInstance = instances[0];
        expect(mockWorkerInstance).toBeDefined();

        // Simulate ready message
        mockWorkerInstance.emit("message", { type: "ready" });
        await initPromise;

        mockWorkerInstance.postMessage.mockClear();
    });

    afterEach(() => {
        service.destroy();
        vi.useRealTimers();
    });

    describe("Constructor & Initialization", () => {
        it("should initialize and set isReady", () => {
            expect(service).toBeTruthy();
        });
    });

    describe("Audio Handling", () => {
        it("should accept audio chunk and forward to worker in pushAudioChunkOnly", () => {
            const chunk = Buffer.alloc(1024);
            service.pushAudioChunkOnly(chunk);
            expect(mockWorkerInstance.postMessage).toHaveBeenCalled();
            const call = mockWorkerInstance.postMessage.mock.calls[0][0];
            expect(call.type).toBe("audio_chunk");
            expect(call.isLast).toBe(false);
        });

        it("should handle silence timer in pushAudioChunk", async () => {
            const chunk = Buffer.alloc(1024);
            service.pushAudioChunk(chunk);
            expect(mockWorkerInstance.postMessage).toHaveBeenCalled();

            // advance timer by 800ms to trigger transcription
            await vi.advanceTimersByTimeAsync(850);
            const calls = mockWorkerInstance.postMessage.mock.calls;
            const finalCall = calls[calls.length - 1][0];
            expect(finalCall.type).toBe("audio_chunk");
            expect(finalCall.isLast).toBe(true);
        });
    });

    describe("Transcription Finalization & Flushing", () => {
        it("should trigger transcription and post final message to worker", () => {
            // Push some audio first to enable streaming
            service.pushAudioChunkOnly(Buffer.alloc(1024));

            service.triggerTranscription();
            const calls = mockWorkerInstance.postMessage.mock.calls;
            const lastCall = calls[calls.length - 1][0];
            expect(lastCall.type).toBe("audio_chunk");
            expect(lastCall.isLast).toBe(true);
        });

        it("should flush and reset state", () => {
            service.flush();
            expect(mockWorkerInstance.postMessage).toHaveBeenCalledWith({ type: "reset" });
        });
    });

    describe("Circuit Breaker & Fallback", () => {
        it("should activate circuit breaker after consecutive failures", () => {
            const fallbackSpy = vi.fn();
            service.on("stt_fallback_activated", fallbackSpy);

            // Trigger 3 failures
            for (let i = 0; i < 3; i++) {
                mockWorkerInstance.emit("message", { type: "error", message: "inference error" });
            }

            expect(service.isCircuitOpen()).toBe(true);
            expect(fallbackSpy).toHaveBeenCalled();
        });
    });
});
