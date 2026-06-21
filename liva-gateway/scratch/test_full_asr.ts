import * as dotenv from "dotenv";
dotenv.config();

import { NemotronSTTService } from "../src/services/NemotronSTTService";
import { logger } from "../src/utils/logger";

async function main() {
    logger.info("Starting ASR test...");
    const stt = new NemotronSTTService();
    
    stt.on("transcription_partial", (text) => {
        logger.info(`[Test] Partial transcription received: "${text}"`);
    });
    
    stt.on("transcription_ready", (text) => {
        logger.info(`[Test] Final transcription received: "${text}"`);
    });
    
    stt.on("stt_fallback_activated", () => {
        logger.warn("[Test] Fallback activated!");
    });

    try {
        logger.info("Initializing NemotronSTTService...");
        await stt.initialize();
        logger.info("NemotronSTTService initialized successfully!");
        
        // Let's send audio chunks. We need at least 10640 samples to run the first inference.
        // Let's send 12000 samples of Float32 audio (zeros) in 6 chunks of 2000 samples.
        const chunkSize = 2000;
        const numChunks = 8;
        
        for (let i = 0; i < numChunks; i++) {
            // Create a float32 array
            const arr = new Float32Array(chunkSize);
            // Fill with small random values to simulate noise/speech
            for (let j = 0; j < chunkSize; j++) {
                arr[j] = (Math.random() - 0.5) * 0.01;
            }
            
            // Convert to Buffer containing Float32 bytes (chunkSize * 4 bytes)
            const buffer = Buffer.from(arr.buffer, arr.byteOffset, arr.byteLength);
            
            logger.info(`Sending chunk ${i + 1}/${numChunks} (${buffer.byteLength} bytes)...`);
            stt.pushAudioChunk(buffer);
            
            // Wait 128ms to simulate real-time streaming
            await new Promise(r => setTimeout(r, 128));
        }
        
        // Wait for silence timeout (VAD_SILENCE_MS is 2000ms by default)
        logger.info("Waiting for silence timeout...");
        await new Promise(r => setTimeout(r, 3000));
        
        logger.info("ASR test finished.");
        stt.destroy();
    } catch (e: any) {
        logger.error(`[Test] Initialization or run failed: ${e.message}`);
        stt.destroy();
    }
}

main().catch(console.error);
