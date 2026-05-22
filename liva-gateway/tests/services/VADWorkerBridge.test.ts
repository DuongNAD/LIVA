import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Store worker instances on globalThis so the hoisted mock can access it
(globalThis as any).__vadWorkerInstances = [];

vi.mock("node:worker_threads", async () => {
    const { EventEmitter: EE } = await import("node:events");

    class _MockWorker extends EE {
        postMessage = vi.fn();
        terminate = vi.fn().mockResolvedValue(0);
        constructor(..._args: any[]) {
            super();
            (globalThis as any).__vadWorkerInstances.push(this);
        }
    }
    return { Worker: _MockWorker };
});

import { VADWorkerBridge } from "@services/VADWorkerBridge";

function getLastWorker(): any {
    return (globalThis as any).__vadWorkerInstances[(globalThis as any).__vadWorkerInstances.length - 1];
}

describe("VADWorkerBridge — Neural VAD Bridge", () => {
    let bridge: VADWorkerBridge;

    beforeEach(() => {
        vi.clearAllMocks();
        (globalThis as any).__vadWorkerInstances = [];
        bridge = new VADWorkerBridge();
    });

    afterEach(async () => {
        await bridge.dispose();
    });

    // ============================================================
    // Constructor
    // ============================================================
    describe("Constructor", () => {
        it("should create without error", () => {
            expect(bridge).toBeTruthy();
        });

        it("should be an EventEmitter", () => {
            expect(bridge).toBeInstanceOf(EventEmitter);
        });

        it("should start as not ready", () => {
            expect(bridge.isReady).toBe(false);
        });

        it("should start as not speaking", () => {
            expect(bridge.isSpeaking).toBe(false);
        });
    });

    // ============================================================
    // initialize()
    // ============================================================
    describe("initialize()", () => {
        it("should create a Worker and send init message", async () => {
            const initPromise = bridge.initialize("/path/to/model.onnx");
            const w = getLastWorker();

            w.emit("message", { type: "ready" });

            await initPromise;
            expect(bridge.isReady).toBe(true);
            expect(w.postMessage).toHaveBeenCalledWith(
                expect.objectContaining({ type: "init", modelPath: "/path/to/model.onnx" })
            );
        });

        it("should reject on error message", async () => {
            // Must add error listener to prevent ERR_UNHANDLED_ERROR from EventEmitter
            bridge.on("error", () => {});

            const initPromise = bridge.initialize("/path/to/model.onnx");
            const w = getLastWorker();

            w.emit("message", { type: "error", message: "Model not found" });

            await expect(initPromise).rejects.toThrow("Model not found");
        });


    });

    // ============================================================
    // pushAudioSamples()
    // ============================================================
    describe("pushAudioSamples()", () => {
        it("should not send if not ready", () => {
            bridge.pushAudioSamples(new Float32Array(480));
        });

        it("should not send if muted", async () => {
            const initPromise = bridge.initialize("/model.onnx");
            const w = getLastWorker();
            w.emit("message", { type: "ready" });
            await initPromise;

            bridge.mute();
            bridge.pushAudioSamples(new Float32Array(480));
            
            const audioCalls = w.postMessage.mock.calls.filter(
                (call: any[]) => call[0]?.type === "audio"
            );
            expect(audioCalls.length).toBe(0);
        });

        it("should send audio when ready and unmuted", async () => {
            const initPromise = bridge.initialize("/model.onnx");
            const w = getLastWorker();
            w.emit("message", { type: "ready" });
            await initPromise;

            bridge.pushAudioSamples(new Float32Array(480));

            const audioCalls = w.postMessage.mock.calls.filter(
                (call: any[]) => call[0]?.type === "audio"
            );
            expect(audioCalls.length).toBe(1);
        });
    });

    // ============================================================
    // mute() / unmute()
    // ============================================================
    describe("mute() / unmute()", () => {
        it("should mute without error", () => {
            expect(() => bridge.mute()).not.toThrow();
        });

        it("should unmute without error", () => {
            bridge.mute();
            expect(() => bridge.unmute()).not.toThrow();
        });
    });

    // ============================================================
    // Speech state machine
    // ============================================================
    describe("Speech state machine", () => {
        let w: any;

        beforeEach(async () => {
            const initPromise = bridge.initialize("/model.onnx");
            w = getLastWorker();
            w.emit("message", { type: "ready" });
            await initPromise;
        });

        it("should emit speech_start after 3 consecutive speech frames", () => {
            const spy = vi.fn();
            bridge.on("speech_start", spy);

            for (let i = 0; i < 3; i++) {
                w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });
            }

            expect(spy).toHaveBeenCalledOnce();
            expect(bridge.isSpeaking).toBe(true);
        });

        it("should NOT emit speech_start with only 2 frames", () => {
            const spy = vi.fn();
            bridge.on("speech_start", spy);

            for (let i = 0; i < 2; i++) {
                w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });
            }

            expect(spy).not.toHaveBeenCalled();
            expect(bridge.isSpeaking).toBe(false);
        });

        it("should emit speech_end after 8 consecutive silence frames during speech", () => {
            const startSpy = vi.fn();
            const endSpy = vi.fn();
            bridge.on("speech_start", startSpy);
            bridge.on("speech_end", endSpy);

            for (let i = 0; i < 3; i++) {
                w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });
            }
            expect(startSpy).toHaveBeenCalledOnce();

            for (let i = 0; i < 8; i++) {
                w.emit("message", { type: "vad_result", isSpeech: false, confidence: 0.1 });
            }
            expect(endSpy).toHaveBeenCalledOnce();
            expect(bridge.isSpeaking).toBe(false);
        });

        it("should reset speech frame counter on silence frame", () => {
            const spy = vi.fn();
            bridge.on("speech_start", spy);

            w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });
            w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });
            w.emit("message", { type: "vad_result", isSpeech: false, confidence: 0.1 });
            w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });
            w.emit("message", { type: "vad_result", isSpeech: true, confidence: 0.9 });

            expect(spy).not.toHaveBeenCalled();
        });
    });

    // ============================================================
    // dispose()
    // ============================================================
    describe("dispose()", () => {
        it("should clean up without error", async () => {
            await expect(bridge.dispose()).resolves.toBeUndefined();
        });

        it("should set isReady to false after init", async () => {
            const initPromise = bridge.initialize("/model.onnx");
            const w = getLastWorker();
            w.emit("message", { type: "ready" });
            await initPromise;

            await bridge.dispose();
            expect(bridge.isReady).toBe(false);
        });
    });
});
