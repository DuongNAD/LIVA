import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));
import { logger } from "../../src/utils/logger";

// Mock HttpClient
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

// Mock TTSFormatter as a class
vi.mock("../../src/utils/TTSFormatter", () => {
    const TTSFormatter = vi.fn().mockImplementation(function(this: any) {
        this.pushToken = vi.fn().mockReturnValue(null);
        this.flush = vi.fn().mockReturnValue(null);
        this.reset = vi.fn();
    });
    return { TTSFormatter };
});

// Mock fs
vi.mock("node:fs", () => ({
    promises: { readFile: vi.fn() },
}));

const { MockWebSocket, mockWsSend, mockWsClose, mockWsRemoveAllListeners } = vi.hoisted(() => {
    const EventEmitter = require("node:events").EventEmitter;
    const mockWsSend = vi.fn();
    const mockWsClose = vi.fn();
    const mockWsRemoveAllListeners = vi.fn();

    class MockWebSocket extends EventEmitter {
        static OPEN = 1;
        readyState = 1;
        send = mockWsSend;
        close = mockWsClose;
        removeAllListeners = mockWsRemoveAllListeners;
    }

    return {
        MockWebSocket,
        mockWsSend,
        mockWsClose,
        mockWsRemoveAllListeners
    };
});

vi.mock("ws", () => ({
    default: MockWebSocket,
    WebSocket: { OPEN: 1 },
}));

import { VoiceEngine } from "@services/VoiceEngine";
import { safeFetch } from "../../src/utils/HttpClient";

describe("VoiceEngine — Python Edge-TTS Relay", () => {
    let engine: VoiceEngine;

    beforeEach(() => {
        vi.clearAllMocks();
        engine = new VoiceEngine();
    });

    afterEach(async () => {
        await engine.destroy();
    });

    // ============================================================
    // Constructor & Connection
    // ============================================================
    describe("Constructor", () => {
        it("should instantiate without throwing", () => {
            expect(engine).toBeTruthy();
        });

        it("should be an EventEmitter", () => {
            expect(engine).toBeInstanceOf(EventEmitter);
        });
    });

    // ============================================================
    // speak() — HTTP TTS
    // ============================================================
    describe("speak()", () => {
        it("should return true and emit audio on successful API call", async () => {
            vi.mocked(safeFetch).mockResolvedValue({
                ok: true,
                json: async () => ({ audio: "base64data" }),
            } as any);

            const audioSpy = vi.fn();
            engine.on("audio_base64", audioSpy);

            const result = await engine.speak("Xin chào");
            expect(result).toBe(true);
            expect(audioSpy).toHaveBeenCalledWith("base64data");
        });

        it("should return true but not emit when API returns no audio", async () => {
            vi.mocked(safeFetch).mockResolvedValue({
                ok: true,
                json: async () => ({}),
            } as any);

            const audioSpy = vi.fn();
            engine.on("audio_base64", audioSpy);

            const result = await engine.speak("Test");
            expect(result).toBe(true);
            expect(audioSpy).not.toHaveBeenCalled();
        });

        it("should return false on HTTP error response", async () => {
            vi.mocked(safeFetch).mockResolvedValue({
                ok: false,
                status: 500,
            } as any);

            const result = await engine.speak("Fail");
            expect(result).toBe(false);
        });

        it("should return false on network error", async () => {
            vi.mocked(safeFetch).mockRejectedValue(new Error("ECONNREFUSED"));

            const result = await engine.speak("Offline");
            expect(result).toBe(false);
        });
    });

    // ============================================================
    // preempt() — Barge-in
    // ============================================================
    describe("preempt()", () => {
        it("should not throw when called", () => {
            expect(() => engine.preempt()).not.toThrow();
        });
    });

    // ============================================================
    // Heartbeat
    // ============================================================
    describe("Heartbeat", () => {
        beforeEach(() => {
            vi.useFakeTimers();
        });

        afterEach(() => {
            vi.useRealTimers();
        });

        it("should start heartbeat on open and send ping every 30s", () => {
            const wsInstance = (engine as any).ws;
            expect(wsInstance).toBeTruthy();
            
            wsInstance.emit("open");
            
            vi.advanceTimersByTime(30000);
            expect(mockWsSend).toHaveBeenCalledWith(JSON.stringify({ type: "ping" }));
        });

        it("should stop heartbeat on close", () => {
            const wsInstance = (engine as any).ws;
            wsInstance.emit("open");
            wsInstance.emit("close");
            
            mockWsSend.mockClear();
            vi.advanceTimersByTime(30000);
            expect(mockWsSend).not.toHaveBeenCalled();
        });
    });

    // ============================================================
    // destroy() — Cleanup
    // ============================================================
    describe("destroy()", () => {
        it("should clean up resources without throwing", async () => {
            await expect(engine.destroy()).resolves.toBeUndefined();
        });
    });
});
