import { describe, it, expect, afterEach } from "vitest";
import * as path from "node:path";
import { NemotronSTTService } from "../../src/services/NemotronSTTService";

describe("NemotronSTTService Integration Test", () => {
    let service: NemotronSTTService | null = null;

    afterEach(() => {
        if (service) {
            service.destroy();
            service = null;
        }
    });

    it("should initialize real ONNX sessions and process audio chunks without dimension mismatch or crash", async () => {
        // Resolve the models directory explicitly
        const modelDir = path.resolve(process.cwd(), "models/nemotron-asr");
        process.env.NEMOTRON_MODEL_DIR = modelDir;
        process.env.NEMOTRON_LANGUAGE = "vi";

        service = new NemotronSTTService();

        // 1. Initialize the service (loads the real ONNX models)
        console.log("Initializing NemotronSTTService (loading ONNX models)...");
        await service.initialize();
        console.log("NemotronSTTService initialized successfully.");

        // 2. Prepare mock audio: 10,640 samples of Float32 silence
        // 10,640 samples * 4 bytes/sample = 42,560 bytes
        const audioBuffer = Buffer.alloc(42560);

        // We listen for events
        let partialCount = 0;
        let readyText: string | null = null;
        let gotError = false;
        let errorMessage = "";

        service.on("transcription_partial", (text) => {
            console.log(`Received partial transcription: "${text}"`);
            partialCount++;
        });

        service.on("transcription_ready", (text) => {
            console.log(`Received final transcription: "${text}"`);
            readyText = text;
        });

        // We also want to intercept worker error events if any
        // Since error isn't public, it is logged, but we can also check circuit breaker or add mock handler
        // Let's hook into the worker to verify no error message is sent.
        // But since service.destroy() cleans up, we can just push audio and wait.
        
        console.log("Pushing 10,640 samples of audio...");
        service.pushAudioChunkOnly(audioBuffer);

        // Wait a bit for the worker to process the chunk
        await new Promise((resolve) => setTimeout(resolve, 1500));

        console.log("Triggering final transcription...");
        service.triggerTranscription();

        // Wait for finalization
        await new Promise((resolve) => setTimeout(resolve, 1500));

        // Since it is silence, the transcription might be empty (which is skipped from emission by service)
        // or it might emit empty string or no transcription_ready.
        // Let's verify that the service is still healthy (circuit breaker is not open)
        expect(service.isCircuitOpen()).toBe(false);
        console.log("Integration test completed successfully without crashes or circuit breaker activation.");
    }, 60000); // 60s timeout for model loading + inference
});
