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
import { Tokenizer } from "@huggingface/tokenizers";
import * as path from "node:path";
import * as fs from "node:fs";

let session: ort.InferenceSession | null = null;
let tokenizer: Tokenizer | null = null;
let useGpu = true;

function resolveTokenizerPaths(modelName: string): { jsonPath: string; configPath: string } {
    const pathsToTry = [
        path.join(process.cwd(), "models", modelName),
        path.join(process.cwd(), "liva-gateway", "models", modelName),
        path.join(process.cwd(), "node_modules", "@huggingface", "transformers", ".cache", "Xenova", modelName),
        path.join(process.cwd(), "..", "node_modules", "@huggingface", "transformers", ".cache", "Xenova", modelName)
    ];

    for (const dir of pathsToTry) {
        const jsonPath = path.join(dir, "tokenizer.json");
        const configPath = path.join(dir, "tokenizer_config.json");
        if (fs.existsSync(jsonPath) && fs.existsSync(configPath)) {
            return { jsonPath, configPath };
        }
    }

    throw new Error(`Could not locate tokenizer files for ${modelName}.`);
}

function resolveModelPath(modelName: string): string {
    const localOnnxPath = path.join(process.cwd(), "models", modelName, "onnx", "model.onnx");
    if (fs.existsSync(localOnnxPath)) return localOnnxPath;

    const rootLocalOnnxPath = path.join(process.cwd(), "liva-gateway", "models", modelName, "onnx", "model.onnx");
    if (fs.existsSync(rootLocalOnnxPath)) return rootLocalOnnxPath;

    const filename = modelName === "multilingual-e5-small" ? "multilingual-e5-small.onnx" : `${modelName}.onnx`;
    const cwdPath = path.join(process.cwd(), "models", filename);
    if (fs.existsSync(cwdPath)) return cwdPath;

    const rootCwdPath = path.join(process.cwd(), "liva-gateway", "models", filename);
    if (fs.existsSync(rootCwdPath)) return rootCwdPath;

    // Check HuggingFace cache folder
    const cachePath = path.join(
        process.cwd(),
        "node_modules",
        "@huggingface",
        "transformers",
        ".cache",
        "Xenova",
        modelName,
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
        modelName,
        "onnx",
        "model.onnx"
    );
    if (fs.existsSync(altCachePath)) return altCachePath;

    throw new Error(`Could not locate ${modelName} model file.`);
}

async function loadModel(modelName: string, useGpuValue: boolean) {
    if (session) {
        try {
            await session.release();
        } catch {
            // Ignore release errors
        }
        session = null;
    }
    useGpu = useGpuValue;
    const modelPath = resolveModelPath(modelName);
    const isDarwin = process.platform === "darwin";
    const providers = useGpu
        ? (isDarwin
            ? ["cpu"] // CPU is faster and thread-safe for MiniLM on macOS
            : ["cuda", "directml", "cpu"])
        : ["cpu"];
    
    try {
        session = await ort.InferenceSession.create(modelPath, {
            executionProviders: providers
        });
    } catch (e: unknown) {
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
    const tokens = tokenizer.encode(text);
    const idsLength = Math.min(tokens.ids.length, 512);
    const ids = tokens.ids.slice(0, idsLength);
    const attentionMask = tokens.attention_mask.slice(0, idsLength);
    
    const feeds = {
        input_ids: new ort.Tensor("int64", BigInt64Array.from(ids.map(BigInt)), [1, idsLength]),
        attention_mask: new ort.Tensor("int64", BigInt64Array.from(attentionMask.map(BigInt)), [1, idsLength]),
        token_type_ids: new ort.Tensor("int64", new BigInt64Array(idsLength).fill(0n), [1, idsLength])
    };
    
    const outputs = await session.run(feeds);
    const lastHiddenState = outputs.last_hidden_state;
    
    const [, seqLength, dim] = lastHiddenState.dims;
    const data = lastHiddenState.data as Float32Array;
    const mask = attentionMask;
    
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

let activeModelName = "all-MiniLM-L6-v2";

// Message handler
parentPort?.on("message", async (msg: { type: string; id?: string; text?: string; texts?: string[]; useGpu?: boolean; useMultilingual?: boolean }) => {
    switch (msg.type) {
        case "init":
            try {
                activeModelName = msg.useMultilingual ? "multilingual-e5-small" : "all-MiniLM-L6-v2";
                const { jsonPath, configPath } = resolveTokenizerPaths(activeModelName);
                const tokenizerJson = JSON.parse(fs.readFileSync(jsonPath, "utf8"));
                const tokenizerConfig = JSON.parse(fs.readFileSync(configPath, "utf8"));
                
                tokenizer = new Tokenizer(tokenizerJson, tokenizerConfig);
                await loadModel(activeModelName, false); // default GPU = false (enforce CPU-only)
                parentPort?.postMessage({ type: "ready" });
            } catch (err: unknown) {
                const msgErr = err instanceof Error ? err.message : String(err);
                parentPort?.postMessage({ type: "error", message: `Embedding worker init failed: ${msgErr}` });
            }
            break;
        case "configure":
            try {
                await loadModel(activeModelName, msg.useGpu !== false);
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
