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

// Mock logger to avoid console pollution during testing
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import { NemotronSTTService } from "@services/NemotronSTTService";

describe("NemotronSTTService Unit Tests", () => {
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

    describe("Constructor", () => {
        it("should create service instance and verify initial properties", () => {
            expect(service).toBeInstanceOf(NemotronSTTService);
            expect(service.isCircuitOpen()).toBe(false);
        });
    });

    describe("Initialization", () => {
        it("should successfully resolve on 'ready' message", async () => {
            const tempService = new NemotronSTTService();
            const initPromise = tempService.initialize();

            const tempMockWorker = instances[instances.length - 1];
            expect(tempMockWorker).toBeDefined();

            tempMockWorker.emit("message", { type: "ready" });
            await expect(initPromise).resolves.toBeUndefined();
            tempService.destroy();
        });

        it("should reject initialization if the worker sends an 'error' message", async () => {
            const tempService = new NemotronSTTService();
            const initPromise = tempService.initialize();

            const tempMockWorker = instances[instances.length - 1];
            expect(tempMockWorker).toBeDefined();

            tempMockWorker.emit("message", { type: "error", message: "Initialization failed" });
            await expect(initPromise).rejects.toThrow("Initialization failed");
            tempService.destroy();
        });
    });

    describe("Audio Ingestion", () => {
        it("should send audio buffer data to the worker when pushAudioChunkOnly is called", () => {
            const chunk = new Float32Array(256);
            service.pushAudioChunkOnly(chunk);

            expect(mockWorkerInstance.postMessage).toHaveBeenCalled();
            const calls = mockWorkerInstance.postMessage.mock.calls;
            const lastCall = calls[calls.length - 1][0];
            expect(lastCall.type).toBe("audio_chunk");
            expect(lastCall.isLast).toBe(false);
            expect(lastCall.buffer).toBeInstanceOf(Float32Array);
        });

        it("should send audio buffer data to the worker when pushAudioChunk is called", () => {
            const chunk = new Float32Array(256);
            service.pushAudioChunk(chunk);

            expect(mockWorkerInstance.postMessage).toHaveBeenCalled();
            const calls = mockWorkerInstance.postMessage.mock.calls;
            const lastCall = calls[calls.length - 1][0];
            expect(lastCall.type).toBe("audio_chunk");
            expect(lastCall.isLast).toBe(false);
            expect(lastCall.buffer).toBeInstanceOf(Float32Array);
        });

        it("should trigger transcription and send empty final chunk with isLast=true after silence timeout in pushAudioChunk", async () => {
            const chunk = new Float32Array(256);
            service.pushAudioChunk(chunk);

            // Advance time to trigger silence timer (800ms)
            await vi.advanceTimersByTimeAsync(850);

            const calls = mockWorkerInstance.postMessage.mock.calls;
            const finalCall = calls[calls.length - 1][0];
            expect(finalCall.type).toBe("audio_chunk");
            expect(finalCall.isLast).toBe(true);
            expect(finalCall.buffer.length).toBe(0);
        });
    });

    describe("Transcription Events", () => {
        it("should trigger transcription_partial event when receiving 'partial' message from worker", () => {
            const partialSpy = vi.fn();
            service.on("transcription_partial", partialSpy);

            mockWorkerInstance.emit("message", { type: "partial", text: "partial text" });

            expect(partialSpy).toHaveBeenCalledWith("partial text");
        });

        it("should trigger transcription_ready event when receiving 'final' message from worker", () => {
            const finalSpy = vi.fn();
            service.on("transcription_ready", finalSpy);

            mockWorkerInstance.emit("message", { type: "final", text: "final text" });

            expect(finalSpy).toHaveBeenCalledWith("final text");
        });

        it("should ignore empty/whitespace-only 'final' messages from worker", () => {
            const finalSpy = vi.fn();
            service.on("transcription_ready", finalSpy);

            mockWorkerInstance.emit("message", { type: "final", text: "   " });

            expect(finalSpy).not.toHaveBeenCalled();
        });
    });

    describe("Circuit Breaker", () => {
        it("should open after 3 consecutive failures, emit event, and reset after timeout", async () => {
            const fallbackActivatedSpy = vi.fn();
            const fallbackDeactivatedSpy = vi.fn();

            service.on("stt_fallback_activated", fallbackActivatedSpy);
            service.on("stt_fallback_deactivated", fallbackDeactivatedSpy);

            expect(service.isCircuitOpen()).toBe(false);

            // Fail 1
            mockWorkerInstance.emit("message", { type: "error", message: "fail 1" });
            expect(service.isCircuitOpen()).toBe(false);

            // Fail 2
            mockWorkerInstance.emit("message", { type: "error", message: "fail 2" });
            expect(service.isCircuitOpen()).toBe(false);

            // Fail 3
            mockWorkerInstance.emit("message", { type: "error", message: "fail 3" });
            expect(service.isCircuitOpen()).toBe(true);

            expect(fallbackActivatedSpy).toHaveBeenCalledTimes(1);
            expect(fallbackDeactivatedSpy).not.toHaveBeenCalled();

            // Advance timers by CIRCUIT_RESET_MS (15000ms)
            await vi.advanceTimersByTimeAsync(15000);

            expect(service.isCircuitOpen()).toBe(false);
            expect(fallbackDeactivatedSpy).toHaveBeenCalledTimes(1);
        });
    });

    describe("Cleanup & Dispose", () => {
        it("should terminate worker, stop timers, and reset state on destroy()", () => {
            const postMessageSpy = mockWorkerInstance.postMessage;
            const terminateSpy = mockWorkerInstance.terminate;

            service.destroy();

            // Should send dispose message and terminate worker
            expect(postMessageSpy).toHaveBeenCalledWith({ type: "dispose" });
            expect(terminateSpy).toHaveBeenCalled();
            expect(service.isCircuitOpen()).toBe(false);
        });
    });
});
