import { pipeline, env } from "@huggingface/transformers";
import * as path from "node:path";
import * as fs from "node:fs";

async function run() {
    console.log("env.localModelPath:", env.localModelPath);
    console.log("env.backends.onnx:", env.backends.onnx);
    
    const cachePath = path.resolve("node_modules/@huggingface/transformers/.cache");
    console.log("Using cache path:", cachePath);
    
    // Test if we can initialize pipeline
    try {
        const extractor = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2", {
            dtype: "fp32",
            cache_dir: cachePath
        });
        const out = await extractor("Hello world", { pooling: "mean", normalize: true });
        console.log("Success! Output dim:", out.data.length);
    } catch (e) {
        console.error("Pipeline failed:", e);
    }
}
run();
