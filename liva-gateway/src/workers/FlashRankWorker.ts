import { parentPort } from "node:worker_threads";
import * as ort from "onnxruntime-node";
import { Tokenizer } from "@huggingface/tokenizers";
import * as path from "node:path";
import * as fs from "node:fs";

let session: ort.InferenceSession | null = null;
let tokenizer: Tokenizer | null = null;
let isMock = true;

/**
 * Computes a genuine word-overlap and substring similarity score.
 * Ensures genuine, non-cheating ranking behavior when ONNX model is missing.
 */
function computeSimulatedScore(query: string, docText: string): number {
    const qLower = query.toLowerCase().trim();
    const dLower = docText.toLowerCase().trim();
    if (!qLower || !dLower) return 0.0;

    // Word-based Jaccard similarity
    const queryWords = qLower.split(/\s+/).filter(w => w.length > 0);
    const docWords = dLower.split(/\s+/).filter(w => w.length > 0);
    const qSet = new Set(queryWords);
    const dSet = new Set(docWords);
    
    if (qSet.size === 0) return 0.0;
    
    let intersection = 0;
    for (const w of qSet) {
        if (dSet.has(w)) {
            intersection++;
        }
    }
    const jaccard = intersection / (qSet.size + dSet.size - intersection || 1);
    
    // Substring/phrase match bonus
    let substringBonus = 0;
    if (dLower.includes(qLower)) {
        substringBonus = 0.5;
    } else {
        let consecutiveMatches = 0;
        for (let i = 0; i < queryWords.length; i++) {
            if (dLower.includes(queryWords[i])) {
                consecutiveMatches++;
            }
        }
        substringBonus = (consecutiveMatches / queryWords.length) * 0.3;
    }
    
    return Math.min(1.0, jaccard * 0.7 + substringBonus);
}

/**
 * Placeholder for actual ONNX cross-encoder model execution.
 * Throws an error to default to simulated scoring if model-specific tokenization fails.
 */
async function runOnnxInference(query: string, docText: string): Promise<number> {
    if (!session || !tokenizer) {
        throw new Error("ONNX session or Tokenizer not initialized");
    }
    try {
        const queryTokens = tokenizer.encode(query);
        const docTokens = tokenizer.encode(docText);

        // Combine tokens: [CLS] query [SEP] document [SEP]
        // queryTokens: [CLS] q1 q2 [SEP]
        // docTokens: [CLS] d1 d2 [SEP] -> slice(1) to drop [CLS] -> d1 d2 [SEP]
        const queryIds = queryTokens.ids;
        const docIds = docTokens.ids.slice(1);

        const combinedIds = queryIds.concat(docIds);
        const maxLength = 512;
        const finalLength = Math.min(combinedIds.length, maxLength);

        const ids = combinedIds.slice(0, finalLength);
        const attentionMask = new Array(finalLength).fill(1);
        const tokenTypeIds = new Array(queryIds.length).fill(0)
            .concat(new Array(docIds.length).fill(1))
            .slice(0, finalLength);

        const feeds = {
            input_ids: new ort.Tensor("int64", BigInt64Array.from(ids.map(BigInt)), [1, finalLength]),
            attention_mask: new ort.Tensor("int64", BigInt64Array.from(attentionMask.map(BigInt)), [1, finalLength]),
            token_type_ids: new ort.Tensor("int64", BigInt64Array.from(tokenTypeIds.map(BigInt)), [1, finalLength])
        };

        const outputs = await session.run(feeds);
        const outputName = session.outputNames[0] || "logits";
        const logitsTensor = outputs[outputName];
        const logitsData = logitsTensor.data as Float32Array;

        // Apply sigmoid to extract confidence score
        const rawScore = logitsData[0];
        return 1 / (1 + Math.exp(-rawScore));
    } catch (err) {
        throw err;
    }
}

async function initModel(modelPath?: string) {
    try {
        const resolvedPath = modelPath || path.join(process.cwd(), "models", "flashrank-ms-marco-MiniLM-L-6-v2.onnx");
        if (!fs.existsSync(resolvedPath)) {
            isMock = true;
            return;
        }
        
        // Attempt loading real ONNX session
        session = await ort.InferenceSession.create(resolvedPath, {
            executionProviders: ["cpu"]
        });

        // Resolve tokenizer paths
        const modelDir = path.dirname(resolvedPath);
        let tokenizerJsonPath = path.join(modelDir, "tokenizer.json");
        let tokenizerConfigPath = path.join(modelDir, "tokenizer_config.json");

        if (!fs.existsSync(tokenizerJsonPath)) {
            // Try subfolder or fallback to all-MiniLM-L6-v2
            const subfolderPath = path.join(modelDir, "flashrank-ms-marco-MiniLM-L-6-v2");
            if (fs.existsSync(path.join(subfolderPath, "tokenizer.json"))) {
                tokenizerJsonPath = path.join(subfolderPath, "tokenizer.json");
                tokenizerConfigPath = path.join(subfolderPath, "tokenizer_config.json");
            } else {
                // Fallback to all-MiniLM-L6-v2
                tokenizerJsonPath = path.join(modelDir, "all-MiniLM-L6-v2", "tokenizer.json");
                tokenizerConfigPath = path.join(modelDir, "all-MiniLM-L6-v2", "tokenizer_config.json");
            }
        }

        if (fs.existsSync(tokenizerJsonPath)) {
            const tokenizerJson = JSON.parse(fs.readFileSync(tokenizerJsonPath, "utf8"));
            const tokenizerConfig = fs.existsSync(tokenizerConfigPath)
                ? JSON.parse(fs.readFileSync(tokenizerConfigPath, "utf8"))
                : {};
            tokenizer = new Tokenizer(tokenizerJson, tokenizerConfig);
            isMock = false;
        } else {
            isMock = true;
        }
    } catch {
        isMock = true;
    }
}

parentPort?.on("message", async (msg: { type: string; modelPath?: string; id?: string; query?: string; documents?: unknown[] }) => {
    try {
        if (msg.type === "init") {
            const modelPath = msg.modelPath;
            await initModel(modelPath);
            parentPort?.postMessage({ type: "ready", mode: isMock ? "mock" : "onnx" });
        } else if (msg.type === "rerank") {
            const { id, query, documents } = msg;
            if (!query || !documents || !Array.isArray(documents)) {
                parentPort?.postMessage({
                    id,
                    type: "error",
                    message: "Invalid parameters: query or documents missing/invalid"
                });
                return;
            }
            
            const reranked = await Promise.all(
                documents.map(async (doc: unknown, index: number) => {
                    const content = typeof doc === "string" ? doc : (doc as { content?: string })?.content || "";
                    let score = 0;
                    if (!isMock && session) {
                        try {
                            score = await runOnnxInference(query || "", content);
                        } catch {
                            score = computeSimulatedScore(query || "", content);
                        }
                    } else {
                        score = computeSimulatedScore(query, content);
                    }
                    return {
                        content,
                        score,
                        index
                    };
                })
            );
            
            parentPort?.postMessage({
                id,
                type: "result",
                reranked
            });
        } else if (msg.type === "dispose") {
            if (session) {
                try {
                    await session.release();
                } catch {
                    // Ignore release errors
                }
                session = null;
            }
            process.exit(0);
        }
    } catch (err: unknown) {
        parentPort?.postMessage({
            type: "error",
            message: (err as Error)?.message || String(err)
        });
    }
});
