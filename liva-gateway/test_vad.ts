import { VADWorkerBridge } from './src/services/VADWorkerBridge';
import * as path from 'node:path';

async function testVAD() {
    const vad = new VADWorkerBridge();
    const modelPath = path.join(process.cwd(), "models", "nemotron-asr", "silero_vad.onnx");
    
    vad.on('ready', () => console.log('VAD is ready!'));
    vad.on('speech_start', () => console.log('Speech Started'));
    vad.on('speech_end', () => console.log('Speech Ended'));
    vad.on('error', (e) => console.error('VAD Error:', e));

    try {
        console.log("Init VAD...");
        await vad.initialize(modelPath);
        console.log("Init returned.");
        
        // Feed zeros (silence)
        const zeros = new Float32Array(512); // 32ms
        for(let i=0; i<100; i++) {
            vad.pushAudioSamples(zeros);
            await new Promise(r => setTimeout(r, 10));
        }
        
        console.log("Done.");
        process.exit(0);
    } catch(e) {
        console.error("Failed:", e);
        process.exit(1);
    }
}

testVAD();
