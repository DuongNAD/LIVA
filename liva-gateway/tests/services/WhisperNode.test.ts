import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { EventEmitter } from "node:events";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock HttpClient
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

vi.mock("../../src/core/config/ConfigManager", () => ({
    ConfigManager: {
        getInstance: () => ({
            get aiProvider() { return process.env.AI_PROVIDER || "local"; },
            get env() { 
                return {
                    get WHISPER_URL() { return process.env.WHISPER_URL; },
                    get WHISPER_CLOUD_URL() { return process.env.WHISPER_CLOUD_URL; },
                    get AI_API_KEY() { return process.env.AI_API_KEY; }
                };
            }
        })
    }
}));

import { WhisperNode } from "@services/WhisperNode";
import { safeFetch } from "../../src/utils/HttpClient";

describe("WhisperNode — Hardware-Asymmetric STT", () => {
    let whisper: WhisperNode;

    beforeEach(() => {
        vi.clearAllMocks();
        // Clear env vars
        delete process.env.WHISPER_URL;
        delete process.env.WHISPER_CLOUD_URL;
        delete process.env.AI_PROVIDER;
        delete process.env.AI_API_KEY;
        whisper = new WhisperNode();
    });

    afterEach(() => {
        whisper.destroy();
    });

    // ============================================================
    // Constructor
    // ============================================================
    describe("Constructor", () => {
        it("should create without error", () => {
            expect(whisper).toBeTruthy();
        });

        it("should be an EventEmitter", () => {
            expect(whisper).toBeInstanceOf(EventEmitter);
        });

        it("should not have circuit open initially", () => {
            expect(whisper.isCircuitOpen()).toBe(false);
        });
    });

    // ============================================================
    // pushAudioChunk()
    // ============================================================
    describe("pushAudioChunk()", () => {
        it("should accept audio data without throwing", () => {
            const chunk = Buffer.alloc(100);
            expect(() => whisper.pushAudioChunk(chunk)).not.toThrow();
        });
    });

    // ============================================================
    // pushAudioChunkOnly()
    // ============================================================
    describe("pushAudioChunkOnly()", () => {
        it("should accept audio without triggering silence timer", () => {
            const chunk = Buffer.alloc(100);
            expect(() => whisper.pushAudioChunkOnly(chunk)).not.toThrow();
        });
    });

    // ============================================================
    // triggerTranscription()
    // ============================================================
    describe("triggerTranscription()", () => {
        it("should not throw when called with empty buffer", () => {
            expect(() => whisper.triggerTranscription()).not.toThrow();
        });
    });

    // ============================================================
    // flush()
    // ============================================================
    describe("flush()", () => {
        it("should clear audio buffer", () => {
            whisper.pushAudioChunkOnly(Buffer.alloc(100));
            whisper.flush();
            // No way to verify directly, but should not throw
        });

        it("should be safe to call multiple times", () => {
            whisper.flush();
            whisper.flush();
        });
    });

    // ============================================================
    // destroy()
    // ============================================================
    describe("destroy()", () => {
        it("should clean up resources", () => {
            whisper.pushAudioChunkOnly(Buffer.alloc(100));
            expect(() => whisper.destroy()).not.toThrow();
        });

        it("should reset circuit breaker state", () => {
            whisper.destroy();
            expect(whisper.isCircuitOpen()).toBe(false);
        });
    });

    // ============================================================
    // Endpoint Resolution (integration via env vars)
    // ============================================================
    describe("Endpoint Resolution", () => {
        it("should use WHISPER_URL when set", async () => {
            process.env.WHISPER_URL = "http://custom:1234/stt";
            vi.mocked(safeFetch).mockResolvedValue({
                ok: true,
                text: async () => "Hello",
            } as any);

            // Push enough audio data (>4096 bytes = 1024 float32 samples)
            const floats = new Float32Array(2048);
            whisper.pushAudioChunkOnly(Buffer.from(floats.buffer));
            whisper.triggerTranscription();

            // Wait for async processing
            await vi.waitFor(() => {
                expect(safeFetch).toHaveBeenCalled();
            }, { timeout: 2000 });

            const callUrl = vi.mocked(safeFetch).mock.calls[0][0];
            expect(callUrl).toBe("http://custom:1234/stt");
        });

        it("should use WHISPER_CLOUD_URL when AI_PROVIDER=local", async () => {
            process.env.AI_PROVIDER = "local";
            process.env.WHISPER_CLOUD_URL = "https://api.groq.com/whisper";
            process.env.AI_API_KEY = "test-key";

            vi.mocked(safeFetch).mockResolvedValue({
                ok: true,
                text: async () => "Hello",
            } as any);

            const floats = new Float32Array(2048);
            whisper.pushAudioChunkOnly(Buffer.from(floats.buffer));
            whisper.triggerTranscription();

            await vi.waitFor(() => {
                expect(safeFetch).toHaveBeenCalled();
            }, { timeout: 2000 });

            const callUrl = vi.mocked(safeFetch).mock.calls[0][0];
            expect(callUrl).toBe("https://api.groq.com/whisper");
        });

        it("should fallback to local endpoint when no env vars set", async () => {
            vi.mocked(safeFetch).mockResolvedValue({
                ok: true,
                text: async () => "Hello",
            } as any);

            const floats = new Float32Array(2048);
            whisper.pushAudioChunkOnly(Buffer.from(floats.buffer));
            whisper.triggerTranscription();

            await vi.waitFor(() => {
                expect(safeFetch).toHaveBeenCalled();
            }, { timeout: 2000 });

            const callUrl = vi.mocked(safeFetch).mock.calls[0][0];
            expect(callUrl).toBe("http://127.0.0.1:8100/v1/audio/transcriptions");
        });
    });

    // ============================================================
    // Circuit Breaker
    // ============================================================
    describe("Circuit Breaker", () => {
        it("should emit transcription_ready on successful inference", async () => {
            vi.mocked(safeFetch).mockResolvedValue({
                ok: true,
                text: async () => "Xin chào LIVA",
            } as any);

            const spy = vi.fn();
            whisper.on("transcription_ready", spy);

            const floats = new Float32Array(2048);
            whisper.pushAudioChunkOnly(Buffer.from(floats.buffer));
            whisper.triggerTranscription();

            await vi.waitFor(() => {
                expect(spy).toHaveBeenCalledWith("Xin chào LIVA");
            }, { timeout: 2000 });
        });

        it("should skip transcription when circuit is open", async () => {
            // Trigger 3 failures to open circuit
            vi.mocked(safeFetch).mockRejectedValue(new Error("ECONNREFUSED"));

            for (let i = 0; i < 3; i++) {
                const floats = new Float32Array(2048);
                whisper.pushAudioChunkOnly(Buffer.from(floats.buffer));
                whisper.triggerTranscription();
                await new Promise(r => setTimeout(r, 50));
            }

            // Wait for all failures to process
            await vi.waitFor(() => {
                expect(whisper.isCircuitOpen()).toBe(true);
            }, { timeout: 3000 });
        });
    });
});
