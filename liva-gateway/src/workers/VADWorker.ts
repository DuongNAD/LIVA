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
import * as ort from "onnxruntime-node";
let session: ort.InferenceSession | null = null;

// Silero VAD state
let stateTensor: ort.Tensor | null = null;
const SR_TENSOR_DATA = new BigInt64Array([16000n]); // 16kHz sample rate
const VAD_FRAME_SIZE = 512; // Silero VAD expects exactly 512 samples per frame (32ms @ 16kHz)
let residualBuffer = new Float32Array(0); // Leftover samples from previous chunks

// Concurrency Control: Sequential FIFO Queue
const audioQueue: Float32Array[] = [];
let isProcessing = false;
let lastInferenceStart = 0;

async function initialize(modelPath: string): Promise<void> {
    try {
        session = await ort.InferenceSession.create(modelPath, {
            executionProviders: ["cpu"]
        });

        // Initialize hidden state tensor for Silero v4/v5 (state)
        stateTensor = new ort.Tensor("float32", new Float32Array(2 * 1 * 128).fill(0), [2, 1, 128]);

        parentPort?.postMessage({ type: "ready" });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `VAD init failed: ${msg}` });
    }
}

async function processAudio(samples: Float32Array): Promise<void> {
    if (!session || !ort || !stateTensor) return;

    lastInferenceStart = Date.now();
    try {
        const inputTensor = new ort.Tensor("float32", samples, [1, samples.length]);
        const srTensor = new ort.Tensor("int64", SR_TENSOR_DATA, []);

        const results = await session.run({
            input: inputTensor,
            sr: srTensor,
            state: stateTensor,
        });

        // Update LSTM hidden state for next frame
        // Must reconstruct tensor from data — onnxruntime-node output tensor dims
        // may not be directly reusable as input (shape mismatch)
        const stateData = results.stateN.data as Float32Array;
        stateTensor = new ort.Tensor("float32", new Float32Array(stateData), [2, 1, 128]);

        const confidence = (results.output.data as Float32Array)[0];

        parentPort?.postMessage({
            type: "vad_result",
            isSpeech: confidence > 0.5,
            confidence,
        });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `VAD inference error: ${msg}` });
    } finally {
        lastInferenceStart = 0;
    }
}

async function processQueue(): Promise<void> {
    if (isProcessing) return;
    isProcessing = true;
    try {
        while (audioQueue.length > 0) {
            const nextBuffer = audioQueue.shift();
            if (!nextBuffer) continue;

            // Combine residual with new audio
            const combined = new Float32Array(residualBuffer.length + nextBuffer.length);
            combined.set(residualBuffer, 0);
            combined.set(nextBuffer, residualBuffer.length);

            // Process in 512-sample frames (Silero VAD requirement)
            let offset = 0;
            while (offset + VAD_FRAME_SIZE <= combined.length) {
                const frame = combined.subarray(offset, offset + VAD_FRAME_SIZE);
                await processAudio(frame);
                offset += VAD_FRAME_SIZE;
            }

            // Save leftover samples for next chunk
            residualBuffer = combined.subarray(offset);
        }
    } finally {
        isProcessing = false;
    }
}

// Message handler
parentPort?.on("message", async (msg: { type: string; modelPath?: string; buffer?: Float32Array }) => {
    switch (msg.type) {
        case "init":
            await initialize(msg.modelPath!);
            break;
        case "audio":
            if (msg.buffer) {
                audioQueue.push(msg.buffer);
                await processQueue();
            }
            break;
        case "ping":
            // v25 Watchdog Heartbeat — respond immediately to prove worker is alive
            if (lastInferenceStart > 0 && Date.now() - lastInferenceStart > 5000) {
                console.error(`[VADWorker] Native hang detected! Inference running for ${Date.now() - lastInferenceStart}ms.`);
                // Withhold pong to trigger watchdog recovery
            } else {
                parentPort?.postMessage({ type: "pong" });
            }
            break;
        case "dispose":
            session = null;
            stateTensor = null;
            audioQueue.length = 0;
            process.exit(0);
            break;
    }
});
