import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { MoonshineSTTService } from "@services/MoonshineSTTService";

// Mock logger to avoid test output pollution
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

// Mock onnxruntime-node in the test so that it runs successfully without real model files.
vi.mock("onnxruntime-node", () => {
    return {
        InferenceSession: {
            create: vi.fn().mockResolvedValue({
                run: vi.fn().mockResolvedValue({}),
                release: vi.fn().mockResolvedValue(undefined),
            }),
        },
        Tensor: class MockTensor {
            type: string;
            data: any;
            dims: any;
            constructor(type: string, data: any, dims: any) {
                this.type = type;
                this.data = data;
                this.dims = dims;
            }
        },
    };
});

describe("MoonshineSTTService Integration Tests", () => {
    let service: MoonshineSTTService;

    beforeEach(() => {
        service = new MoonshineSTTService();
    });

    afterEach(() => {
        if (service) {
            service.dispose();
        }
    });

    it("should initialize successfully", async () => {
        await expect(service.initialize()).resolves.toBeUndefined();
    });

    it("should process dynamic length audio chunks and return partial/final transcriptions", async () => {
        await service.initialize();

        service.startStreaming();

        // Generate 16000 samples (1s of 16kHz mono audio) of a sine wave (amplitude > 0.002 to trigger fallback speech detection)
        const numSamples = 16000;
        const floatArray = new Float32Array(numSamples);
        for (let i = 0; i < numSamples; i++) {
            floatArray[i] = Math.sin(2 * Math.PI * 440 * i / 16000) * 0.1;
        }

        const audioBuffer = Buffer.from(floatArray.buffer);

        // Listen for the transcription_partial event
        const partialPromise = new Promise<string>((resolve) => {
            service.once("transcription_partial", (text) => {
                resolve(text);
            });
        });

        // Push the dynamic length audio chunk
        service.pushAudioChunkOnly(audioBuffer);

        // Wait for the worker to process and emit a partial transcription
        const partialText = await partialPromise;
        expect(partialText).toContain("hello");

        // Stop streaming and wait for the final transcription
        const finalPromise = new Promise<string>((resolve) => {
            service.once("transcription_ready", (text) => {
                resolve(text);
            });
        });

        const stopPromise = service.stopStreaming();

        const [finalText, stopText] = await Promise.all([finalPromise, stopPromise]);

        expect(finalText).toContain("hello");
        expect(stopText).toBe(finalText);
    });

    it("should handle empty or silent chunks without crashing", async () => {
        await service.initialize();
        service.startStreaming();

        // Push an empty buffer
        service.pushAudioChunkOnly(Buffer.alloc(0));

        // Push a silent buffer (amplitude = 0)
        const silentFloat = new Float32Array(4000);
        const silentBuffer = Buffer.from(silentFloat.buffer);
        service.pushAudioChunkOnly(silentBuffer);

        const finalText = await service.stopStreaming();
        expect(finalText).toBeDefined();
    });
});
