/**
 * VADWorker — Neural VAD in isolated Worker Thread (Zero Main Thread Block)
 * =========================================================================
 * [v22 Full-Duplex Pillar 1]
 *
 * Runs Silero ONNX inference inside a dedicated Node.js worker_thread.
 * Main thread only sends raw Float32Array via MessagePort (zero-copy transfer).
 * Worker responds with SPEECH_START / SPEECH_END events.
 *
 * WHY WORKER?
 * - Silero inference takes ~10-15ms per 30ms audio chunk
 * - Running this on Main Thread blocks Event Loop 30+ times/second
 * - This violates AI_CONTEXT CRITICAL_DIRECTIVE 4.0: >10ms CPU = Worker
 *
 * PROTOCOL (parentPort messages):
 * Parent → Worker:  { type: "init", modelPath: string }
 * Parent → Worker:  { type: "audio", buffer: Float32Array }  (transferable)
 * Parent → Worker:  { type: "dispose" }
 * Worker → Parent:  { type: "ready" }
 * Worker → Parent:  { type: "vad_result", isSpeech: boolean, confidence: number }
 * Worker → Parent:  { type: "error", message: string }
 */

import { parentPort } from "node:worker_threads";

// Lazily loaded onnxruntime-web
let ort: typeof import("onnxruntime-web") | null = null;
let session: any = null;

// Silero VAD state
let h: any = null;
let c: any = null;
const SR_TENSOR_DATA = new BigInt64Array([16000n]); // 16kHz sample rate

async function initialize(modelPath: string): Promise<void> {
    try {
        ort = await import("onnxruntime-web");
        ort.env.wasm.numThreads = 1;

        session = await ort.InferenceSession.create(modelPath, {
            executionProviders: ["wasm"],
        });

        // Initialize hidden state tensors (Silero VAD is stateful LSTM)
        h = new ort.Tensor("float32", new Float32Array(2 * 64).fill(0), [2, 1, 64]);
        c = new ort.Tensor("float32", new Float32Array(2 * 64).fill(0), [2, 1, 64]);

        parentPort?.postMessage({ type: "ready" });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `VAD init failed: ${msg}` });
    }
}

async function processAudio(samples: Float32Array): Promise<void> {
    if (!session || !ort || !h || !c) return;

    try {
        const inputTensor = new ort.Tensor("float32", samples, [1, samples.length]);
        const srTensor = new ort.Tensor("int64", SR_TENSOR_DATA, []);

        const results = await session.run({
            input: inputTensor,
            sr: srTensor,
            h: h,
            c: c,
        });

        // Update LSTM hidden states for next frame
        h = results.hn;
        c = results.cn;

        const confidence = (results.output.data as Float32Array)[0];

        parentPort?.postMessage({
            type: "vad_result",
            isSpeech: confidence > 0.5,
            confidence,
        });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `VAD inference error: ${msg}` });
    }
}

// Message handler
parentPort?.on("message", async (msg: { type: string; modelPath?: string; buffer?: Float32Array }) => {
    switch (msg.type) {
        case "init":
            await initialize(msg.modelPath!);
            break;
        case "audio":
            await processAudio(msg.buffer!);
            break;
        case "ping":
            // v25 Watchdog Heartbeat — respond immediately to prove worker is alive
            parentPort?.postMessage({ type: "pong" });
            break;
        case "dispose":
            session = null;
            h = null;
            c = null;
            ort = null;
            process.exit(0);
            break;
    }
});
