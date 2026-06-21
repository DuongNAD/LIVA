import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
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

// Mock EdgeTTSClient
const mockSynthesize = vi.fn();
const mockSetVoice = vi.fn().mockReturnValue(true);
vi.mock("../../src/services/EdgeTTSClient", () => ({
    EdgeTTSClient: class MockEdgeTTSClient {
        synthesize = mockSynthesize;
        setVoice = mockSetVoice;
        get voice() { return "vi-VN-HoaiMyNeural"; }
    },
}));

import { VoiceEngine } from "@services/VoiceEngine";

describe("VoiceEngine v4 — Direct Node.js Edge-TTS", () => {
    let engine: VoiceEngine;

    beforeEach(() => {
        vi.clearAllMocks();
        engine = new VoiceEngine();
    });

    afterEach(async () => {
        await engine.destroy();
    });

    // ============================================================
    // Constructor & Identity
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
    // speak() — Direct Edge-TTS synthesis
    // ============================================================
    describe("speak()", () => {
        it("should return true and emit audio on successful synthesis", async () => {
            const fakeAudio = Buffer.from("fake-mp3-data");
            mockSynthesize.mockResolvedValue(fakeAudio);

            const audioSpy = vi.fn();
            const bufferSpy = vi.fn();
            engine.on("audio_base64", audioSpy);
            engine.on("audio_buffer", bufferSpy);

            const result = await engine.speak("Xin chào");
            expect(result).toBe(true);
            expect(audioSpy).toHaveBeenCalledWith(fakeAudio.toString("base64"));
            // [Optimization C4] Also emits raw buffer for binary protocol
            expect(bufferSpy).toHaveBeenCalledWith(fakeAudio);
        });

        it("should return false when synthesis returns null (no audio)", async () => {
            mockSynthesize.mockResolvedValue(null);

            const audioSpy = vi.fn();
            engine.on("audio_base64", audioSpy);

            const result = await engine.speak("Test");
            expect(result).toBe(false);
            expect(audioSpy).not.toHaveBeenCalled();
        });

        it("should return false on synthesis error", async () => {
            mockSynthesize.mockRejectedValue(new Error("Azure CDN unreachable"));

            const result = await engine.speak("Fail");
            expect(result).toBe(false);
        });

        it("should return true for empty text", async () => {
            const result = await engine.speak("   ");
            expect(result).toBe(true);
            expect(mockSynthesize).not.toHaveBeenCalled();
        });

        it("should return true immediately and not call synthesis when text is a single space (keepalive probe)", async () => {
            const result = await engine.speak(" ");
            expect(result).toBe(true);
            expect(mockSynthesize).not.toHaveBeenCalled();
        });

        it("should return false after destroy", async () => {
            await engine.destroy();
            const result = await engine.speak("After destroy");
            expect(result).toBe(false);
        });
    });

    // ============================================================
    // setVoiceProfile()
    // ============================================================
    describe("setVoiceProfile()", () => {
        it("should delegate to EdgeTTSClient.setVoice", () => {
            engine.setVoiceProfile("en-US-AriaNeural");
            expect(mockSetVoice).toHaveBeenCalledWith("en-US-AriaNeural");
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
    // destroy() — Cleanup
    // ============================================================
    describe("destroy()", () => {
        it("should clean up resources without throwing", async () => {
            await expect(engine.destroy()).resolves.toBeUndefined();
        });
    });
});
