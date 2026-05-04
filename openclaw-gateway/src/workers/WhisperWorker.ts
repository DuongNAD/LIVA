import { parentPort } from 'node:worker_threads';
// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-ignore
import { pipeline } from '@huggingface/transformers';

const MODEL_ID = "onnx-community/whisper-base";
let sttPipeline: any = null;
let isReady = false;

async function init() {
    try {
        sttPipeline = await pipeline("automatic-speech-recognition", MODEL_ID, {
            dtype: "q8",
            device: "cpu",
        });
        isReady = true;
        parentPort?.postMessage({ type: "ready" });
    } catch (e: any) {
        parentPort?.postMessage({ type: "error", message: e.message });
    }
}

function encodeWAV(samples: Float32Array, sampleRate: number): ArrayBuffer {
    const wavBuffer = new ArrayBuffer(44 + samples.length * 2);
    const view = new DataView(wavBuffer);
    const writeString = (str: string, offset: number) => {
        for (let i = 0; i < str.length; i++) {
            view.setUint8(offset + i, str.charCodeAt(i));
        }
    };

    writeString("RIFF", 0);
    view.setUint32(4, 36 + samples.length * 2, true);
    writeString("WAVE", 8);
    writeString("fmt ", 12);
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true); // PCM
    view.setUint16(22, 1, true); // Mono
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    writeString("data", 36);
    view.setUint32(40, samples.length * 2, true);

    let offset = 44;
    for (let i = 0; i < samples.length; i++) {
        const s = Math.max(-1, Math.min(1, samples[i]));
        view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7FFF, true);
        offset += 2;
    }

    return wavBuffer;
}

parentPort?.on("message", async (msg) => {
    if (msg.type === "init") {
        await init();
    } else if (msg.type === "process") {
        if (!isReady || !sttPipeline) return;

        // msg.buffer is an ArrayBuffer (Zero-Copy)
        const float32Arr = new Float32Array(msg.buffer);
        
        try {
            const wavArrayBuffer = encodeWAV(float32Arr, 16000);
            
            const result = await sttPipeline(
                new Blob([new Uint8Array(wavArrayBuffer)], { type: "audio/wav" }),
                { language: "vi", task: "transcribe" }
            );

            const text = (result?.text || "").trim();
            if (text && text.length > 0) {
                parentPort?.postMessage({ type: "transcription", text });
            }
        } catch (e: any) {
            parentPort?.postMessage({ type: "error", message: e.message });
        }
    }
});
