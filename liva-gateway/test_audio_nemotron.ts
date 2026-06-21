import { NemotronSTTService } from './src/services/NemotronSTTService';

async function testAudio() {
    const service = new NemotronSTTService();
    try {
        console.log("Initializing Nemotron STT...");
        await service.initialize();
        console.log("Initialization successful.");

        service.on('transcription_partial', (text) => console.log("Partial:", text));
        service.on('transcription_ready', (text) => console.log("Final:", text));
        service.on('stt_fallback_activated', () => console.log("Fallback activated"));

        // Generate 2 seconds of 400Hz sine wave at 16000Hz (Float32)
        const sampleRate = 16000;
        const durationSec = 2;
        const numSamples = sampleRate * durationSec;
        const float32 = new Float32Array(numSamples);
        for (let i = 0; i < numSamples; i++) {
            float32[i] = Math.sin((i * 400 * Math.PI * 2) / sampleRate);
        }

        // Send in chunks of 2048 samples (128ms)
        const chunkSize = 2048;
        for (let i = 0; i < numSamples; i += chunkSize) {
            const chunkSamples = float32.subarray(i, Math.min(i + chunkSize, numSamples));
            const buffer = Buffer.from(chunkSamples.buffer, chunkSamples.byteOffset, chunkSamples.byteLength);
            service.pushAudioChunkOnly(buffer);
            await new Promise(r => setTimeout(r, 50)); // simulate real-time
        }

        console.log("Triggering final transcription...");
        service.triggerTranscription();

        // Wait a bit for inference
        await new Promise(r => setTimeout(r, 2000));
        process.exit(0);

    } catch (e) {
        console.error("Failed:", e);
        process.exit(1);
    }
}

testAudio();
