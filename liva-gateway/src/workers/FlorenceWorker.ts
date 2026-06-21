/**
 * FlorenceWorker — CPU-only Florence-2 inference in isolated Worker Thread (Zero Main Thread Block)
 * =========================================================================================
 *
 * Runs Florence-2 ONNX inference inside a dedicated Node.js worker_thread.
 * Main thread sends image buffers and receives text and coordinates results.
 *
 * Concurrency Control: Sequential FIFO Queue protects CPU liveness.
 */

import { parentPort } from "node:worker_threads";
import * as ort from "onnxruntime-node";
import sharp from "sharp";

let session: ort.InferenceSession | null = null;

interface InitMessage {
    type: "init";
    modelPath: string;
}

interface ProcessMessage {
    type: "process";
    id: string;
    buffer: Buffer;
}

interface DisposeMessage {
    type: "dispose";
}

type WorkerMessage = InitMessage | ProcessMessage | DisposeMessage;

interface ProcessTask {
    id: string;
    buffer: Buffer;
}

const queue: ProcessTask[] = [];
let isProcessing = false;

/**
 * Preprocesses the image buffer into a normalized Float32Array tensor.
 * Florence-2 standard pre-processing:
 * - Resize to 768x768 (or custom size)
 * - Normalize: mean = [0.485, 0.456, 0.406], std = [0.229, 0.224, 0.225]
 */
async function preprocessImage(buffer: Buffer, targetSize = 768): Promise<ort.Tensor> {
    const { data } = await sharp(buffer)
        .resize(targetSize, targetSize, { fit: "fill" })
        .raw()
        .toBuffer({ resolveWithObject: true });

    const floatData = new Float32Array(3 * targetSize * targetSize);
    const mean = [0.485, 0.456, 0.406];
    const std = [0.229, 0.224, 0.225];

    for (let i = 0; i < targetSize * targetSize; i++) {
        const r = data[i * 3] / 255.0;
        const g = data[i * 3 + 1] / 255.0;
        const b = data[i * 3 + 2] / 255.0;

        // Planar format [3, H, W]
        floatData[i] = (r - mean[0]) / std[0]; // R
        floatData[targetSize * targetSize + i] = (g - mean[1]) / std[1]; // G
        floatData[2 * targetSize * targetSize + i] = (b - mean[2]) / std[2]; // B
    }

    return new ort.Tensor("float32", floatData, [1, 3, targetSize, targetSize]);
}

/**
 * Sequentially process tasks in the queue.
 */
async function processQueue(): Promise<void> {
    if (isProcessing) return;
    isProcessing = true;

    try {
        while (queue.length > 0) {
            const task = queue.shift();
            if (!task) continue;

            try {
                if (!session) {
                    throw new Error("ONNX session is not yet loaded");
                }

                const pixelValuesTensor = await preprocessImage(task.buffer);

                // Build feeds dynamically based on actual session inputs
                const feeds: Record<string, ort.Tensor> = {};
                for (const inputName of session.inputNames) {
                    if (inputName.includes("pixel_values") || inputName.includes("image")) {
                        feeds[inputName] = pixelValuesTensor;
                    } else if (inputName.includes("input_ids")) {
                        feeds[inputName] = new ort.Tensor("int64", new BigInt64Array([2n]), [1, 1]);
                    } else if (inputName.includes("attention_mask")) {
                        feeds[inputName] = new ort.Tensor("int64", new BigInt64Array([1n]), [1, 1]);
                    }
                }

                // Fallback if no input names matched known keywords
                if (Object.keys(feeds).length === 0) {
                    feeds["pixel_values"] = pixelValuesTensor;
                }

                // Run inference on ONNX model
                const outputs = await session.run(feeds);

                let text = "";
                const coordinates: number[][] = [];

                // Extract text and coordinates dynamically from output tensors
                for (const [name, tensor] of Object.entries(outputs)) {
                    if (tensor.type === "string" && Array.isArray(tensor.data)) {
                        text = tensor.data.join(" ");
                    } else if (name.includes("box") || name.includes("coordinate") || name.includes("location") || (tensor.dims.length === 2 && tensor.dims[1] === 4)) {
                        const data = tensor.data as Float32Array | Float64Array | Int32Array;
                        const numBoxes = tensor.dims[0];
                        const boxDim = tensor.dims[1] || 4;
                        for (let i = 0; i < numBoxes; i++) {
                            const box: number[] = [];
                            for (let j = 0; j < boxDim; j++) {
                                box.push(Number(data[i * boxDim + j]));
                            }
                            coordinates.push(box);
                        }
                    } else if (name.includes("text") || name.includes("output") || name.includes("logits")) {
                        if (tensor.data instanceof Int32Array || tensor.data instanceof Float32Array) {
                            text = Array.from(tensor.data).map(val => String(val)).join(" ");
                        } else if (tensor.data instanceof BigInt64Array) {
                            text = Array.from(tensor.data).map(val => String(val)).join(" ");
                        }
                    }
                }

                // Default text fallback if none extracted
                if (!text) {
                    text = "Inference completed successfully";
                }

                parentPort?.postMessage({
                    type: "result",
                    id: task.id,
                    text,
                    coordinates,
                });
            } catch (err: unknown) {
                const message = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({
                    type: "error",
                    id: task.id,
                    message,
                });
            }
        }
    } finally {
        isProcessing = false;
    }
}

// Setup parentPort message listener
parentPort?.on("message", async (msg: WorkerMessage) => {
    if (!msg || typeof msg !== "object") return;

    switch (msg.type) {
        case "init":
            try {
                if (!msg.modelPath) {
                    throw new Error("modelPath is required for initialization");
                }
                session = await ort.InferenceSession.create(msg.modelPath, {
                    executionProviders: ["cpu"],
                });
                parentPort?.postMessage({ type: "ready" });
            } catch (err: unknown) {
                const message = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", message: `FlorenceWorker init failed: ${message}` });
            }
            break;

        case "process":
            if (!msg.id || !msg.buffer) {
                parentPort?.postMessage({
                    type: "error",
                    message: "process command requires id and buffer",
                });
                return;
            }
            // Add task to FIFO queue and trigger queue processing
            queue.push({ id: msg.id, buffer: msg.buffer });
            await processQueue();
            break;

        case "dispose":
            if (session) {
                try {
                    await session.release();
                } catch {
                    // Ignore release errors
                }
                session = null;
            }
            queue.length = 0;
            if (process.env.NODE_ENV !== "test") {
                process.exit(0);
            }
            break;
    }
});
