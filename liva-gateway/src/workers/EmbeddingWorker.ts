import { parentPort } from "node:worker_threads";
/* eslint-disable no-restricted-imports */
import { pipeline, env } from "@huggingface/transformers";

// Disable local models loading by default, rely on huggingface cache
if (env.backends.onnx.wasm) {
    env.backends.onnx.wasm.numThreads = 1;
}

let extractor: any = null;

async function initialize() {
    try {
        // Initialize the feature extraction pipeline
        extractor = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2", {
            dtype: "fp32", // Full precision for node
        });
        parentPort?.postMessage({ type: "ready" });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `Embedding init failed: ${msg}` });
    }
}

async function processEmbed(id: string, text: string) {
    if (!extractor) return;
    try {
        const output = await extractor(text, { pooling: "mean", normalize: true });
        const vector = Array.from(output.data) as number[];
        parentPort?.postMessage({
            type: "embed_result",
            id,
            vector
        });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", id, message: `Embed inference error: ${msg}` });
    }
}

async function processEmbedBatch(id: string, texts: string[]) {
    if (!extractor) return;
    try {
        const output = await extractor(texts, { pooling: "mean", normalize: true });
        const dim = 384;
        const vectors: number[][] = [];
        const flatData = Array.from(output.data) as number[];
        for (let i = 0; i < texts.length; i++) {
            vectors.push(flatData.slice(i * dim, (i + 1) * dim));
        }
        parentPort?.postMessage({
            type: "embed_batch_result",
            id,
            vectors
        });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", id, message: `Embed batch error: ${msg}` });
    }
}

// Message handler
parentPort?.on("message", async (msg: { type: string; id?: string; text?: string; texts?: string[] }) => {
    switch (msg.type) {
        case "init":
            await initialize();
            break;
        case "embed":
            await processEmbed(msg.id!, msg.text!);
            break;
        case "embed_batch":
            await processEmbedBatch(msg.id!, msg.texts!);
            break;
        case "ping":
            parentPort?.postMessage({ type: "pong" });
            break;
        case "dispose":
            extractor = null;
            process.exit(0);
            break;
    }
});
