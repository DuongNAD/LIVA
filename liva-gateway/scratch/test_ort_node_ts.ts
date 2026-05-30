import * as ort from "onnxruntime-node";
import { pipeline, env } from "@huggingface/transformers";

async function run() {
    try {
        const modelPath = "node_modules/@huggingface/transformers/.cache/Xenova/all-MiniLM-L6-v2/onnx/model.onnx";
        console.log("Loading session from:", modelPath);
        
        const session = await ort.InferenceSession.create(modelPath);
        console.log("Session loaded! Input names:", session.inputNames);
        
        // Use transformers to get tokenizer
        const extractor = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2");
        const tokenizer = extractor.tokenizer;
        
        const text = "Hello world";
        const tokens = await tokenizer(text);
        console.log("Tokens:", tokens);
        
        const feeds = {
            input_ids: new ort.Tensor("int64", BigInt64Array.from(tokens.input_ids.data.map(BigInt)), tokens.input_ids.dims),
            attention_mask: new ort.Tensor("int64", BigInt64Array.from(tokens.attention_mask.data.map(BigInt)), tokens.attention_mask.dims),
            token_type_ids: new ort.Tensor("int64", new BigInt64Array(tokens.input_ids.data.length).fill(0n), tokens.input_ids.dims)
        };
        
        const outputs = await session.run(feeds);
        console.log("Outputs keys:", Object.keys(outputs));
        
        const lastHiddenState = outputs.last_hidden_state;
        console.log("lastHiddenState dims:", lastHiddenState.dims); // [batch, seq, 384]
        
        // Mean pooling
        const [batchSize, seqLength, dim] = lastHiddenState.dims;
        const data = lastHiddenState.data as Float32Array;
        const mask = tokens.attention_mask.data as Int32Array;
        
        const pooled = new Float32Array(dim);
        let validTokensCount = 0;
        
        for (let s = 0; s < seqLength; s++) {
            if (mask[s] === 1) {
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
        
        const normalized = new Float32Array(dim);
        if (norm > 0) {
            for (let d = 0; d < dim; d++) {
                normalized[d] = pooled[d] / norm;
            }
        }
        
        console.log("Success! Normalized embedding prefix:", Array.from(normalized.slice(0, 5)));
        
    } catch (e) {
        console.error("ONNX direct run failed:", e);
    }
}
run();
