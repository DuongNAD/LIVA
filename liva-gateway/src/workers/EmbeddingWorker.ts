/**
 * EmbeddingWorker — CPU-only ONNX embedding inference in Worker Thread
 * ====================================================================
 * [EXCEPTION] @huggingface/transformers is BANNED on main thread (AI_CONTEXT §3)
 * because Tensor CPU ops block the Event Loop. However, this file runs inside
 * `node:worker_threads` — it has its OWN Event Loop, isolated from Gateway.
 * We import ONLY the tokenizer (WordPiece) from HF transformers, NOT the inference engine.
 * Actual model inference is done via `onnxruntime-node` (also CPU, also worker-isolated).
 */
import { parentPort } from "node:worker_threads";
import * as ort from "onnxruntime-node";
// eslint-disable-next-line no-restricted-imports
import { pipeline } from "@huggingface/transformers"; // [EXCEPTION] Tokenizer only — see header comment
import * as path from "node:path";
import * as fs from "node:fs";

let session: ort.InferenceSession | null = null;
let tokenizer: any = null;
let useGpu = true;

function resolveModelPath(): string {
    const cwdPath = path.join(process.cwd(), "models", "all-MiniLM-L6-v2.onnx");
    if (fs.existsSync(cwdPath)) return cwdPath;

    // Check HuggingFace cache folder
    const cachePath = path.join(
        process.cwd(),
        "node_modules",
        "@huggingface",
        "transformers",
        ".cache",
        "Xenova",
        "all-MiniLM-L6-v2",
        "onnx",
        "model.onnx"
    );
    if (fs.existsSync(cachePath)) return cachePath;

    // Direct check in nested node_modules or parent directories
    const altCachePath = path.join(
        process.cwd(),
        "..",
        "node_modules",
        "@huggingface",
        "transformers",
        ".cache",
        "Xenova",
        "all-MiniLM-L6-v2",
        "onnx",
        "model.onnx"
    );
    if (fs.existsSync(altCachePath)) return altCachePath;

    throw new Error("Could not locate all-MiniLM-L6-v2.onnx model file.");
}

async function loadModel(useGpuValue: boolean) {
    if (session) {
        try {
            await session.release();
        } catch {
            // Ignore release errors
        }
        session = null;
    }
    useGpu = useGpuValue;
    const modelPath = resolveModelPath();
    const providers = useGpu ? ["cuda", "directml", "cpu"] : ["cpu"];
    
    try {
        session = await ort.InferenceSession.create(modelPath, {
            executionProviders: providers
        });
    } catch (e: any) {
        if (useGpu) {
            // Fallback to CPU
            session = await ort.InferenceSession.create(modelPath, {
                executionProviders: ["cpu"]
            });
            useGpu = false;
        } else {
            throw e;
        }
    }
}

async function computeEmbedding(text: string): Promise<number[]> {
    if (!tokenizer || !session) {
        throw new Error("Model or tokenizer not initialized.");
    }
    const tokens = await tokenizer(text);
    
    const feeds = {
        input_ids: new ort.Tensor("int64", BigInt64Array.from(tokens.input_ids.data.map(BigInt)), tokens.input_ids.dims),
        attention_mask: new ort.Tensor("int64", BigInt64Array.from(tokens.attention_mask.data.map(BigInt)), tokens.attention_mask.dims),
        token_type_ids: new ort.Tensor("int64", new BigInt64Array(tokens.input_ids.data.length).fill(0n), tokens.input_ids.dims)
    };
    
    const outputs = await session.run(feeds);
    const lastHiddenState = outputs.last_hidden_state;
    
    const [batchSize, seqLength, dim] = lastHiddenState.dims;
    const data = lastHiddenState.data as Float32Array;
    const mask = tokens.attention_mask.data;
    
    const pooled = new Float32Array(dim);
    let validTokensCount = 0;
    
    for (let s = 0; s < seqLength; s++) {
        if (Number(mask[s]) === 1) {
            validTokensCount++;
            for (let d = 0; d < dim; d++) {
                pooled[d] += data[s * dim + d];
            }
        }
    }
    
    if (validTokensCount > 0) {
        for (let d = 0; d < dim; d++) {
            pooled[d] /= validTokensCount;
        }
    }
    
    // L2 Normalize
    let norm = 0;
    for (let d = 0; d < dim; d++) {
        norm += pooled[d] * pooled[d];
    }
    norm = Math.sqrt(norm);
    
    const normalized = new Array(dim);
    for (let d = 0; d < dim; d++) {
        normalized[d] = norm > 0 ? pooled[d] / norm : 0;
    }
    
    return normalized;
}

async function processEmbedBatch(id: string, texts: string[]) {
    try {
        const vectors = [];
        for (const text of texts) {
            vectors.push(await computeEmbedding(text));
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
parentPort?.on("message", async (msg: { type: string; id?: string; text?: string; texts?: string[]; useGpu?: boolean }) => {
    switch (msg.type) {
        case "init":
            try {
                const extractor = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2");
                tokenizer = extractor.tokenizer;
                await loadModel(true); // default GPU = true
                parentPort?.postMessage({ type: "ready" });
            } catch (err: unknown) {
                const msgErr = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", message: `Embedding worker init failed: ${msgErr}` });
            }
            break;
        case "configure":
            try {
                await loadModel(msg.useGpu !== false);
            } catch (err: unknown) {
                const msgErr = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", message: `Embedding worker configure failed: ${msgErr}` });
            }
            break;
        case "embed":
            try {
                const vector = await computeEmbedding(msg.text!);
                parentPort?.postMessage({
                    type: "embed_result",
                    id: msg.id!,
                    vector
                });
            } catch (err: unknown) {
                const msgErr = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", id: msg.id!, message: `Embed inference error: ${msgErr}` });
            }
            break;
        case "embed_batch":
            await processEmbedBatch(msg.id!, msg.texts!);
            break;
        case "ping":
            parentPort?.postMessage({ type: "pong" });
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
            tokenizer = null;
            process.exit(0);
            break;
    }
});
